// SPDX-License-Identifier: BSD-2-Clause

//! Compares sorting implementations using paired blocks and A/A controls.

/// Runs the compare command-line executable.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tessera_bench::main(false)
}
