// SPDX-License-Identifier: BSD-2-Clause

//! Stable index output and single-rank selection. Movement stays in the engine.

use crate::engine::{self, ByKey, Direct, RadixKey};
use alloc::vec::Vec;

/// Returns the indices that would sort `values`, without moving them.
///
/// `numpy.argsort(kind="stable")`: `values[result[i]]` is the `i`th key of
/// [`crate::sort`], and equal keys keep ascending index order. The O(n) index
/// result is output space; sorting it uses the sorting workspace in addition.
///
/// # Examples
/// ```
/// let values = [3u64, 1, 3, 2];
/// assert_eq!(tessera_order::argsort(&values), [1, 3, 0, 2]);
/// ```
pub fn argsort<K: RadixKey>(values: &[K]) -> Vec<usize> {
    argsort_by_key(values, |key| *key)
}

/// Returns the indices that would sort `values` by a key function.
///
/// Result and memory follow [`argsort`]; key calls follow
/// [`crate::sort_by_key`]. If keys are inconsistent, the call may panic or
/// return a permutation of the source indices in an unspecified order.
///
/// # Panics
///
/// If `key` unwinds, the partial index output is discarded and the panic
/// propagates. The source records are never moved, and changes made through
/// interior mutability remain visible in `values`.
///
/// # Examples
/// ```
/// let rows = [(2u32, "first"), (1, "middle"), (2, "last")];
/// assert_eq!(tessera_order::argsort_by_key(&rows, |r| r.0), [1, 0, 2]);
/// ```
pub fn argsort_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(values: &[T], mut key: F) -> Vec<usize> {
    let mut indices: Vec<_> = (0..values.len()).collect();
    // Indices are payloads, not direct keys: counting/refilling indices would
    // destroy their identity when source keys repeat.
    engine::sort(&mut indices, ByKey(|index: &usize| key(&values[*index])));
    indices
}

/// Places the record that [`crate::sort`] would place at `index`, and
/// partitions the rest around it.
///
/// Like [`slice::select_nth_unstable`], with equal keys resolved as the stable sort
/// resolves them: the selected record is that sort's record at `index`, and the
/// two outer regions are exactly its records before and after, in unspecified
/// order. The workspace bound is that of [`crate::sort`].
///
/// # Panics
///
/// Panics if `index >= values.len()`, including on an empty slice, before
/// moving any records.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// let (before, selected, after) = tessera_order::select_nth(&mut values, 2);
/// assert_eq!(*selected, 3);
/// assert!(before.iter().all(|v| *v < 3));
/// assert!(after.iter().all(|v| *v > 3));
/// ```
pub fn select_nth<K: RadixKey>(values: &mut [K], index: usize) -> (&mut [K], &mut K, &mut [K]) {
    engine::select_nth(values, index, Direct)
}

/// Places the record that [`crate::sort_by_key`] would place at `index`.
///
/// Result and memory follow [`select_nth`]; key calls follow
/// [`crate::sort_by_key`]. If keys are inconsistent, the call may panic or
/// return without guarantees about which records occupy the three regions.
///
/// # Panics
///
/// Panics if `index >= values.len()`, including on an empty slice, before
/// calling `key` or moving any records. If `key` unwinds, the panic propagates
/// after all original records are back in `values` exactly once, in an
/// unspecified order. As with [`crate::sort_by_key`], changes made through
/// interior mutability remain visible, and recovery does not call `key`.
pub fn select_nth_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    index: usize,
    key: F,
) -> (&mut [T], &mut T, &mut [T]) {
    engine::select_nth(values, index, ByKey(key))
}
