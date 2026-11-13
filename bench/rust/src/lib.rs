// SPDX-License-Identifier: BSD-2-Clause

//! Inputs and measurement tools for reproducible sorting comparisons.

pub mod case;
#[cfg(feature = "compare")]
mod comparison;
pub mod plan;
#[cfg(any(feature = "compare", feature = "rayon"))]
pub mod pool;
#[cfg(feature = "compare")]
pub mod sampling;
#[cfg(feature = "compare")]
pub use comparison::{CaseInput, Kernel, Variant, list_variants, main, read_scratch};

#[cfg(feature = "compare")]
pub mod allocation;
