// SPDX-License-Identifier: BSD-2-Clause

//! Compares sorting algorithms using paired blocks and A/A controls.

/// Runs the compare command-line executable.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tessera_bench::main(false)
}
