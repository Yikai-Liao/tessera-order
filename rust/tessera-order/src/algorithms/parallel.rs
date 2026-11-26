// SPDX-License-Identifier: BSD-2-Clause

//! Rayon sorting entry points and their record and extractor bounds.

use crate::engine::{self, ByKey, Direct, RadixKey};
use alloc::vec::Vec;

/// Sorts keys stably through the Rayon entry point.
///
/// Available with the `rayon` feature. Ordering and stability are the same as
/// [`crate::sort`], including floats and [`Reverse`](core::cmp::Reverse).
/// Small inputs and one-thread pools use the serial engine; large inputs
/// partition the root and sort completed buckets across Rayon's current pool.
///
/// # Memory
///
/// For pool width `P`, auxiliary space is O(P√n) for fixed record size and
/// key width. Root scratch and directories use O(√(nP)). Up to the fixed root
/// fan-out R of 4096 finishers can retain serial scratch, including suspended
/// callbacks; checked-out and pooled allocations both count. Whole-child tiles
/// have bounded byte and cursor capacity, independent of n. Direct counting
/// uses O(PR) histogram storage. No contiguous n-record output or private bucket
/// staging is allocated. Rayon runtime storage and thread stacks are separate.
///
/// # Examples
///
/// ```
/// let mut keys = [42u64, 7, 19, 7, 3];
/// tessera_order::par_sort(&mut keys);
/// assert_eq!(keys, [3, 7, 7, 19, 42]);
/// ```
pub fn par_sort<K: RadixKey>(values: &mut [K]) {
    engine::par_sort::<K, _>(values, Direct);
}

/// Sorts records stably by a key through the Rayon entry point.
///
/// Available with the `rayon` feature. Ordering, stability and record
/// preservation are the same as [`crate::sort_by_key`]. Records need
/// `Send`, but do not need `Sync`, `Copy`, `Clone`, `Default` or `Ord`. Small
/// inputs and one-thread pools use the serial engine.
///
/// # Key function
///
/// `key` must implement `Fn + Sync` so Rayon jobs can borrow it concurrently.
/// It does not need `Send` or `Clone`. It may run concurrently, any number of
/// times per record and in any order. The key must stay the same for each
/// record throughout the call to obtain a stable sorted result. Inconsistent
/// keys may cause a panic or leave the result's order unspecified, with every
/// record still present exactly once.
///
/// # Panics
///
/// If `key` unwinds, the same record preservation and lack of rollback as for
/// [`crate::sort_by_key`] apply, including changes through interior mutability.
/// Other tasks may still call `key`, but all tasks join and every record is
/// restored before the panic reaches the caller. If multiple tasks panic,
/// which panic propagates is unspecified.
///
/// # Memory
///
/// Uses the same O(P√n) auxiliary-space bound as [`par_sort`] for
/// pool width `P` and fixed record size and key width. Reentrant callbacks can
/// retain serial scratch for up to the fixed root fan-out, not just `P` buckets.
/// A nested public call owns its own scratch; suspended outer buffers remain
/// charged to the outer call.
///
/// # Examples
///
/// ```
/// use core::cmp::Reverse;
///
/// let mut rows = [(2u32, "first"), (1, "middle"), (2, "last")];
/// tessera_order::par_sort_by_key(&mut rows, |row| Reverse(row.0));
/// assert_eq!(rows, [(2, "first"), (2, "last"), (1, "middle")]);
/// ```
pub fn par_sort_by_key<T, K, F>(values: &mut [T], key: F)
where
    T: Send,
    K: RadixKey,
    F: Fn(&T) -> K + Sync,
{
    engine::par_sort::<T, _>(values, ByKey(&key));
}

/// Returns the indices that would sort `values` in ascending order, using
/// Rayon's current pool without moving `values`.
///
/// Available with the `rayon` feature. Result order and tie resolution match
/// [`crate::argsort`]: equal keys keep ascending source-index order. Small
/// inputs and one-thread pools use the serial engine.
///
/// # Memory
///
/// Allocates the O(n) index output plus the same O(P√n) auxiliary workspace
/// bound as [`par_sort`]. No second n-record staging buffer is allocated.
///
/// # Examples
///
/// ```
/// let values = [3u64, 1, 3, 2];
/// assert_eq!(tessera_order::par_argsort(&values), [1, 3, 0, 2]);
/// ```
#[must_use]
pub fn par_argsort<K: RadixKey>(values: &[K]) -> Vec<usize> {
    par_argsort_by_key(values, |key| *key)
}

/// Returns the indices that would sort `values` by a key function, using
/// Rayon's current pool without moving `values`.
///
/// Available with the `rayon` feature. Result order and tie resolution match
/// [`crate::argsort_by_key`]. Source records are shared across workers and need
/// `Sync`, not `Send`, `Copy`, `Clone`, `Default` or `Ord`.
///
/// # Key function
///
/// `key` must implement `Fn + Sync` and may run concurrently, any number of
/// times per record and in any order. Consistent keys produce the exact stable
/// permutation; inconsistent keys may cause a panic or return a permutation of
/// `0..values.len()` in an unspecified order.
///
/// # Panics
///
/// If `key` unwinds, all tasks join, the partial index output is discarded and
/// the panic propagates. Source records are never moved, and changes made
/// through interior mutability remain visible in `values`.
///
/// # Memory
///
/// Allocates the O(n) index output plus the same O(P√n) auxiliary workspace
/// bound as [`par_sort_by_key`].
///
/// # Examples
///
/// ```
/// let rows = [(2u32, "first"), (1, "middle"), (2, "last")];
/// assert_eq!(tessera_order::par_argsort_by_key(&rows, |r| r.0), [1, 0, 2]);
/// ```
#[must_use]
pub fn par_argsort_by_key<T, K, F>(values: &[T], key: F) -> Vec<usize>
where
    T: Sync,
    K: RadixKey,
    F: Fn(&T) -> K + Sync,
{
    engine::par_argsort(values, key)
}

/// Renamed to [`par_sort`]; the contract is unchanged.
#[deprecated(since = "0.2.0", note = "renamed to `par_sort`")]
pub fn par_stable_sort<K: RadixKey>(values: &mut [K]) {
    par_sort(values);
}

/// Renamed to [`par_sort_by_key`]; the contract is unchanged.
#[deprecated(since = "0.2.0", note = "renamed to `par_sort_by_key`")]
pub fn par_stable_sort_by_key<T, K, F>(values: &mut [T], key: F)
where
    T: Send,
    K: RadixKey,
    F: Fn(&T) -> K + Sync,
{
    par_sort_by_key(values, key);
}
