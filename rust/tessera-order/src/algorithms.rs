// SPDX-License-Identifier: BSD-2-Clause

//! Slice operations composed from complete ordered refinement. Rank validation
//! and normalization precede execution; no adapter owns partition decisions,
//! live scratch records or a separate selection engine.

use crate::{
    engine,
    key::{ByKey, Direct, Extract, RadixKey},
    rank::{self, Requirement},
    workspace::Workspace,
};
use alloc::vec::Vec;
use core::{
    cmp::Reverse,
    ops::{Bound, Range, RangeBounds},
};

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
/// [`crate::sort_by_key`]. A key panic drops the partial index output and
/// propagates; `values` is never moved.
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

/// Sorts each chunk of `chunk_len` records, as [`crate::sort`] on every
/// `values.chunks_mut(chunk_len)`; the last chunk may be shorter.
///
/// One call reuses its workspace across chunks and releases it on return; its
/// bound is the sorting workspace of one chunk.
///
/// # Panics
/// Panics if `chunk_len` is zero, before any record is changed.
///
/// # Examples
/// ```
/// let mut values = [3u64, 1, 2, 9, 7];
/// tessera_order::sort_chunks(&mut values, 3);
/// assert_eq!(values, [1, 2, 3, 7, 9]);
/// ```
pub fn sort_chunks<K: RadixKey>(values: &mut [K], chunk_len: usize) {
    chunks(values, chunk_len, Direct);
}

/// Sorts each chunk of `chunk_len` records by a key function.
///
/// Chunks and memory follow [`sort_chunks`]; key calls follow
/// [`crate::sort_by_key`], with one `FnMut` across all chunks.
///
/// # Panics
/// Panics if `chunk_len` is zero, before any key call. A key panic restores
/// every record of the current chunk before propagating; earlier chunks are
/// sorted and later chunks unvisited.
pub fn sort_chunks_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    chunk_len: usize,
    key: F,
) {
    chunks(values, chunk_len, ByKey(key));
}

/// Sorts the records that [`crate::sort`] would place in `range`, and only
/// partitions the rest around them.
///
/// The nightly `slice::partial_sort_unstable`, with equal keys resolved as the
/// stable sort resolves them: `values[range]` equals that interval of the
/// sorted slice, and the records before and after it are exactly the sorted
/// slice's records there, in unspecified order. An empty range `k..k` with
/// `0 < k < len` partitions at `k`; at either end it changes nothing. The
/// workspace bound is that of [`crate::sort`], independent of the range length.
///
/// # Panics
/// Panics if `range` is out of bounds, as slice indexing does, before any
/// record is changed.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// tessera_order::partial_sort(&mut values, ..3);
/// assert_eq!(&values[..3], &[1, 2, 3]);
/// tessera_order::partial_sort(&mut values, 1..4);
/// assert_eq!(&values[1..4], &[2, 3, 7]);
/// ```
pub fn partial_sort<K: RadixKey>(values: &mut [K], range: impl RangeBounds<usize>) {
    partial(values, ranks(range, values.len()), Direct);
}

/// Sorts the records that [`crate::sort_by_key`] would place in `range`.
///
/// Result, bounds and memory follow [`partial_sort`]; key calls and panic
/// restoration follow [`crate::sort_by_key`]. A range that changes nothing
/// makes no key calls and allocates nothing.
///
/// # Panics
/// An out-of-bounds range panics before any key call. A key panic restores
/// every record before propagating, in unspecified order.
pub fn partial_sort_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    range: impl RangeBounds<usize>,
    key: F,
) {
    partial(values, ranks(range, values.len()), ByKey(key));
}

/// Moves the `k` smallest records to the front in sorted order and returns
/// them with the remaining records, in unspecified order.
///
/// itertools `k_smallest`, in place: `partial_sort(values, ..k)` followed by
/// `split_at_mut(k)`. Equal keys keep input order, including at the boundary.
/// For membership without ordering, use `partial_sort(values, k..k)`.
///
/// # Panics
/// Panics if `k > values.len()`, before any record is changed.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// let (smallest, rest) = tessera_order::k_smallest(&mut values, 3);
/// assert_eq!(smallest, [1, 2, 3]);
/// assert!(rest.iter().all(|v| *v >= 7));
/// ```
pub fn k_smallest<K: RadixKey>(values: &mut [K], k: usize) -> (&mut [K], &mut [K]) {
    smallest(values, k, Direct)
}

/// Moves the `k` records with the smallest keys to the front, sorted.
///
/// Result and bounds follow [`k_smallest`]; key calls and panic restoration
/// follow [`crate::sort_by_key`].
pub fn k_smallest_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    k: usize,
    key: F,
) -> (&mut [T], &mut [T]) {
    smallest(values, k, ByKey(key))
}

/// Moves the `k` largest records to the front in descending order and returns
/// them with the remaining records, in unspecified order.
///
/// itertools `k_largest`, in place: [`k_smallest`] with a
/// [`Reverse`](core::cmp::Reverse) key, so equal keys keep input order.
///
/// # Panics
/// Panics if `k > values.len()`, before any record is changed.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// let (largest, _) = tessera_order::k_largest(&mut values, 2);
/// assert_eq!(largest, [9, 7]);
/// ```
pub fn k_largest<K: RadixKey>(values: &mut [K], k: usize) -> (&mut [K], &mut [K]) {
    smallest(values, k, ByKey(|value: &K| Reverse(*value)))
}

