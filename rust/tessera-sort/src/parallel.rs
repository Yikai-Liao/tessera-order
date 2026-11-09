// SPDX-License-Identifier: BSD-2-Clause

//! Rayon entry points for stable sorting.
//!
//! The parallel engine shares the serial probes and node/tile sorting.
//! Large inputs partition the root across disjoint shards. Bounded buckets
//! sort in staging and write directly to final bands; other layouts retain
//! parallel block permutation and overlapping materialized-bucket sorting.
//! Small inputs and one-thread pools use the serial engine.

use crate::key::{ByKey, Direct};
use crate::{RadixKey, engine};

/// Sorts keys stably through the Rayon entry point.
///
/// Available with the `rayon` feature. Ordering and stability are the same as
/// [`crate::stable_sort`], including floats and [`Reverse`](core::cmp::Reverse).
/// Small inputs and one-thread pools use the serial engine; large inputs
/// partition the root and sort completed buckets across Rayon's current pool.
///
/// # Memory
///
/// For pool width `P`, auxiliary space is O(P√n) for fixed record size and
/// key width. Root scratch and directories use O(√(nP)); bounded bucket
/// staging adds O(R√n), where the fixed root fan-out `R` is at most 4096.
/// Reentrant callbacks may suspend more staged buckets than there are workers.
/// Direct counting uses O(PR) histogram storage. No contiguous n-record output
/// buffer is allocated. Rayon runtime storage and thread stacks are separate.
///
/// # Examples
///
/// ```
/// let mut keys = [42u64, 7, 19, 7, 3];
/// tessera_sort::par_stable_sort(&mut keys);
/// assert_eq!(keys, [3, 7, 7, 19, 42]);
/// ```
pub fn par_stable_sort<K: RadixKey + Send>(values: &mut [K]) {
    engine::parallel::sort::<K, _, false>(values, &mut Direct);
}

/// Sorts records stably by a key through the Rayon entry point.
///
/// Available with the `rayon` feature. Ordering, stability and record
/// preservation are the same as [`crate::stable_sort_by_key`]. Records need
/// `Send`, but do not need `Sync`, `Copy`, `Clone`, `Default` or `Ord`. Small
/// inputs and one-thread pools use the serial engine.
///
/// # Key function
///
/// `key` must implement `Fn + Sync` so Rayon jobs can borrow it concurrently.
/// It does not need `Send` or `Clone`. It may be called several times per
/// record, in any order, and should return the same key for a record throughout
/// the call. Inconsistent keys leave the result's
/// order unspecified, with every record still present exactly once.
///
/// # Panics
///
/// If `key` panics, every record is restored to `values`, in an unspecified
/// order, before the panic propagates to the caller. No record is dropped or
/// duplicated. The `rayon` feature enables `std`, so recovery is available
/// during block partitioning as well.
///
/// # Memory
///
/// Uses the same O(P√n) auxiliary-space bound as [`par_stable_sort`] for
/// pool width `P` and fixed record size and key width. Reentrant callbacks can
/// retain staging for up to the fixed root fan-out, not just `P` buckets.
///
/// # Examples
///
/// ```
/// use core::cmp::Reverse;
///
/// let mut rows = [(2u32, "first"), (1, "middle"), (2, "last")];
/// tessera_sort::par_stable_sort_by_key(&mut rows, |row| Reverse(row.0));
/// assert_eq!(rows, [(2, "first"), (2, "last"), (1, "middle")]);
/// ```
pub fn par_stable_sort_by_key<T, K, F>(values: &mut [T], key: F)
where
    T: Send,
    K: RadixKey,
    F: Fn(&T) -> K + Sync,
{
    engine::parallel::sort::<T, _, true>(values, &mut ByKey(&key));
}
