// SPDX-License-Identifier: BSD-2-Clause

//! Public ordering operations over the private engine.
//!
//! These entry points choose the extractor or build the operation's index or
//! key buffer. Engine operations own their workspace and record recovery.

pub(super) mod cached;
#[cfg(feature = "rayon")]
pub(super) mod parallel;

use crate::engine::{self, ByKey, Direct, RadixKey};
use alloc::vec::Vec;

/// Sorts keys in ascending order, keeping equal keys in input order.
///
/// The order is the encoded order documented on [`RadixKey`]: integers in their
/// usual order, floats as [`f64::total_cmp`], and [`Reverse`](core::cmp::Reverse)
/// keys descending.
///
/// # Memory
///
/// Allocates the scratch described in the [crate documentation](crate#memory)
/// and frees it before returning. Inputs that are already ordered or constant
/// do not allocate.
///
/// # Examples
///
/// ```
/// let mut values = [0.5f64, -0.0, f64::NAN, 0.0, -1.0];
/// tessera_order::sort(&mut values);
/// let bits: Vec<u64> = values.iter().map(|x| x.to_bits()).collect();
/// let expected = [-1.0f64, -0.0, 0.0, 0.5, f64::NAN].map(f64::to_bits);
/// assert_eq!(bits, expected);
/// ```
pub fn sort<K: RadixKey>(values: &mut [K]) {
    engine::sort::<K, _>(values, Direct);
}

/// Sorts records by a key, keeping records with equal keys in input order.
///
/// `key` maps a record to a [`RadixKey`]. Records of any type are moved, never
/// cloned or dropped, so `String`, `Box<T>` and `&mut T` records are fine.
///
/// # Key function
///
/// `key` may be called any number of times per record, in any order. It must
/// return the same key for a record throughout the call to obtain a stable
/// sorted result. Inconsistent keys may cause a panic or a return with the
/// result in unspecified order; either way every record remains in the slice.
///
/// # Panics
///
/// If `key` unwinds, the panic propagates after `values` again holds every
/// original record exactly once, in an unspecified order. Changes made
/// through interior mutability remain visible; side effects are not rolled
/// back. Recovery does not call `key`.
///
/// # Memory
///
/// As [`sort`], plus up to two bytes per tile record of cached bucket
/// IDs.
///
/// # Examples
///
/// ```
/// struct Row {
///     timestamp: i64,
///     name: String,
/// }
///
/// let mut rows = vec![
///     Row { timestamp: 30, name: "c".into() },
///     Row { timestamp: 10, name: "a".into() },
///     Row { timestamp: 30, name: "d".into() },
/// ];
/// tessera_order::sort_by_key(&mut rows, |row| row.timestamp);
/// let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
/// assert_eq!(names, ["a", "c", "d"]);
/// ```
pub fn sort_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(values: &mut [T], key: F) {
    engine::sort::<T, _>(values, ByKey(key));
}

/// Renamed to [`sort`]; the contract is unchanged.
#[deprecated(since = "0.2.0", note = "renamed to `sort`")]
pub fn stable_sort<K: RadixKey>(values: &mut [K]) {
    sort(values);
}

/// Renamed to [`sort_by_key`]; the contract is unchanged.
#[deprecated(since = "0.2.0", note = "renamed to `sort_by_key`")]
pub fn stable_sort_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(values: &mut [T], key: F) {
    sort_by_key(values, key);
}

/// Returns the indices that would sort `values`, without moving them.
///
/// `numpy.argsort(kind="stable")`: `values[result[i]]` is the `i`th key of
/// [`sort`], and equal keys keep ascending index order. The O(n) index
/// result is output space; sorting it uses the sorting workspace in addition.
///
/// # Examples
/// ```
/// let values = [3u64, 1, 3, 2];
/// assert_eq!(tessera_order::argsort(&values), [1, 3, 0, 2]);
/// ```
pub fn argsort<K: RadixKey>(values: &[K]) -> Vec<usize> {
    engine::argsort(values, Direct)
}

/// Returns the indices that would sort `values` by a key function.
///
/// Result and memory follow [`argsort`]; key calls follow
/// [`sort_by_key`]. If keys are inconsistent, the call may panic or
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
pub fn argsort_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(values: &[T], key: F) -> Vec<usize> {
    engine::argsort(values, ByKey(key))
}

/// Places the record that [`sort`] would place at `index`, and
/// partitions the rest around it.
///
/// Like [`slice::select_nth_unstable`], with equal keys resolved as the stable sort
/// resolves them: the selected record is that sort's record at `index`, and the
/// two outer regions contain exactly its records before and after. Equal keys
/// retain their input order throughout the slice. Records with different
/// keys may appear in any order within a region. The workspace bound is
/// that of [`sort`].
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

/// Places the record that [`sort_by_key`] would place at `index`.
///
/// Result and memory follow [`select_nth`]; key calls follow
/// [`sort_by_key`]. If keys are inconsistent, the call may panic or
/// return without guarantees about which records occupy the three regions.
///
/// # Panics
///
/// Panics if `index >= values.len()`, including on an empty slice, before
/// calling `key` or moving any records. If `key` unwinds, the panic propagates
/// after all original records are back in `values` exactly once, in an
/// unspecified order. As with [`sort_by_key`], changes made through
/// interior mutability remain visible, and recovery does not call `key`.
pub fn select_nth_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    index: usize,
    key: F,
) -> (&mut [T], &mut T, &mut [T]) {
    engine::select_nth(values, index, ByKey(key))
}
