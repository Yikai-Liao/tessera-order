// SPDX-License-Identifier: BSD-2-Clause

//! Insertion passes: the one loop that finishes small and nearly ordered slices.
//!
//! A pass is a stable insertion sort that stops after a swap budget. It finds
//! inversions with an [`Adjacency`] kernel, so the same loop serves a user key
//! function, a record that is its own key, and a vector backend selected once
//! per pass.

use crate::key::{Extract, Unsigned};
use crate::scan::{self, SimdKey};
use fearless_simd::{Level, dispatch};

/// A key source for one insertion pass.
pub(crate) trait Adjacency<T> {
    /// The encoded key type.
    type Value: Unsigned;

    /// Returns the encoded key of `record`.
    fn key(&mut self, record: &T) -> Self::Value;

    /// Returns the offset of the first adjacent pair whose keys descend.
    fn inversion(&mut self, records: &[T]) -> Option<usize>;
}

/// Adjacency that reads keys through an extractor, one pair at a time.
pub(crate) struct Keyed<'a, E>(pub(crate) &'a mut E);

impl<T, E: Extract<T>> Adjacency<T> for Keyed<'_, E> {
    type Value = E::Value;

    #[inline]
    fn key(&mut self, record: &T) -> Self::Value {
        self.0.key(record)
    }

    #[inline]
    fn inversion(&mut self, records: &[T]) -> Option<usize> {
        scan::scalar_by(records, self.0)
    }
}

/// Adjacency over records that are their own keys, scanned one pair at a time.
pub(crate) struct Sealed;

impl<K: crate::key::RadixKey> Adjacency<K> for Sealed {
    type Value = K::Encoded;

    #[inline]
    fn key(&mut self, record: &K) -> Self::Value {
        record.encode()
    }

    #[inline]
    fn inversion(&mut self, records: &[K]) -> Option<usize> {
        scan::scalar(records)
    }
}

/// Sorts `records` stably by insertion within `budget` swaps.
///
/// Returns the number of swaps, or `None` once the budget runs out; the
/// records are then a stable permutation of their input.
///
/// The loop is inlined so a vector kernel selected for the pass also inlines its
/// scan. Expanding the inversion first and then shifting keeps the scan's
/// adjacency knowledge: the pair it compared is swapped without re-reading keys.
#[inline(always)]
pub(crate) fn run<T, A: Adjacency<T>>(
    records: &mut [T],
    budget: usize,
    kernel: &mut A,
) -> Option<usize> {
    let mut swaps = 0;
    let mut start = 0;
    while let Some(offset) = kernel.inversion(&records[start..]) {
        let index = start + offset + 1;
        let (prefix, tail) = records.split_at_mut(index);
        let (mut current, prefix) = prefix.split_last_mut().unwrap();
        core::mem::swap(current, &mut tail[0]);
        swaps += 1;
        if swaps > budget {
            return None;
        }
        for before in prefix.iter_mut().rev() {
            if kernel.key(before) <= kernel.key(current) {
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

/// Runs one insertion pass over records that are their own keys, with the
/// vector kernel for `K` selected once for the whole pass.
///
/// Dispatching per scan instead of per pass repeats the level check on every
/// moved record, and a short slice never reaches the vector kernel at all.
pub(crate) fn run_vector<K: SimdKey>(records: &mut [K], budget: usize) -> Option<usize> {
    if records.len() < scan::SIMD_MIN {
        return run(records, budget, &mut Sealed);
    }
    dispatch!(Level::new(), simd => run(records, budget, &mut scan::VectorScan::new(simd)))
}

/// Finishes a nearly ordered slice with a budgeted insertion pass.
///
/// The pass first insertion-sorts a window of `window` records within a budget
/// of one swap per window record, then scales the swaps that window needed to
/// the whole slice. A window that runs out of budget says the input is too
/// disordered to be worth finishing, and one that needs no swaps says nothing
/// about the records behind it, so the slice is left alone in both cases.
///
/// Returns whether the slice is now sorted. A slice left unfinished is a stable
/// permutation of its input.
pub(crate) fn probe<T, E: Extract<T>>(records: &mut [T], window: usize, extractor: &mut E) -> bool {
    let window = window.clamp(2, records.len().saturating_sub(1));
    match extractor.insertion_sort(&mut records[..window], window) {
        None | Some(0) => false,
        Some(swaps) => {
            // The window finished within its own budget, so the scaled budget
            // fits `usize` even when the intermediate product does not.
            let budget = (swaps as u128 * records.len() as u128 / window as u128) as usize;
            extractor.insertion_sort(records, budget).is_some()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_finishes_near_sorted_slices() {
        let near = |len: u64| {
            let mut values: Vec<u64> = (0..len).collect();
            for index in (1..len as usize - 1).step_by(64) {
                values.swap(index, index + 1);
            }
            values
        };
        for len in [64u64, 1000, 65536] {
            let mut values = near(len);
            assert!(probe(&mut values, 64, &mut crate::key::Direct));
            assert_eq!(values, (0..len).collect::<Vec<_>>());

            // A highly disordered slice keeps every record and gives up early.
            let mut values: Vec<u64> = (0..len).map(|id| (id * 7919) % 1024).collect();
            let mut expected = values.clone();
            expected.sort_unstable();
            let _ = probe(&mut values, 64, &mut crate::key::Direct);
            values.sort_unstable();
            assert_eq!(values, expected, "len={len}");
        }
    }
}
