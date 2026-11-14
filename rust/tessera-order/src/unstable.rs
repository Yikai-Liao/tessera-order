// SPDX-License-Identifier: BSD-2-Clause

//! Weaker tie contracts over the same complete ordered-refinement operations.
//! Public calls express results, never a choice of movement algorithm.

use crate::{
    algorithms,
    key::{ByKey, Direct, RadixKey},
};
use core::ops::Range;

/// Partitions the smallest `k` records into a prefix, without sorting them.
///
/// Returns disjoint borrows of the prefix and remainder. Every prefix key is
/// no greater than every remainder key. Equal-key boundary members may be
/// chosen in any order, unlike [`stable_top_k`](crate::stable_top_k).
/// Zero and full-length requests are no-ops without key calls or allocation.
/// For a sorted prefix use [`unstable_sort_prefix`].
///
/// # Memory
/// Invocation-owned workspace is bounded by the complete sorting workspace,
/// not O(k); there is no allocated output. Storage is released on return.
///
/// # Panics
/// Panics before mutation if `k > values.len()`.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// let (smallest, remaining) = tessera_order::unstable_top_k(&mut values, 3);
/// smallest.sort_unstable();
/// assert_eq!(smallest, [1, 2, 3]);
/// assert!(remaining.iter().all(|v| *v >= 7));
/// ```
pub fn unstable_top_k<K: RadixKey>(values: &mut [K], k: usize) -> (&mut [K], &mut [K]) {
    algorithms::top_k::<_, _, true>(values, k, Direct)
}

/// Partitions the smallest `k` records by key, with unspecified tie membership.
///
/// Result, bounds and memory follow [`unstable_top_k`]. Key evaluation and
/// permutation/panic guarantees follow [`crate::unstable_sort_by_key`].
///
/// # Panics
/// Invalid counts panic before mutation or key calls. A key panic restores
/// every record before unwinding, without promising a valid partial result.
pub fn unstable_top_k_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    k: usize,
    key: F,
) -> (&mut [T], &mut [T]) {
    algorithms::top_k::<_, _, true>(values, k, ByKey(key))
}

/// Sorts the smallest `prefix_len` records into an ordered prefix.
///
/// Prefix keys match those of a complete sort; ties may be chosen and ordered
/// arbitrarily. The suffix is not sorted. Zero is a no-op without key calls or
/// allocation; full length is a complete unstable sort. Workspace follows
/// [`crate::unstable_sort`], not `O(prefix_len)`.
///
/// # Panics
/// Panics before mutation if `prefix_len > values.len()`.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// tessera_order::unstable_sort_prefix(&mut values, 3);
/// assert_eq!(&values[..3], &[1, 2, 3]);
/// ```
pub fn unstable_sort_prefix<K: RadixKey>(values: &mut [K], prefix_len: usize) {
    algorithms::refine_range::<_, _, true>(values, 0..prefix_len, Direct);
}

/// Produces a sorted prefix by key, with unspecified equal-key members.
///
/// Result, bounds and memory follow [`unstable_sort_prefix`]. Key evaluation
/// and panic restoration follow [`crate::unstable_sort_by_key`].
///
/// # Panics
/// Invalid lengths panic before mutation or key calls. A key panic restores all
/// records before unwinding, without promising a valid partial result.
pub fn unstable_sort_prefix_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    prefix_len: usize,
    key: F,
) {
    algorithms::refine_range::<_, _, true>(values, 0..prefix_len, ByKey(key));
}

/// Places the key at `index` from a complete sort, with unspecified ties.
///
/// Returns disjoint mutable borrows of the preceding records, selected record
/// and following records. Every preceding key is <= the selected key and every
/// following key is >= it. Neither outer region is sorted. Equal-key record
/// identity is unspecified. Workspace follows [`crate::unstable_sort`].
///
/// # Panics
/// Panics before mutation if `index >= values.len()`.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// let (before, selected, after) = tessera_order::unstable_select_nth(&mut values, 2);
/// assert_eq!(*selected, 3);
/// assert!(before.iter().all(|v| *v <= *selected));
/// assert!(after.iter().all(|v| *v >= *selected));
/// ```
pub fn unstable_select_nth<K: RadixKey>(
    values: &mut [K],
    index: usize,
) -> (&mut [K], &mut K, &mut [K]) {
    algorithms::select_nth::<_, _, true>(values, index, Direct)
}

/// Selects a ranked key and separates its outer regions, without tie identity.
///
/// Result, bounds and memory follow [`unstable_select_nth`]. Key evaluation
/// and panic restoration follow [`crate::unstable_sort_by_key`].
///
/// # Panics
/// Invalid indices panic before mutation or key calls. A key panic restores all
/// records before unwinding, without promising a valid partial result.
pub fn unstable_select_nth_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    index: usize,
    key: F,
) -> (&mut [T], &mut T, &mut [T]) {
    algorithms::select_nth::<_, _, true>(values, index, ByKey(key))
}

/// Sorts a global rank interval, with unspecified equal-key record identities.
///
/// Keys in `values[ranks.clone()]` match that interval in a complete sort.
/// Outer regions contain their corresponding ranked keys, without internal
/// ordering. These are ranks, not original subslice positions. An empty range
/// is a no-op and establishes no outer boundary. Workspace follows
/// [`crate::unstable_sort`].
///
/// # Panics
/// Panics before mutation if `start > end` or `end > values.len()`.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// tessera_order::unstable_sort_rank_range(&mut values, 1..4);
/// assert_eq!(&values[1..4], &[2, 3, 7]);
/// ```
pub fn unstable_sort_rank_range<K: RadixKey>(values: &mut [K], ranks: Range<usize>) {
    algorithms::refine_range::<_, _, true>(values, ranks, Direct);
}

/// Sorts a global rank interval by key, without preserving tie identities.
///
/// Result, bounds and memory follow [`unstable_sort_rank_range`]. Key evaluation
/// and panic restoration follow [`crate::unstable_sort_by_key`].
///
/// # Panics
/// Invalid ranges panic before mutation or key calls. A key panic restores all
/// records before unwinding, without promising a valid partial result.
pub fn unstable_sort_rank_range_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    ranks: Range<usize>,
    key: F,
) {
    algorithms::refine_range::<_, _, true>(values, ranks, ByKey(key));
}

/// Places each requested rank's key, with unspecified equal-key record identity.
///
/// Every gap contains its corresponding ranked keys in unspecified order.
/// Indices may be unordered or repeated; normalization happens once inside the
/// call. Empty requests are no-ops without keys or allocation. Workspace is the
/// complete sort's bound plus O(m) request metadata for m supplied indices.
///
/// # Panics
/// Panics before mutation if any index is outside the input.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// tessera_order::unstable_select_nth_many(&mut values, &[3, 1, 3]);
/// assert_eq!(values[1], 2);
/// assert_eq!(values[3], 7);
/// ```
pub fn unstable_select_nth_many<K: RadixKey>(values: &mut [K], indices: &[usize]) {
    algorithms::select_many::<_, _, true>(values, indices, Direct);
}

/// Selects multiple ranked keys without promising equal-key record identities.
///
/// Result, normalization and memory follow [`unstable_select_nth_many`]. Key
/// evaluation and panic restoration follow [`crate::unstable_sort_by_key`].
///
/// # Panics
/// Invalid indices panic before mutation or key calls. A key panic restores all
/// records before unwinding, without promising a valid partial result.
pub fn unstable_select_nth_many_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    indices: &[usize],
    key: F,
) {
    algorithms::select_many::<_, _, true>(values, indices, ByKey(key));
}
