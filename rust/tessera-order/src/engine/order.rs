// SPDX-License-Identifier: BSD-2-Clause

//! Scalar order scans and stable insertion, independent of recursive execution.
//!
//! Key extractors select the scan; direct-key callers keep the insertion loop
//! inside their once-per-pass SIMD dispatch. These operations own no routing policy.

use crate::engine::key::{Extract, Unsigned};

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

/// Holds a moved-out record while insertion shifts predecessors right,
/// restoring it to the vacated slot on normal completion, budget exit or unwind.
struct Hole<T> {
    value: core::mem::ManuallyDrop<T>,
    dest: *mut T,
}

impl<T> Drop for Hole<T> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: `dest` is the single vacated slot in the active slice and
        // `value` holds the moved-out record that restores ownership.
        unsafe {
            self.dest
                .write(core::mem::ManuallyDrop::take(&mut self.value))
        };
    }
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
        // The scanner is safe code; enforce the pair bound the moves below rely on.
        assert!(
            index < records.len(),
            "inversion scanner returned an unpaired offset"
        );
        let base = records.as_mut_ptr();
        // SAFETY: the assertion gives `0 < index < records.len()`.
        // Lifting `index` and moving `index - 1` into `index` leaves `index - 1` as
        // the sole vacated slot, which `Hole::drop` restores on any exit or unwind.
        let mut hole = unsafe {
            let dest = base.add(index - 1);
            let value = core::mem::ManuallyDrop::new(base.add(index).read());
            core::ptr::copy_nonoverlapping(dest, base.add(index), 1);
            Hole { value, dest }
        };
        swaps += 1;
        if swaps > budget {
            return None;
        }
        while hole.dest > base {
            // SAFETY: `hole.dest > base` and stays within `records`, so `prev` is an
            // initialized slot distinct from `hole.dest`; the borrow ends before writes.
            let (prev, prev_key) = unsafe {
                let prev = hole.dest.sub(1);
                (prev, extractor.key(&*prev))
            };
            if prev_key <= extractor.key(&hole.value) {
                break;
            }
            // SAFETY: `prev` is initialized and `hole.dest` (`prev + 1`) is the sole
            // vacated slot; updating `hole.dest` restores the drop invariant immediately.
            unsafe {
                core::ptr::copy_nonoverlapping(prev, hole.dest, 1);
                hole.dest = prev;
            }
            swaps += 1;
            if swaps > budget {
                return None;
            }
        }
        drop(hole);
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
    fn insertion_restores_the_hole_on_budget_exit_and_panic() {
        let input = [1u64, 2, 3, 0];
        let after_inner_shift = [1, 0, 2, 3];

        let mut records = input;
        let mut extract = ByKey(|key: &u64| *key);
        assert_eq!(insertion_loop(&mut records, 1, &mut extract, by_key), None);
        assert_eq!(records, after_inner_shift);

        let mut records = input;
        let mut saw_one = false;
        let panic = crate::engine::unwind::catch(|| {
            let mut extract = ByKey(|key: &u64| {
                if *key == 1 {
                    assert!(!saw_one, "key failed");
                    saw_one = true;
                }
                *key
            });
            let _ = insertion_loop(&mut records, usize::MAX, &mut extract, by_key);
        })
        .unwrap_err();
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"key failed"));
        assert_eq!(records, after_inner_shift);
    }

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
