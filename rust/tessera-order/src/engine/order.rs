// SPDX-License-Identifier: BSD-2-Clause

//! Scalar order scans and stable insertion, independent of recursive execution.
//!
//! Key extractors select the scan; direct-key callers keep the insertion loop
//! inside their once-per-pass SIMD dispatch. These operations own no routing policy.

use super::thresholds::SINGLE_INSERTION_MAX;
use crate::engine::key::{Extract, Unsigned};
use core::mem::ManuallyDrop;

/// Returns the first descending pair, retaining the previous extracted key so
/// a user callback runs once per scanned record. The callback may panic or
/// change its answer; no records move during this shared scan.
#[inline]
pub(crate) fn by_key<T, E: Extract<T>>(records: &[T], extractor: &mut E) -> Option<usize> {
    let first = records.first()?;
    let mut previous = extractor.key(first);
    records.windows(2).position(|pair| {
        let current = extractor.key(&pair[1]);
        let inverted = previous > current;
        previous = current;
        inverted
    })
}

/// Stable insertion for short leaves. Insert pairs together so a predecessor
/// greater than both keys moves directly to its final slot for this pair.
#[inline(always)]
pub(crate) fn sort_short<T, E: Extract<T>>(records: &mut [T], extractor: &mut E) {
    if records.len() <= SINGLE_INSERTION_MAX {
        let [first, second, rest @ ..] = &mut *records else {
            return;
        };
        let second_key = extractor.key(second);
        if extractor.key(first) > second_key {
            if let Some(third_key) = rest.first().map(|record| extractor.key(record))
                && third_key < second_key
            {
                return finish_descending_prefix(records, extractor, third_key);
            }
            core::mem::swap(first, second);
        }
        for index in 2..records.len() {
            insert_one(&mut records[..=index], extractor);
        }
    } else {
        sort_pairs(records, extractor);
    }
}

// Keep the reversal and its record-sized temporary off the ascending path.
#[inline(never)]
fn finish_descending_prefix<T, E: Extract<T>>(
    records: &mut [T],
    extractor: &mut E,
    mut previous: E::Value,
) {
    // The caller has already found three strictly decreasing keys.
    let mut start = 3;
    for record in &records[3..] {
        let next = extractor.key(record);
        if next >= previous {
            break;
        }
        previous = next;
        start += 1;
    }
    records[..start].reverse();
    for index in start..records.len() {
        insert_one(&mut records[..=index], extractor);
    }
}

fn sort_pairs<T, E: Extract<T>>(records: &mut [T], extractor: &mut E) {
    let len = records.len();

    // An even suffix leaves no unpaired record at the end.
    let mut start = if len.is_multiple_of(2) {
        insert_one(&mut records[..2], extractor);
        2
    } else {
        1
    };
    while start < len {
        let mut low = extractor.key(&records[start]);
        let mut high = extractor.key(&records[start + 1]);
        if high < low {
            records.swap(start, start + 1);
            core::mem::swap(&mut low, &mut high);
        }
        let last = extractor.key(&records[start - 1]);
        if last > low {
            // SAFETY: the two guards own the records in adjacent holes.
            // Moving a predecessor two slots opens the next pair of holes.
            unsafe {
                let base = records.as_mut_ptr();
                let tail = base.add(start);
                let mut left = InsertionGap {
                    record: ManuallyDrop::new(tail.read()),
                    hole: tail,
                };
                let mut right = InsertionGap {
                    record: ManuallyDrop::new(tail.add(1).read()),
                    hole: tail.add(1),
                };
                while left.hole != base && extractor.key(&*left.hole.sub(1)) > high {
                    let before = left.hole.sub(1);
                    right.hole.copy_from_nonoverlapping(before, 1);
                    right.hole = left.hole;
                    left.hole = before;
                }
                drop(right);
                while left.hole != base && extractor.key(&*left.hole.sub(1)) > low {
                    let before = left.hole.sub(1);
                    left.hole.copy_from_nonoverlapping(before, 1);
                    left.hole = before;
                }
            }
        }
        start += 2;
    }
}

#[inline(always)]
fn insert_one<T, E: Extract<T>>(records: &mut [T], extractor: &mut E) {
    let index = records.len() - 1;
    let key = extractor.key(&records[index]);
    if key >= extractor.key(&records[index - 1]) {
        return;
    }
    // SAFETY: index and its predecessor are initialized. Each copy moves
    // the preceding owner into the hole before another callback can run.
    // On normal return or unwind the guard fills the one remaining hole.
    unsafe {
        let base = records.as_mut_ptr();
        let tail = base.add(index);
        let mut gap = InsertionGap {
            record: ManuallyDrop::new(tail.read()),
            hole: tail,
        };
        loop {
            let before = gap.hole.sub(1);
            gap.hole.copy_from_nonoverlapping(before, 1);
            gap.hole = before;
            if before == base || extractor.key(&*before.sub(1)) <= key {
                break;
            }
        }
    }
}

/// Sole owner of one removed record and its unique vacant slot in the slice.
struct InsertionGap<T> {
    record: ManuallyDrop<T>,
    hole: *mut T,
}

