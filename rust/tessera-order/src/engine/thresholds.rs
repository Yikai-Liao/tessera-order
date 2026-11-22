// SPDX-License-Identifier: BSD-2-Clause

//! Internal engine budgets and representation limits, also exposed to benchmarks.

/// Largest leaf finished by stable insertion.
pub const INSERTION_MAX: usize = 48;
/// Largest leaf finished one record at a time before pair insertion pays off.
pub(crate) const SINGLE_INSERTION_MAX: usize = 16;

/// On unordered input the shift work per record scales with length times
/// inline record width. Convert the crossover bytes to a record count,
/// keeping tiny leaves on insertion and bounding comparisons on narrow keys. See the cost model in docs/explain/tiles.md.
pub(crate) fn insertion_limit<T>() -> usize {
    const CROSSOVER_BYTES: usize = 2 * 1024;
    (CROSSOVER_BYTES / size_of::<T>().max(1)).clamp(SINGLE_INSERTION_MAX, INSERTION_MAX)
}

/// Default tile budget, in bytes of records.
pub const TILE_BYTES: usize = 64 * 1024;
/// Most records in one tile, so that a `u16` cursor addresses any of them.
pub const MAX_TILE_RECORDS: usize = super::tile::Cursor::MAX as usize;
/// Largest cursor table of a tile-local pass, in bytes.
pub const MAX_CURSOR_BYTES: usize = 32 * 1024;
