// SPDX-License-Identifier: BSD-2-Clause

//! Slice operations composed from complete ordered refinement. Rank validation
//! and normalization precede execution; no adapter owns partition decisions,
//! live scratch records or a separate selection engine.

use crate::{
    engine,
    key::{ByKey, Direct, Extract, RadixKey},
    rank::{self, Requirement},
    thresholds::TILE_BYTES,
    workspace::Workspace,
};
use alloc::vec::Vec;
use core::ops::Range;

/// Returns the indices of a stable sort, without modifying the source.
///
/// Ordering follows [`crate::stable_sort`], including float total order and
/// `Reverse`. Equal keys keep their original index order. The O(n) index result
/// is output space; sorting it uses the ordinary sorting workspace in addition.
///
/// # Examples
/// ```
/// let values = [3u64, 1, 3, 2];
/// assert_eq!(tessera_order::stable_argsort(&values), [1, 3, 0, 2]);
/// assert_eq!(values, [3, 1, 3, 2]);
/// ```
pub fn stable_argsort<K: RadixKey>(values: &[K]) -> Vec<usize> {
    stable_argsort_by_key(values, |key| *key)
}

/// Returns stable sorted indices using a key function, leaving the source intact.
///
/// Key evaluation follows [`crate::stable_sort_by_key`]: repeated `FnMut` calls
/// are allowed and consistent keys are required for ordering. A key panic drops
/// the partial index output and propagates. Source records are never moved;
/// side effects performed by the callback are not rolled back.
/// Memory is the O(n) index result plus the workspace for sorting indices.
///
/// # Examples
/// ```
/// let rows = [(2u32, "first"), (1, "middle"), (2, "last")];
/// assert_eq!(tessera_order::stable_argsort_by_key(&rows, |r| r.0), [1, 0, 2]);
/// ```
pub fn stable_argsort_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &[T],
    mut key: F,
) -> Vec<usize> {
    let mut indices: Vec<_> = (0..values.len()).collect();
    // Indices are payloads, not direct keys: counting/refilling indices would
    // destroy their identity when source keys repeat.
    engine::sort(&mut indices, ByKey(|index: &usize| key(&values[*index])));
    indices
}

/// Stably sorts each segment described by consecutive offsets.
///
/// Offsets must start at zero, end at `values.len()` and be nondecreasing.
/// Repeated offsets permit empty segments; an empty input accepts `[0]`.
/// One invocation reuses private workspace across segments and releases it on
/// return. Its bound is the sorting workspace of the largest segment.
///
/// # Panics
/// Invalid offsets panic before any record is changed.
///
/// # Examples
/// ```
/// let mut values = [3u64, 1, 2, 9, 7];
/// tessera_order::stable_sort_segments(&mut values, &[0, 3, 3, 5]);
/// assert_eq!(values, [1, 2, 3, 7, 9]);
/// ```
pub fn stable_sort_segments<K: RadixKey>(values: &mut [K], offsets: &[usize]) {
    segments(values, offsets, Direct);
}

/// Stably sorts segments by key, reusing one invocation's private workspace.
///
/// Boundaries and memory follow [`stable_sort_segments`]. Key evaluation follows
/// [`crate::stable_sort_by_key`]; the same `FnMut` is used across all segments.
///
/// # Panics
/// Invalid offsets panic before mutation or key calls. A key panic restores
/// every record in its current segment before unwinding; prior segments may
/// already be sorted and later segments have not been visited.
pub fn stable_sort_segments_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    offsets: &[usize],
    key: F,
) {
    segments(values, offsets, ByKey(key));
}

/// Partitions the smallest `k` records into a prefix, without sorting them.
///
/// Returns disjoint borrows of that prefix and the remaining records. Each
/// region contains exactly the corresponding members of a complete stable
/// sort, in unspecified internal order. Equal-key boundary members are chosen
/// in original input order. Zero and the full input length are no-ops without
/// key calls or allocation. For an ordered result use [`stable_sort_prefix`].
/// Workspace follows [`crate::stable_sort`], not O(k).
///
/// # Panics
/// Panics before mutation if `k > values.len()`.
///
/// # Examples
/// ```rust
/// let mut values = [9u64, 3, 1, 7, 2];
/// let (smallest, remaining) = tessera_order::stable_top_k(&mut values, 3);
/// let mut members = smallest.to_vec();
/// members.sort();
/// assert_eq!(members, [1, 2, 3]);
/// assert!(remaining.iter().all(|v| *v >= 7));
/// ```
pub fn stable_top_k<K: RadixKey>(values: &mut [K], k: usize) -> (&mut [K], &mut [K]) {
    top_k(values, k, Direct)
}

