// SPDX-License-Identifier: BSD-2-Clause

//! Inputs and measurement tools for reproducible sorting comparisons.

pub mod case;
mod comparison;
pub mod plan;
pub mod pool;
pub mod sampling;
pub use comparison::{CaseInput, Kernel, Variant, list_variants, main, read_scratch};

pub mod allocation;
