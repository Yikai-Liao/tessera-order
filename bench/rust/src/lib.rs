// SPDX-License-Identifier: BSD-2-Clause

//! Inputs and measurement tools for reproducible sorting comparisons.

pub mod case;
#[cfg(feature = "compare")]
mod comparison;
#[cfg(feature = "compare")]
pub use comparison::main;

#[cfg(feature = "compare")]
pub mod allocation;
