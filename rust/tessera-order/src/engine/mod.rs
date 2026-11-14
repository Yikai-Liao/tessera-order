// SPDX-License-Identifier: BSD-2-Clause

//! Complete ordering and distribution operations for the algorithm layer.
//!
//! Callers choose required ranks, extract keys and own output construction.
//! The engine owns planning, scratch layout, stable movement and restoration:
//! each operation returns or unwinds with every record back in its slice.
//! Borrowed workspace retains capacity only, never live records or progress.
//! These interfaces remain private while their consumers establish the contract.

mod block;
mod distribution;
mod key;
mod order;
mod portable_simd;
mod rank;
mod refine;
mod tile;
mod trace;
mod workspace;

pub mod thresholds;

pub use key::RadixKey;
#[cfg(feature = "trace")]
pub use trace::{Event, partition as trace_partition};

pub(crate) use distribution::{MAX_PARTITION_BUCKETS, distribute};
pub(crate) use key::{ByKey, Direct, Extract, Unsigned, bit_len};
pub(crate) use rank::{Requirement, normalize_indices};
#[cfg(feature = "rayon")]
pub(crate) use refine::parallel::sort as par_sort;
pub(crate) use refine::{refine, sort};
pub(crate) use workspace::Workspace;
