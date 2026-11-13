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
//! Each call allocates scratch as needed and frees it before returning.
//!
//! # Backend and Cargo features
//!
//! Tessera requires `std` and Rust 1.89 or newer. Every build uses
//! [`fearless_simd`](https://docs.rs/fearless_simd) for adjacent-key
//! scans, ascending-order confirmation and bucket-count prefix sums. On x86 it
//! selects a supported instruction set at run time, once per insertion pass,
//! ascending confirmation or prefix sum. Other targets use their compile-time
//! baseline, including Arm Neon and WebAssembly SIMD when enabled for the
//! target. Keys without a vector kernel use scalar scans. There is no backend
//! feature switch; `--no-default-features` uses this same implementation.
//!
//! `rayon` is off by default and enables `par_stable_sort` and
//! `par_stable_sort_by_key`. Large inputs partition the root across disjoint
//! shards and order each bucket's records by shard. Buckets transfer directly
//! to exclusive final bands and sort there while peers continue moving their
//! records. Small inputs and one-thread pools use the serial engine. The
//! feature requires `Send` records and a shared (`Fn + Sync`) key function.

extern crate alloc;

mod block;
mod engine;
mod key;
mod order;
#[cfg(feature = "rayon")]
mod parallel;
mod portable_simd;
mod rank;
mod tile;
mod trace;
#[cfg(test)]
mod unwind;
mod workspace;

pub use key::RadixKey;
#[cfg(feature = "rayon")]
pub use parallel::{par_stable_sort, par_stable_sort_by_key};

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
/// tessera_sort::stable_sort(&mut values);
/// let bits: Vec<u64> = values.iter().map(|x| x.to_bits()).collect();
/// let expected = [-1.0f64, -0.0, 0.0, 0.5, f64::NAN].map(f64::to_bits);
/// assert_eq!(bits, expected);
/// ```
pub fn stable_sort<K: RadixKey>(values: &mut [K]) {
    engine::sort::<K, _>(values, key::Direct);
}

/// Sorts records by a key, keeping records with equal keys in input order.
///
/// `key` maps a record to a [`RadixKey`]. Records of any type are moved, never
/// cloned or dropped, so `String`, `Box<T>` and `&mut T` records are fine.
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
/// dropped or duplicated.
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
    engine::sort::<T, _>(values, key::ByKey(key));
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
    pub const MAX_TILE_RECORDS: usize = crate::tile::Cursor::MAX as usize;
    /// Largest cursor table of a tile-local pass, in bytes.
    pub const MAX_CURSOR_BYTES: usize = 32 * 1024;
}

/// Block-partition steps for the documentation figures (feature `trace`).
///
/// Not part of the public API: the events may change in any release.
#[cfg(feature = "trace")]
#[doc(hidden)]
pub mod figures {
    pub use crate::trace::{Event, partition};
}
