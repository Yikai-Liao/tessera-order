// SPDX-License-Identifier: BSD-2-Clause

//! Stable radix sort with O(√n) auxiliary memory.
//!
//! Tessera sorts slices by a primitive key: an integer up to 128 bits, an
//! `f32` or `f64`, or [`Reverse`](core::cmp::Reverse) of one of these. It runs
//! in linear time for a fixed key width, keeps records with equal keys in
//! input order, and needs no `Copy`, `Clone`, `Default` or `Ord` on the
//! records it moves.
//!
//! # Examples
//!
//! ```
//! use core::cmp::Reverse;
//! use tessera_sort::{stable_sort, stable_sort_by_key};
//!
//! let mut keys = [42, -7, 42, 0];
//! stable_sort(&mut keys);
//! assert_eq!(keys, [-7, 0, 42, 42]);
//!
//! // Equal keys keep their input order, also when sorting in descending order.
//! let mut rows = [(2u32, "first"), (1, "middle"), (2, "last")];
//! stable_sort_by_key(&mut rows, |row| Reverse(row.0));
//! assert_eq!(rows, [(2, "first"), (2, "last"), (1, "middle")]);
//! ```
//!
//! # Ordering
//!
//! Integers sort in their usual order. Floats follow [`f64::total_cmp`]:
//! `-0.0` sorts before `+0.0`, and NaNs sort by sign and payload, negative NaNs
//! first and positive NaNs last. To treat every NaN or both zeros as equal,
//! normalize the key in the key function. For descending order, wrap the key in
//! `Reverse` rather than reversing an ascending result, which would also
//! reverse the order of equal keys.
//!
//! # Memory
//!
//! Inputs larger than one 64 KiB tile are split by a stable block partition
//! that needs O(√n) records of scratch for a fixed record size and key width.
//! Each tile is then sorted in a scratch buffer of at most 64 KiB of records,
//! plus up to two bytes per record of cached bucket IDs for
//! [`stable_sort_by_key`]. Each tile-local MSD level also keeps a cursor table
//! of up to 32 KiB on the stack, with at most 22 levels for 128-bit keys.
//!
//! These bounds describe the sequential API. With `rayon`, parallel calls use
//! reusable child workspaces; see the parallel functions for the workspace-
//! count dependent memory bound and the reentrant-key qualification.
//!
//! The free functions allocate this scratch during each call. A [`Workspace`]
//! keeps it between calls.
//!
//! # Cargo features
//!
//! `std` is enabled by default. Disable it for `no_std`: the crate then uses
//! `core` and `alloc`, so it needs a global allocator. A key-function panic
//! during block partitioning aborts rather than exposing partially moved
//! records, even when the host enables unwinding. Panics elsewhere may unwind
//! safely. Sorting, ordering and the memory bounds above are the same either way.
//!
//! The optional `rayon` feature enables `par_stable_sort` and
//! `par_stable_sort_by_key` and implies `std`. It does not change the
//! sequential API or the default feature selection.
//!
//! `portable-simd` is off by default and requires `std` and Rust 1.89 or newer.
//! It adds [`fearless_simd`](https://docs.rs/fearless_simd) for adjacent-key
//! scans, ascending-order confirmation and bucket-count prefix sums. On x86 it
//! selects a supported instruction set at run time, once per insertion pass,
//! ascending confirmation or prefix sum. Other targets use their compile-time
//! baseline, including Arm Neon and WebAssembly SIMD when enabled for the
//! target. Keys without a vector kernel use scalar scans. Results are the same
//! with and without the feature.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod block;
mod engine;
mod key;
#[cfg(feature = "portable-simd")]
mod portable_simd;
mod scan;
mod unwind;
mod workspace;

pub use key::RadixKey;
pub use workspace::Workspace;

/// Sorts keys in ascending order, keeping equal keys in input order.
///
/// The order is the encoded order documented on [`RadixKey`]: integers in their
/// usual order, floats as [`f64::total_cmp`], and [`Reverse`](core::cmp::Reverse)
/// keys descending. To reuse scratch memory across calls, use
/// [`Workspace::stable_sort`].
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
/// tessera_sort::stable_sort(&mut values);
/// let bits: Vec<u64> = values.iter().map(|x| x.to_bits()).collect();
/// let expected = [-1.0f64, -0.0, 0.0, 0.5, f64::NAN].map(f64::to_bits);
/// assert_eq!(bits, expected);
/// ```
pub fn stable_sort<K: RadixKey>(values: &mut [K]) {
    engine::sort::<K, _, false>(values, &mut key::Direct, None);
}