/// Moves the `k` records with the largest keys to the front, descending.
///
/// Result and bounds follow [`k_largest`]; key calls and panic restoration
/// follow [`crate::sort_by_key`].
pub fn k_largest_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    k: usize,
    mut key: F,
) -> (&mut [T], &mut [T]) {
    smallest(values, k, ByKey(|record: &T| Reverse(key(record))))
}

/// Places the record that [`crate::sort`] would place at `index`, and
/// partitions the rest around it.
///
/// `slice::select_nth_unstable`, with equal keys resolved as the stable sort
/// resolves them: the selected record is that sort's record at `index`, and the
/// two outer regions are exactly its records before and after, in unspecified
/// order. The workspace bound is that of [`crate::sort`].
///
/// # Panics
/// Panics if `index >= values.len()`, before any record is changed.
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
    select(values, index, Direct)
}

/// Places the record that [`crate::sort_by_key`] would place at `index`.
///
/// Result and memory follow [`select_nth`]; key calls and panic restoration
/// follow [`crate::sort_by_key`].
///
/// # Panics
/// An invalid index panics before any key call. A key panic restores every
/// record before propagating, in unspecified order.
pub fn select_nth_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    index: usize,
    key: F,
) -> (&mut [T], &mut T, &mut [T]) {
    select(values, index, ByKey(key))
}

/// Places the records that [`crate::sort`] would place at each of `indices`,
/// and partitions the rest between them.
///
/// `numpy.partition(kth=indices)`, with equal keys resolved as the stable sort
/// resolves them: every gap between selected positions holds exactly that
/// sort's records there, in unspecified order. Indices may be unordered and
/// repeated. Memory is the sorting workspace plus O(m) for m indices.
///
/// # Panics
/// Panics if any index is out of bounds, before any record is changed.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// tessera_order::select_nth_many(&mut values, &[3, 1, 3]);
/// assert_eq!(values[1], 2);
/// assert_eq!(values[3], 7);
/// ```
pub fn select_nth_many<K: RadixKey>(values: &mut [K], indices: &[usize]) {
    select_many(values, indices, Direct);
}

/// Places the records that [`crate::sort_by_key`] would place at each index.
///
/// Result and memory follow [`select_nth_many`]; key calls and panic
/// restoration follow [`crate::sort_by_key`]. No indices make no key calls.
///
/// # Panics
/// All indices are validated before any key call. A key panic restores every
/// record before propagating, in unspecified order.
pub fn select_nth_many_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    indices: &[usize],
    key: F,
) {
    select_many(values, indices, ByKey(key));
}

/// Resolves `range` against `len` with slice-indexing bounds.
fn ranks(range: impl RangeBounds<usize>, len: usize) -> Range<usize> {
    let start = match range.start_bound() {
        Bound::Included(&start) => Some(start),
        Bound::Excluded(&start) => start.checked_add(1),
        Bound::Unbounded => Some(0),
    };
    let end = match range.end_bound() {
        Bound::Included(&end) => end.checked_add(1),
        Bound::Excluded(&end) => Some(end),
        Bound::Unbounded => Some(len),
    };
    match (start, end) {
        (Some(start), Some(end)) if start <= end && end <= len => start..end,
        _ => panic!("rank range out of bounds"),
    }
}

/// Requests ordered positions or an empty range's membership boundary.
fn partial<T, E: Extract<T>>(values: &mut [T], ranks: Range<usize>, extractor: E) {
    let required = Requirement::new(values.len(), core::slice::from_ref(&ranks));
    engine::refine(values, extractor, required, None);
}

fn smallest<T, E: Extract<T>>(values: &mut [T], k: usize, extractor: E) -> (&mut [T], &mut [T]) {
    assert!(k <= values.len(), "k out of bounds");
    partial(values, 0..k, extractor);
    values.split_at_mut(k)
}

fn select<T, E: Extract<T>>(
    values: &mut [T],
    index: usize,
    extractor: E,
) -> (&mut [T], &mut T, &mut [T]) {
    assert!(index < values.len(), "selection index out of bounds");
    partial(values, index..index + 1, extractor);
    let (before, rest) = values.split_at_mut(index);
    let (selected, after) = rest.split_first_mut().unwrap();
    (before, selected, after)
}

fn select_many<T, E: Extract<T>>(values: &mut [T], indices: &[usize], extractor: E) {
    let ranges = rank::normalize_indices(values.len(), indices);
    let required = Requirement::new(values.len(), &ranges);
    engine::refine(values, extractor, required, None);
}

fn chunks<T, E: Extract<T>>(values: &mut [T], chunk_len: usize, mut extractor: E) {
    assert!(chunk_len != 0, "chunk size must be non-zero");
    let mut workspace = Workspace::new();
    for chunk in values.chunks_mut(chunk_len) {
        let ranks = 0..chunk.len();
        let required = Requirement::new(chunk.len(), core::slice::from_ref(&ranks));
        engine::refine(chunk, &mut extractor, required, Some(&mut workspace));
    }
}
