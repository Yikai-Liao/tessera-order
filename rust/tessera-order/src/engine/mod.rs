// SPDX-License-Identifier: BSD-2-Clause

//! Complete ordering and distribution operations for the algorithm layer.
//!
//! Callers sort a slice, place one ranked record or distribute cached digits.
//! The engine owns planning, scratch, stable movement and restoration: each
//! operation returns or unwinds with every record back in its slice.

mod block;
mod distribution;
mod key;
mod order;
mod portable_simd;
mod refine;
mod tile;
mod trace;
mod workspace;

#[cfg(test)]
mod unwind;

pub mod thresholds;

pub use key::RadixKey;
#[cfg(feature = "trace")]
pub use trace::{Event, partition as trace_partition};

pub(crate) use distribution::{MAX_PARTITION_BUCKETS, distribute};
pub(crate) use key::{ByKey, Direct, Unsigned, bit_len};
#[cfg(feature = "rayon")]
pub(crate) use refine::parallel::{argsort as par_argsort, sort as par_sort};
pub(crate) use refine::{argsort, order_by_cached_indices, select_nth, sort};