/// Sorts records by a key, keeping records with equal keys in input order.
///
/// `key` maps a record to a [`RadixKey`]. Records of any type are moved, never
/// cloned or dropped, so `String`, `Box<T>` and `&mut T` records are fine.
/// To reuse scratch memory across calls, use [`Workspace::stable_sort_by_key`].
///
/// # Key function
///
/// `key` may be called several times for each record, in any order. It should
/// return the same key for a record throughout one call. If it does not, the
/// order of the result is unspecified, but the slice still holds every record
/// exactly once.
///
/// # Panics
///
/// If `key` panics, the panic propagates to the caller after every record has
/// been moved back into `values`, in an unspecified order. No record is
/// dropped or duplicated. Without the `std` feature, a panic during block
/// partitioning aborts; panics elsewhere may unwind with every record preserved.
///
/// # Memory
///
/// As [`stable_sort`], plus up to two bytes per tile record of cached bucket
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
/// tessera_sort::stable_sort_by_key(&mut rows, |row| row.timestamp);
/// let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
/// assert_eq!(names, ["a", "c", "d"]);
/// ```
pub fn stable_sort_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(values: &mut [T], key: F) {
    engine::sort::<T, _, true>(values, &mut key::ByKey(key), None);
}

/// Internal thresholds, exported for benchmarks that probe boundary sizes.
///
/// Not part of the public API: the values may change in any release.
#[doc(hidden)]
pub mod thresholds {
    /// Largest tile or bucket that is finished by insertion sort.
    pub const INSERTION_MAX: usize = 48;
    /// Default tile budget, in bytes of records.
    pub const TILE_BYTES: usize = 64 * 1024;
    /// Most records in one tile, so that a `u16` cursor addresses any of them.
    pub const MAX_TILE_RECORDS: usize = u16::MAX as usize;
    /// Largest cursor table of a tile-local pass, in bytes.
    pub const MAX_CURSOR_BYTES: usize = 32 * 1024;
}

/// Sorts keys in ascending order in the current Rayon pool, stably.
///
/// Requires the `rayon` feature, which enables `std`. Ordering is identical to
/// [`stable_sort`]. Small inputs and adaptive fast paths run sequentially.
///
/// # Memory
///
/// The block-partition route uses reusable workspaces rather than a full-size
/// output buffer. With W
/// workspaces and R root buckets, heap scratch is O(√n + √(Wn) + W + R) for
/// fixed record size and key width. Ordinary non-reentrant keys lease at most
/// one workspace per worker. Keys that enter Rayon can suspend several jobs
/// on one worker; W is then bounded by the number of root bucket jobs rather
/// than the pool size. Each workspace also retains bounded tile storage.
/// Rayon owns its thread stacks and pool allocations.
///
/// Direct `u32`/`u64` windows of one to eight varying bits may instead use a
/// bounded parallel count. This retains the preparation scratch but creates no
/// root/child jobs or their workspaces. With P workers and R <= 256 buckets,
/// its vectors request `(P * R + R + 1) * size_of::<usize>()` bytes. Admission
/// additionally accounts a conservative 4096 * P task/frame allowance, capped
/// together with the vectors at `8 * P * ceil(sqrt(n)) * size_of::<K>()`.
/// It requires P >= 2 and P <= n / 4096. This is an algorithm storage budget,
/// not measured Rayon stack usage or RSS; preparation scratch is additional.
/// The default sequential API and unsupported parallel types are unchanged.
///
/// # Examples
///
/// ```
/// let mut keys = [42, -7, 42, 0];
/// tessera_sort::par_stable_sort(&mut keys);
/// assert_eq!(keys, [-7, 0, 42, 42]);
/// ```
#[cfg(feature = "rayon")]
pub fn par_stable_sort<K: RadixKey + Send>(values: &mut [K]) {
    engine::par_sort::<K, _, false>(values, key::Direct);
}

/// Sorts records by a key in the current Rayon pool, keeping equal keys stable.
///
/// Requires the `rayon` feature. Records only need `Send`, not `Sync`, `Copy`
/// or `Clone`. The key function may run concurrently and therefore implements
/// `Fn + Sync`; the sequential [`stable_sort_by_key`] also accepts `FnMut`.
///
/// # Key function
///
/// `key` may be called several times per record, in any order, including from
/// different threads. It should return the same key for a record throughout
/// the call. Inconsistent keys produce unspecified ordering, but every record
/// remains present exactly once.
///
/// # Panics
///
/// If `key` panics, started tasks finish unwinding before the panic propagates.
/// Every record is restored to `values`, in an unspecified order. If several
/// tasks panic, one of those panics propagates.
///
/// # Memory
///
/// Uses the bounded worker scratch described by [`par_stable_sort`].
///
/// # Examples
///
/// ```
/// let mut rows = vec![(2u32, "first"), (1, "middle"), (2, "last")];
/// tessera_sort::par_stable_sort_by_key(&mut rows, |row| row.0);
/// assert_eq!(rows, [(1, "middle"), (2, "first"), (2, "last")]);
/// ```
#[cfg(feature = "rayon")]
pub fn par_stable_sort_by_key<T: Send, K: RadixKey, F: Fn(&T) -> K + Sync>(
    values: &mut [T],
    key: F,
) {
    if engine::par_use_serial::<T>(values.len()) {
        stable_sort_by_key(values, key);
        return;
    }
    engine::par_sort::<T, _, true>(values, key::ByKey(&key));
}
