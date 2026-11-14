// SPDX-License-Identifier: BSD-2-Clause

//! Internal engine budgets and representation limits, also exposed to benchmarks.

/// Largest leaf finished by stable insertion.
pub const INSERTION_MAX: usize = 48;
/// Default tile budget, in bytes of records.
pub const TILE_BYTES: usize = 64 * 1024;
/// Most records in one tile, so that a `u16` cursor addresses any of them.
pub const MAX_TILE_RECORDS: usize = super::tile::Cursor::MAX as usize;
/// Largest cursor table of a tile-local pass, in bytes.
pub const MAX_CURSOR_BYTES: usize = 32 * 1024;
