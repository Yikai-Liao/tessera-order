// SPDX-License-Identifier: BSD-2-Clause

//! Scalar order scans and stable insertion, independent of recursive execution.
//!
//! Key extractors select the scan; direct-key callers keep the insertion loop
//! inside their once-per-pass SIMD dispatch. These operations own no routing policy.

use crate::key::{Extract, Unsigned};

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

/// Stably inserts inverted records until sorted or the swap budget runs out.
/// A failed pass still leaves a stable permutation of the input.
///
/// The scanner returns the earliest descending adjacent pair in its shared
/// suffix, using the same extractor where it needs user keys. Direct-key
/// callers instead capture the backend selected for this entire pass.
/// The SIMD caller enters this from a target-feature context. The probe and
/// continuation share one scan call site: separate loops left the vector scan
/// out of that context, with out-of-line calls for each vector operation.
#[inline(always)]
fn insertion_loop<T, E: Extract<T>>(
    records: &mut [T],
    mut scan_end: usize,
    mut budget: usize,
    extractor: &mut E,
    mut first_inversion: impl FnMut(&[T], &mut E) -> Option<usize>,
) -> Option<usize> {
    let mut swaps = 0;
    let mut start = 0;
    loop {
        let Some(offset) = first_inversion(&records[start..scan_end], extractor) else {
            if scan_end == records.len() {
                return Some(swaps);
            }
            if swaps == 0 {
                return None;
            }
            budget = scaled_probe_budget(swaps, records.len(), scan_end);
            start = scan_end - 1;
            scan_end = records.len();
            continue;
        };
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
}

/// Computes the full-input budget once after a successful prefix probe.
///
/// Keep the wider multiplication outside the insertion loop: otherwise LLVM
/// can hoist it into every scan, even when the probe never reaches this point.
#[inline(never)]
fn scaled_probe_budget(swaps: usize, records_len: usize, probe_len: usize) -> usize {
    // The successful probe bounds swaps by probe_len, so the quotient fits
    // usize even when the intermediate product does not.
    (swaps as u128 * records_len as u128 / probe_len as u128) as usize
}

/// Tries a bounded insertion pass, scaling a nonzero probe to the full input.
/// A successful probe has already sorted its prefix, so the same pass resumes
/// at its boundary instead of extracting and sorting those records again.
#[inline(always)]
pub(crate) fn try_insertion<T, E: Extract<T>>(
    records: &mut [T],
    tile_len: usize,
    extractor: &mut E,
    first_inversion: impl FnMut(&[T], &mut E) -> Option<usize>,
) -> bool {
    let probe_len = tile_len.max(2).min(records.len());
    insertion_loop(
        records,
        probe_len,
        probe_len / 4,
        extractor,
        first_inversion,
    )
    .is_some()
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
    let _ = insertion_loop(
        records,
        records.len(),
        usize::MAX,
        extractor,
        first_inversion,
    );
}

/// Confirms ascending keys after an already encoded predecessor.
#[inline]
pub(crate) fn confirm_ascending<T, E: Extract<T>>(
    records: &[T],
    extractor: &mut E,
    previous: E::Value,
) -> bool {
    records
        .iter()
        .try_fold(previous, |before, record| {
            let after = extractor.key(record);
            (before <= after).then_some(after)
        })
        .is_some()
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::key::{ByKey, Direct};
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn insertion_loop_respects_arbitrary_budgets() {
        for len in [0usize, 1, 48, 63, 64, 65, if cfg!(miri) { 97 } else { 500 }] {
            for budget in [0, 10, usize::MAX] {
                let mut keys: Vec<_> = (0..len as u64).rev().collect();
                let swaps = len * len.saturating_sub(1) / 2;
                assert_eq!(
                    insertion_loop(&mut keys, len, budget, &mut Direct, by_key),
                    (swaps <= budget).then_some(swaps),
                );
                keys.sort_unstable();
                assert_eq!(keys, (0..len as u64).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn scaled_probe_continues_from_its_sorted_prefix() {
        let (len, tile_len, step) = if cfg!(miri) {
            (193, 64, 8)
        } else {
            (65536, 8192, 100)
        };
        let mut keys: Vec<u64> = (0..len).collect();
        for index in (0..keys.len() - 1).step_by(step) {
            keys.swap(index, index + 1);
        }
        assert!(Direct.try_insertion(&mut keys, tile_len));
        assert!(keys.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn probe_and_continuation_keep_stable_record_identity() {
        let cases: &[(&[u64], bool)] = &[
            // Two prefix swaps predict a total of four; the suffix key 5
            // crosses the boundary and stops after the earlier equal key.
            (&[0, 1, 3, 2, 4, 5, 7, 6, 5, 8, 9, 10, 11, 12, 13, 14], true),
            // An initially ordered prefix does not predict the suffix.
            (
                &[0, 1, 2, 3, 4, 5, 6, 7, 0, 8, 9, 10, 11, 12, 13, 14],
                false,
            ),
            // The suffix exceeds the scaled total budget.
            (
                &[0, 1, 3, 2, 4, 5, 7, 6, 0, 8, 9, 10, 11, 12, 13, 14],
                false,
            ),
            // The initial window itself exceeds its budget.
            (
                &[7, 6, 5, 4, 3, 2, 1, 0, 8, 9, 10, 11, 12, 13, 14, 15],
                false,
            ),
        ];
        for &(keys, finished) in cases {
            let mut records: Vec<_> = keys.iter().copied().zip(0..).collect();
            let mut expected = records.clone();
            expected.sort_by_key(|record| record.0);
            let mut extractor = ByKey(|record: &(u64, usize)| record.0);
            assert_eq!(extractor.try_insertion(&mut records, 8), finished);
            if finished {
                assert_eq!(records, expected);
            }
            records.sort_by_key(|record| record.0);
            assert_eq!(records, expected);
        }
    }

    #[test]
    fn continuation_panic_keeps_borrowed_records_after_prefix_moves() {
        let keys = [0u64, 1, 3, 2, 4, 5, 7, 6, 5, 8, 9, 10, 11, 12, 13, 14];
        let mut values: Vec<_> = (0..keys.len()).collect();
        let mut records: Vec<_> = values
            .iter_mut()
            .enumerate()
            .map(|(id, value)| (keys[id], id, value))
            .collect();
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut extractor = ByKey(|record: &(u64, usize, &mut usize)| {
                assert_ne!(record.1, 8, "suffix key panicked");
                record.0
            });
            extractor.try_insertion(&mut records, 8);
        }));
        assert!(result.is_err());
        assert!(records[..8].windows(2).all(|pair| pair[0].0 <= pair[1].0));
        records.sort_by_key(|record| record.1);
        for (id, record) in records.iter_mut().enumerate() {
            assert_eq!(record.1, id);
            assert_eq!(*record.2, id);
            *record.2 += keys.len();
        }
        drop(records);
        assert_eq!(values, (keys.len()..2 * keys.len()).collect::<Vec<_>>());
    }
}