impl<T> Drop for InsertionGap<T> {
    fn drop(&mut self) {
        // SAFETY: the guard owns this record and exclusively owns the vacant
        // slot. Transferring the record back consumes its only live copy.
        unsafe { self.hole.write(ManuallyDrop::take(&mut self.record)) };
    }
}

/// Scans until all candidate bits have appeared in both states, or the input ends.
/// Only candidate bits are returned, so even changing keys cannot restore a
/// parent partition digit and prevent the recursion from making progress.
/// A nonzero candidate requires a nonempty slice.
pub(crate) fn scan_varying_bits<T, E: Extract<T>>(
    records: &[T],
    candidate: u128,
    extractor: &mut E,
) -> u128 {
    if candidate == 0 {
        return 0;
    }
    let first = extractor.key(&records[0]);
    let (mut union, mut intersection) = (first, first);
    let mut index = 1;
    let mut batch = 64;
    while index < records.len() {
        let end = index + batch.min(records.len() - index);
        for record in &records[index..end] {
            let key = extractor.key(record);
            union |= key;
            intersection = intersection & key;
        }
        let seen = union.to_u128() ^ intersection.to_u128();
        if seen & candidate == candidate {
            break;
        }
        index = end;
        // Early witnesses stay cheap; long scans get vectorizable stretches.
        batch = batch.saturating_mul(2);
    }
    (union.to_u128() ^ intersection.to_u128()) & candidate
}

/// Stably inserts inverted records until sorted or the swap budget runs out.
/// A failed pass still leaves a stable permutation of the input.
///
/// The scanner returns the earliest descending adjacent pair in its shared
/// suffix, using the same extractor where it needs user keys. Direct-key
/// callers instead capture the backend selected for this entire pass.
/// The SIMD caller enters this from a target-feature context. Keeping the
/// loop in that context avoids out-of-line calls for each vector operation.
#[inline(always)]
pub(crate) fn insertion_loop<T, E: Extract<T>>(
    records: &mut [T],
    budget: usize,
    extractor: &mut E,
    mut first_inversion: impl FnMut(&[T], &mut E) -> Option<usize>,
) -> Option<usize> {
    let mut swaps = 0;
    let mut start = 0;
    while let Some(offset) = first_inversion(&records[start..], extractor) {
        let index = start + offset + 1;
        let (prefix, tail) = records.split_at_mut(index);
        let (mut current, prefix) = prefix.split_last_mut().unwrap();
        // The scan already compared this pair, so swap without re-reading keys.
        core::mem::swap(current, &mut tail[0]);
        swaps += 1;
        if swaps > budget {
            return None;
        }
        for before in prefix.iter_mut().rev() {
            if extractor.key(before) <= extractor.key(current) {
                break;
            }
            core::mem::swap(before, current);
            current = before;
            swaps += 1;
            if swaps > budget {
                return None;
            }
        }
        start = index;
    }
    Some(swaps)
}

/// Finishes insertion without a swap limit.
///
/// The unit return keeps the unused swap total inside the inlined operation,
/// so it can be discarded before returning across a target-feature boundary.
#[inline(always)]
pub(crate) fn finish_insertion<T, E: Extract<T>>(
    records: &mut [T],
    extractor: &mut E,
    first_inversion: impl FnMut(&[T], &mut E) -> Option<usize>,
) {
    let _ = insertion_loop(records, usize::MAX, extractor, first_inversion);
}

/// Confirms ascending keys after an already encoded predecessor.
#[inline]
pub(crate) fn confirm_ascending<T, E: Extract<T>>(
    records: &[T],
    extractor: &mut E,
    mut previous: E::Value,
) -> bool {
    // Combine each group's ordering results before the rejection decision;
    // independent comparisons can overlap without promising branchless code.
    // Keys still run once, in order; an inversion may read the rest of its group.
    let (chunks, tail) = records.as_chunks::<4>();
    for chunk in chunks {
        let mut ordered = true;
        for record in chunk {
            let current = extractor.key(record);
            ordered &= previous <= current;
            previous = current;
        }
        if !ordered {
            return false;
        }
    }
    tail.iter()
        .try_fold(previous, |before, record| {
            let after = extractor.key(record);
            (before <= after).then_some(after)
        })
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::key::ByKey;
    use alloc::vec::Vec;

    #[test]
    fn confirmation_checks_group_boundaries_and_preserves_key_order() {
        for len in 0..=17 {
            for inverted in 0..=len {
                let records: Vec<_> = (0..len)
                    .map(|id| {
                        (
                            if id == inverted {
                                0u64
                            } else {
                                id as u64 / 2 + 1
                            },
                            id,
                        )
                    })
                    .collect();
                for previous in [0, 10] {
                    let expected = records.first().is_none_or(|record| previous <= record.0)
                        && records.windows(2).all(|pair| pair[0].0 <= pair[1].0);
                    let mut visited = Vec::new();
                    let actual = confirm_ascending(
                        &records,
                        &mut ByKey(|record: &(u64, usize)| {
                            visited.push(record.1);
                            record.0
                        }),
                        previous,
                    );
                    assert_eq!(actual, expected);
                    assert!(visited.iter().enumerate().all(|(index, &id)| index == id));
                }
            }
        }
    }
}