/// Partitions the smallest `k` records by key, with stable boundary membership.
///
/// Result, bounds and memory follow [`stable_top_k`]; key evaluation and panic
/// restoration follow [`crate::stable_sort_by_key`]. Neither output region is
/// required to be sorted. A zero or full-length request evaluates no keys.
///
/// # Panics
/// An invalid count panics before mutation or key calls. An extractor panic
/// restores all records before unwinding, in unspecified order.
pub fn stable_top_k_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    k: usize,
    key: F,
) -> (&mut [T], &mut [T]) {
    top_k(values, k, ByKey(key))
}

/// Makes the first `prefix_len` records equal a complete stable sort's prefix.
///
/// The suffix contains exactly the remaining records, in unspecified order.
/// Equal-key members at the boundary are chosen in original input order.
/// Zero is a no-op; `prefix_len == values.len()` is a complete stable sort.
/// Workspace has the bound of [`crate::stable_sort`], not `O(prefix_len)`.
///
/// # Panics
/// Panics before mutation if `prefix_len > values.len()`.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// tessera_order::stable_sort_prefix(&mut values, 3);
/// assert_eq!(&values[..3], &[1, 2, 3]);
/// ```
pub fn stable_sort_prefix<K: RadixKey>(values: &mut [K], prefix_len: usize) {
    refine_range(values, 0..prefix_len, Direct);
}

/// Produces the exact stable sorted prefix by key.
///
/// Result and memory follow [`stable_sort_prefix`]; key evaluation and panic
/// restoration follow [`crate::stable_sort_by_key`]. A zero prefix makes no key
/// calls and allocates nothing.
///
/// # Panics
/// Invalid length panics before mutation or key calls. A key panic restores
/// every record before propagating, without promising a valid partial result.
pub fn stable_sort_prefix_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    prefix_len: usize,
    key: F,
) {
    refine_range(values, 0..prefix_len, ByKey(key));
}

/// Places the exact record at `index` from a complete stable sort.
///
/// Returns disjoint borrows of the preceding records, selected record and
/// following records, as slice `select_nth_unstable` does. The two outer regions
/// contain exactly their corresponding stable ranked members, without internal
/// order guarantees. Stability determines which equal-key member is selected;
/// it does not preserve input order across different keys. Workspace follows
/// [`crate::stable_sort`].
///
/// # Panics
/// Panics before mutation if `index >= values.len()`.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// let (before, selected, after) = tessera_order::stable_select_nth(&mut values, 2);
/// assert_eq!(*selected, 3);
/// assert!(before.iter().all(|v| *v < 3));
/// assert!(after.iter().all(|v| *v > 3));
/// ```
pub fn stable_select_nth<K: RadixKey>(
    values: &mut [K],
    index: usize,
) -> (&mut [K], &mut K, &mut [K]) {
    select_nth(values, index, Direct)
}

/// Selects the exact stable ranked record by key.
///
/// Result and memory follow [`stable_select_nth`]; key evaluation follows
/// [`crate::stable_sort_by_key`]. No index decoration of records is required.
///
/// # Panics
/// An invalid index panics before mutation or key calls. A key panic restores
/// every record before unwinding, in unspecified order.
pub fn stable_select_nth_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    index: usize,
    key: F,
) -> (&mut [T], &mut T, &mut [T]) {
    select_nth(values, index, ByKey(key))
}

/// Sorts the requested ranks exactly as a complete stable sort would.
///
/// `values[ranks.clone()]` equals the same interval in a complete stable sort.
/// Prefix and suffix contain exactly their corresponding stable ranked members,
/// in unspecified order. An empty range is a no-op and establishes no outer
/// boundary. Workspace follows [`crate::stable_sort`]. These are global ranks,
/// not the original positions of a physical subslice.
///
/// # Panics
/// Panics before mutation if `start > end` or `end > values.len()`.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// tessera_order::stable_sort_rank_range(&mut values, 1..4);
/// assert_eq!(&values[1..4], &[2, 3, 7]);
/// ```
pub fn stable_sort_rank_range<K: RadixKey>(values: &mut [K], ranks: Range<usize>) {
    refine_range(values, ranks, Direct);
}

