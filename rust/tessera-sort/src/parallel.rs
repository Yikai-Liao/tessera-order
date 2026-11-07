// SPDX-License-Identifier: BSD-2-Clause

//! Rayon entry points for stable sorting.
//!
//! The parallel engine uses the existing serial probes and node/tile sorting.
//! It partitions the root serially, then sorts each complete bucket in Rayon's
//! current pool while root compaction continues. Small inputs and one-thread
//! pools use the serial engine.

use crate::key::{ByKey, Direct};
use crate::{RadixKey, engine};

/// Sorts keys stably through the Rayon entry point.
///
/// Available with the `rayon` feature. Ordering and stability are the same as
/// [`crate::stable_sort`], including floats and [`Reverse`](core::cmp::Reverse).
/// Small inputs and one-thread pools use the serial engine; larger inputs split
/// completed root buckets across Rayon's current pool.
///
/// # Memory
///
/// Scratch remains O(√n) for fixed record size and key width. The root digit is
/// at most 12 bits, so at most 4096 child jobs can retain workspaces. If job
/// `i` holds `mᵢ` records, their disjoint inputs give
/// `Σ√mᵢ ≤ √(4096n)`; the root partition scratch and fixed-size tile buffers
/// preserve the same bound.
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
/// Scratch remains O(√n) for fixed record size and key width. The root digit is
/// at most 12 bits, bounding the pool to 4096 job workspaces; fixed fan-out
/// keeps their combined scratch O(√n).
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
