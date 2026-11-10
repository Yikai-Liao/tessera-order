// SPDX-License-Identifier: BSD-2-Clause

//! Stable radix sort with O(sqrt(n)) auxiliary memory.
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
//! that needs O(sqrt(n)) records of scratch for a fixed record size and digit
//! width. Each tile is then finished in a scratch buffer of at most 64 KiB of
//! records plus two bytes per record of cached bucket IDs, and each tile split
//! keeps a cursor table of under 3 KiB on the stack. A tile-local window that
//! covers every remaining key difference of a [`stable_sort`] input is filled
//! in place instead, and needs no scratch at all.
//!
//! Each call allocates scratch as needed and frees it before returning.
//!
//! # Cargo features
//!
//! The crate needs `std` and Rust 1.89 or newer. Adjacent-key scans use
//! [`fearless_simd`](https://docs.rs/fearless_simd), which selects a supported
//! instruction set at run time on x86; other targets use their compile-time
//! baseline, including Arm Neon and WebAssembly SIMD when enabled for the
//! target. Keys without a vector kernel use scalar scans. Results are the same
//! on every path.
//!
//! `rayon` is enabled by default and adds [`par_stable_sort`] and
//! [`par_stable_sort_by_key`]. Large inputs partition the root across disjoint
//! shards, then sort the buckets of that split across Rayon's current pool.
//! Small inputs and one-thread pools use the serial engine. The feature requires
//! `Send` records and a shared (`Fn + Sync`) key function.

mod block;
mod cost;
mod engine;
mod insertion;
mod key;
#[cfg(feature = "rayon")]
mod parallel;
mod scan;
mod tile;
mod unwind;

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
    engine::sort::<K, _>(values, &mut key::Direct);
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
/// As [`stable_sort`]. The two bytes of cached bucket IDs per tile record apply
/// to this entry point's tiles as well.
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
    engine::sort::<T, _>(values, &mut key::ByKey(key));
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
    /// Most records in one tile, bounded by the width of its bucket cursor.
    pub const MAX_TILE_RECORDS: usize = u16::MAX as usize;
}
