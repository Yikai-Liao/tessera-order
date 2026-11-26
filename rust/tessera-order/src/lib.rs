// SPDX-License-Identifier: BSD-2-Clause

//! Stable radix ordering with O(√n) auxiliary memory: one engine behind
//! sorting and the standard ordering operations.
//!
//! Tessera sorts slices by a fixed-width [`RadixKey`]. It runs in linear time
//! for a fixed key width, keeps records with equal keys in input order, and
//! needs no `Copy`, `Clone`, `Default` or `Ord` on the records it moves.
//!
//! [`argsort`] returns indices in stable sorted order. [`select_nth`] places
//! the same record as a stable sort would at a given position and leaves the
//! order within either outer region unspecified. There are no `_by`
//! comparator variants, because the radix engine needs keys, not comparisons.
//!
//! # Examples
//!
//! ```
//! use core::cmp::Reverse;
//! use tessera_order::{sort, sort_by_key};
//!
//! let mut keys = [42, -7, 42, 0];
//! sort(&mut keys);
//! assert_eq!(keys, [-7, 0, 42, 42]);
//!
//! // Equal keys keep their input order, also when sorting in descending order.
//! let mut rows = [(2u32, "first"), (1, "middle"), (2, "last")];
//! sort_by_key(&mut rows, |row| Reverse(row.0));
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
//! # Panics and unwinding
//!
//! Like Rust's [`slice::sort_by_key`], in-place operations keep every original
//! record exactly once if the key function unwinds, including changes made
//! through interior mutability. Recovery itself does not call the key
//! function or roll back its side effects. Neither input order nor a valid
//! partial result is guaranteed.
//! [`argsort_by_key`] never moves the source records, even on unwind.
//!
//! # Memory
//!
//! Inputs larger than one 64 KiB tile are split by a stable block partition
//! that needs O(√n) records of scratch for a fixed record size and key width.
//! Each tile is then sorted in a scratch buffer of at most 64 KiB of records,
//! plus up to two bytes per record of cached bucket IDs for
//! [`sort_by_key`]. Each tile-local MSD level also keeps a cursor table
//! of up to 32 KiB on the stack, with at most 22 levels for 128-bit keys.
//!
//! Each call allocates scratch as needed and frees it on return or unwind.
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
//! `rayon` is off by default and enables `par_sort`, `par_sort_by_key`,
//! `par_argsort` and `par_argsort_by_key`. Large inputs partition the root
//! across disjoint shards and order each bucket's records by shard. Buckets
//! transfer directly to exclusive final bands and sort there while peers
//! continue moving their records. Small inputs and one-thread pools use the
//! serial engine. In-place sorting requires `Send` records, parallel argsort
//! requires `Sync` records, and both `_by_key` forms require a shared
//! (`Fn + Sync`) key function.

extern crate alloc;

mod algorithms;
mod engine;

pub use algorithms::cached::sort_by_cached_key;
#[cfg(feature = "rayon")]
#[allow(deprecated)]
pub use algorithms::parallel::{
    par_argsort, par_argsort_by_key, par_sort, par_sort_by_key, par_stable_sort,
    par_stable_sort_by_key,
};
pub use algorithms::{argsort, argsort_by_key, select_nth, select_nth_by_key, sort, sort_by_key};
#[allow(deprecated)]
pub use algorithms::{stable_sort, stable_sort_by_key};
pub use engine::RadixKey;

/// Internal thresholds, exported for benchmarks that probe boundary sizes.
///
/// Not part of the public API: the values may change in any release.
#[doc(hidden)]
pub use engine::thresholds;

/// Block-partition steps for the documentation figures (feature `trace`).
///
/// Not part of the public API: the events may change in any release.
#[cfg(feature = "trace")]
#[doc(hidden)]
pub mod figures {
    pub use crate::engine::{Event, trace_partition as partition};
}
