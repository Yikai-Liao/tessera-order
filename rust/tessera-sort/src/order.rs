// SPDX-License-Identifier: BSD-2-Clause

//! Scalar order scans and stable insertion, independent of recursive execution.
//!
//! Key extractors select the scan; direct-key callers keep the insertion loop
//! inside their once-per-pass SIMD dispatch. These operations own no routing policy.

use crate::key::Extract;

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
