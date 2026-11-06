// SPDX-License-Identifier: BSD-2-Clause

//! Stable radix sort with O(√n) auxiliary memory.
//!
//! Tessera partitions large inputs with a stable in-place block partition and
//! finishes cache-sized tiles with LSD counting passes, MSD partitions or
//! insertion sort. See the repository README for the design and the roadmap.
//!
//! This crate is in pre-release: the engine is being ported from its research
//! prototype to stable Rust, and no sorting API is exported yet.

#![warn(missing_docs)]
