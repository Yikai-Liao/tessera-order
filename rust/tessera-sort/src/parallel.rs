// SPDX-License-Identifier: BSD-2-Clause

//! Rayon entry points and, once implemented, task scheduling.
//!
//! The serial engine remains the fallback and owns key encoding, routing and
//! record recovery. Keep scheduling here rather than adding threading bounds
//! to the serial API. These entry points currently delegate the whole sort.

use crate::key::{ByKey, Direct};
use crate::{RadixKey, engine};

/// Sorts keys stably through the Rayon entry point.
///
/// Available with the `rayon` feature. Ordering and stability are the same as
/// [`crate::stable_sort`], including floats and [`Reverse`](core::cmp::Reverse).
/// This initial implementation uses the serial engine for every input; it does
/// not schedule parallel tasks yet.
///
/// # Memory
///
/// Currently allocates and frees the same scratch as [`crate::stable_sort`].
/// The parallel scratch budget will be specified with task scheduling.
///
/// # Examples
///
/// ```
/// let mut keys = [42u64, 7, 19, 7, 3];
/// tessera_sort::par_stable_sort(&mut keys);
/// assert_eq!(keys, [3, 7, 7, 19, 42]);
/// ```
pub fn par_stable_sort<K: RadixKey + Send>(values: &mut [K]) {
    engine::sort::<K, _, false>(values, &mut Direct, None);
}

/// Sorts records stably by a key through the Rayon entry point.
///
/// Available with the `rayon` feature. Ordering, stability and record
/// preservation are the same as [`crate::stable_sort_by_key`]. Records need
/// `Send`, but do not need `Sync`, `Copy`, `Clone`, `Default` or `Ord`.
/// This initial implementation uses the serial engine for every input; it does
/// not schedule parallel tasks yet.
///
/// # Key function
///
/// `key` must implement `Fn + Sync` so tasks can borrow it concurrently once
/// scheduling is implemented. It does not need `Send` or `Clone`. It may be
/// called several times per record, in any order, and should return the same
/// key for a record throughout the call. Inconsistent keys leave the result's
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
/// Currently allocates and frees the same scratch as [`crate::stable_sort_by_key`].
/// The parallel scratch budget will be specified with task scheduling.
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
    engine::sort::<T, _, true>(values, &mut ByKey(&key), None);
}