/// Produces an exact stable sorted rank interval by key.
///
/// Result, bounds and memory follow [`stable_sort_rank_range`]. Key evaluation
/// and panic restoration follow [`crate::stable_sort_by_key`]. Empty ranges make
/// no key calls and allocate nothing.
///
/// # Panics
/// Invalid ranks panic before mutation or key calls. A key panic restores all
/// records before propagating, without promising a valid partial result.
pub fn stable_sort_rank_range_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    ranks: Range<usize>,
    key: F,
) {
    refine_range(values, ranks, ByKey(key));
}

/// Places each requested position's exact record from a complete stable sort.
///
/// Every gap contains exactly its corresponding stable ranked members, without
/// internal order guarantees. Indices may be unordered and repeated; they are
/// validated and normalized once. An empty request is a no-op. Memory is the
/// sorting workspace plus O(m) request metadata for m supplied indices.
///
/// # Panics
/// Panics before mutation if any index is outside the input.
///
/// # Examples
/// ```
/// let mut values = [9u64, 3, 1, 7, 2];
/// tessera_order::stable_select_nth_many(&mut values, &[3, 1, 3]);
/// assert_eq!(values[1], 2);
/// assert_eq!(values[3], 7);
/// ```
pub fn stable_select_nth_many<K: RadixKey>(values: &mut [K], indices: &[usize]) {
    select_many(values, indices, Direct);
}

/// Selects several exact stable ranked members by key.
///
/// Result, normalization and memory follow [`stable_select_nth_many`]. Key
/// evaluation and panic restoration follow [`crate::stable_sort_by_key`]. An
/// empty request makes no key calls and allocates nothing.
///
/// # Panics
/// All indices are validated before mutation or key calls. An extractor panic
/// restores all records before unwinding, in unspecified order.
pub fn stable_select_nth_many_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    indices: &[usize],
    key: F,
) {
    select_many(values, indices, ByKey(key));
}

fn refine_range<T, E: Extract<T>>(values: &mut [T], ranks: Range<usize>, extractor: E) {
    assert!(
        ranks.start <= ranks.end && ranks.end <= values.len(),
        "rank range out of bounds"
    );
    let ranges = if ranks.is_empty() {
        &[][..]
    } else {
        core::slice::from_ref(&ranks)
    };
    let required = Requirement::new(values.len(), ranges);
    engine::refine(values, extractor, required, None, TILE_BYTES);
}

fn top_k<T, E: Extract<T>>(values: &mut [T], k: usize, extractor: E) -> (&mut [T], &mut [T]) {
    assert!(k <= values.len(), "top-k count out of bounds");
    if k > 0 && k < values.len() {
        // Isolating the first excluded rank establishes both memberships.
        // Reuse selection's request, not a second cut interpretation in engine.
        refine_range(values, k..k + 1, extractor);
    }
    values.split_at_mut(k)
}

fn select_nth<T, E: Extract<T>>(
    values: &mut [T],
    index: usize,
    extractor: E,
) -> (&mut [T], &mut T, &mut [T]) {
    assert!(index < values.len(), "selection index out of bounds");
    refine_range(values, index..index + 1, extractor);
    let (before, rest) = values.split_at_mut(index);
    let (selected, after) = rest.split_first_mut().unwrap();
    (before, selected, after)
}

fn select_many<T, E: Extract<T>>(values: &mut [T], indices: &[usize], extractor: E) {
    let ranges = rank::normalize_indices(values.len(), indices);
    let required = Requirement::new(values.len(), &ranges);
    engine::refine(values, extractor, required, None, TILE_BYTES);
}

fn segments<T, E: Extract<T>>(values: &mut [T], offsets: &[usize], mut extractor: E) {
    assert!(
        offsets.first() == Some(&0)
            && offsets.last() == Some(&values.len())
            && offsets.windows(2).all(|w| w[0] <= w[1]),
        "invalid segment offsets"
    );
    let mut workspace = Workspace::new();
    for ends in offsets.windows(2) {
        let segment = &mut values[ends[0]..ends[1]];
        let ranks = 0..segment.len();
        let required = Requirement::new(segment.len(), core::slice::from_ref(&ranks));
        engine::refine(
            segment,
            &mut extractor,
            required,
            Some(&mut workspace),
            TILE_BYTES,
        );
    }
}
