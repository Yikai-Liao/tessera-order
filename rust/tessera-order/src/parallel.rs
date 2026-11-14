// SPDX-License-Identifier: BSD-2-Clause

//! Rayon entry points for sorting.
//!
//! The parallel engine shares the serial probes and node/tile sorting.
//! Large inputs partition the root across disjoint shards, then transfer each
//! bucket directly into its exclusive final band and finish it in that task.
//! Small inputs and one-thread pools use the serial engine.

use crate::key::{ByKey, Direct};
use crate::{RadixKey, engine};

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
pub fn par_sort<K: RadixKey + Send>(values: &mut [K]) {
    engine::parallel::sort::<K, _>(values, Direct);
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
/// It does not need `Send` or `Clone`. It may be called several times per
/// record, in any order, and should return the same key for a record throughout
/// the call. Inconsistent keys may make the sort panic or leave the result's
/// order unspecified, with every record still present exactly once.
///
/// # Panics
///
/// If `key` panics, every record is restored to `values`, in an unspecified
/// order, before the panic propagates to the caller. No record is dropped or
/// duplicated.
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
    engine::parallel::sort::<T, _>(values, ByKey(&key));
}

/// Renamed to [`par_sort`]; the contract is unchanged.
#[deprecated(since = "0.2.0", note = "renamed to `par_sort`")]
pub fn par_stable_sort<K: RadixKey + Send>(values: &mut [K]) {
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
