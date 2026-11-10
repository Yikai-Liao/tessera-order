// SPDX-License-Identifier: BSD-2-Clause

//! Counting passes over slices within the tile budget.
//!
//! Two implementations finish a tile, and both work through one buffer of tile
//! size, so the recursion always sees the caller's slice:
//!
//! - a **flat plan** runs one counting pass per window of varying key bits,
//!   least significant window first, and needs no recursion at all;
//! - a **counted split** partitions by one window and leaves the children to the
//!   recursion, which reaches the insertion leaf in a few passes.
//!
//! The cost model decides which is cheaper for the tile's own keys. Cached
//! bucket ids keep every scatter free of key calls, so no user code runs while
//! scratch holds a second copy of the records.

use crate::cost::{self, Route};
use crate::key::{Digit, Extract};
use crate::scan;
use core::mem::MaybeUninit;
use core::ptr;

pub(crate) struct Tile<T> {
    records: Vec<MaybeUninit<T>>,
    ids: Vec<u16>,
    counts: Vec<u16>,
}

impl<T> Tile<T> {
    /// Creates empty scratch without allocating.
    pub(crate) fn new() -> Self {
        Self {
            records: Vec::new(),
            ids: Vec::new(),
            counts: Vec::new(),
        }
    }

    /// Grows the buffers to hold `len` records and `buckets` cursors.
    fn reserve(&mut self, len: usize, buckets: usize) {
        if self.records.len() < len {
            self.records.resize_with(len, MaybeUninit::uninit);
        }
        if self.ids.len() < len {
            self.ids.resize(len, 0);
        }
        if self.counts.len() < buckets {
            self.counts.resize(buckets, 0);
        }
    }
}

/// Partitions `records` stably by `digit`, writing each bucket's cumulative
/// record end into `ends`.
///
/// The records are back in place when this returns. Every key call runs before
/// the first write to scratch, so a key panic leaves the slice untouched.
///
/// Panics if `records` is longer than `u16::MAX` records or `digit` is
/// wider than [`cost::MAX_DIGIT_BITS`].
pub(crate) fn partition<T, E: Extract<T>>(
    records: &mut [T],
    digit: Digit,
    tile: &mut Tile<T>,
    extractor: &mut E,
    ends: &mut Vec<usize>,
) -> usize {
    let len = records.len();
    if len < 2 {
        ends.clear();
        return len;
    }
    let buckets = 1usize << digit.width;
    debug_assert!(buckets <= cost::MAX_BUCKETS && len <= u16::MAX as usize);
    tile.reserve(len, buckets);
    let scratch = tile.records.as_mut_ptr().cast::<T>();
    let ids = tile.ids.as_mut_ptr();
    let cursor = &mut tile.counts[..buckets];
    cursor.fill(0);
    // The largest bucket decides whether the caller recursion has anything to
    // do, so it comes with the histogram instead of a walk over the table.
    let mut largest = 0u16;
    for (index, record) in records.iter().enumerate() {
        let bucket = digit.classify(extractor, record);
        // SAFETY: the scratch reserves len id slots, and the digit keeps the
        // bucket below the cursor table length.
        unsafe { *ids.add(index) = bucket as u16 };
        let count = &mut cursor[bucket];
        *count += 1;
        largest = largest.max(*count);
    }
    // Counting sort: every cursor becomes the start of its bucket.
    scan::prefix_sum(cursor);
    let source = records.as_ptr();
    for index in 0..len {
        // SAFETY: the histogram initialized the id of every record.
        let bucket = unsafe { *ids.add(index) } as usize;
        let slot = cursor[bucket] as usize;
        cursor[bucket] += 1;
        // SAFETY: cached ids send each record to its own start slot below the
        // record count, in a buffer disjoint from the records.
        unsafe { ptr::write(scratch.add(slot), ptr::read(source.add(index))) };
    }
    // SAFETY: scratch holds exactly as many records as the slice, and none is
    // dropped first, so the slice owns each record exactly once afterwards.
    unsafe { ptr::copy_nonoverlapping(scratch, records.as_mut_ptr(), len) };
    // A slice whose buckets are all leaves is finished by one insertion pass
    // over it, so it needs no boundaries.
    ends.clear();
    if largest as usize > crate::cost::LEAF {
        ends.extend(cursor.iter().map(|&end| end as usize));
    }
    largest as usize
}

/// Finishes a tile with the implementation the cost model chose, and
/// answers with the split digit when the caller should split instead.
///
/// A slice left unfinished is a stable permutation of its input.
pub(crate) fn finish<T, E: Extract<T>>(
    records: &mut [T],
    varying: u128,
    tile: &mut Tile<T>,
    planner: &mut cost::Planner,
    extractor: &mut E,
) -> Option<Digit> {
    match planner.tile(records, varying, extractor).route {
        Route::Flat { width, passes, .. } => {
            let mut digits = [Digit { shift: 0, width: 0 }; cost::MAX_PASSES];
            let planned = cost::plan_flat(varying, width, &mut digits);
            debug_assert_eq!(planned, passes);
            flat_plan(records, &digits[..passes], tile, extractor);
            None
        }
        Route::Split { digit, .. } => Some(digit),
        // A tile has no rotation or pivot route: a tile finishes in one pass
        // either way, so the model never answers with one.
        Route::Rotate | Route::Pivot => unreachable!("tile route"),
    }
}
/// Runs a flat plan: one stable counting pass per window, least significant
/// window first.
fn flat_plan<T, E: Extract<T>>(
    records: &mut [T],
    digits: &[Digit],
    tile: &mut Tile<T>,
    extractor: &mut E,
) {
    let len = records.len();
    let buckets = digits
        .iter()
        .map(|digit| 1usize << digit.width)
        .max()
        .unwrap_or(2);
    tile.reserve(len, buckets);
    // SAFETY: each pass gets a disjoint source and destination of len records.
    // Only one of them is ever the tile itself, the other is the tile's scratch,
    // and a panic in a later histogram is repaired by `Restore` before the slice
    // is visible again.
    unsafe {
        let base = records.as_mut_ptr();
        let scratch = tile.records.as_mut_ptr().cast::<T>();
        let ids = tile.ids.as_mut_ptr();
        let counts = &mut tile.counts[..buckets];
        let mut restore = Restore {
            base,
            live: base,
            len,
        };
        let mut source = base;
        let mut destination = scratch;
        // The plan lists windows highest first; a least-significant-digit run
        // consumes them in reverse.
        for &digit in digits.iter().rev() {
            pass(source, destination, len, digit, counts, ids, extractor);
            core::mem::swap(&mut source, &mut destination);
            restore.live = source;
        }
        if source != base {
            ptr::copy_nonoverlapping(source, base, len);
        }
        restore.live = base;
    }
}

/// Copies the live records back to the tile if a later pass unwinds.
///
/// A pass moves records between the tile and its scratch, so after the first
/// pass the tile holds stale bytes. Dropping the guard restores the live
/// sequence, which is a stable permutation of the input.
struct Restore<T> {
    base: *mut T,
    live: *mut T,
    len: usize,
}

impl<T> Drop for Restore<T> {
    fn drop(&mut self) {
        if self.live != self.base {
            // SAFETY: the live pointer names len initialized records in scratch
            // and the base reserved the same count. Bytes are moved, never
            // dropped, and the scratch is recycled rather than read as records.
            unsafe { ptr::copy_nonoverlapping(self.live, self.base, self.len) };
        }
    }
}

/// Runs one stable counting pass from `source` into `destination`.
///
/// # Safety
///
/// `source` and `destination` are each valid for `len` records and do not
/// overlap. `counts` holds at least one cursor per bucket of `digit`, `ids`
/// holds `len` slots, and `len <= u16::MAX`.
unsafe fn pass<T, E: Extract<T>>(
    source: *const T,
    destination: *mut T,
    len: usize,
    digit: Digit,
    counts: &mut [u16],
    ids: *mut u16,
    extractor: &mut E,
) {
    counts.fill(0);
    for index in 0..len {
        // SAFETY: the caller supplies len initialized records.
        let record = unsafe { &*source.add(index) };
        let bucket = digit.classify(extractor, record);
        // SAFETY: the caller reserves len id slots and the digit keeps the
        // bucket below the cursor table length.
        unsafe { *ids.add(index) = bucket as u16 };
        counts[bucket] += 1;
    }
    scan::prefix_sum(counts);
    for index in 0..len {
        // SAFETY: the histogram initialized the id of every record.
        let bucket = unsafe { *ids.add(index) } as usize;
        let slot = counts[bucket] as usize;
        counts[bucket] += 1;
        // SAFETY: cached ids place disjoint output slots below len, in a buffer
        // that does not overlap the source.
        unsafe { ptr::write(destination.add(slot), ptr::read(source.add(index))) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::ByKey;

    #[test]
    fn narrow_tiles_finish_flat() {
        // Nine values over four bits: one counting pass finishes the tile, so
        // the model takes the flat plan and the records come back sorted.
        let mut records: Vec<(u32, u32)> = (0..300u32).map(|id| (id % 9, id)).collect();
        let mut expected = records.clone();
        expected.sort_by_key(|record| record.0);
        let mut tile = Tile::new();
        let mut planner = cost::Planner::new();
        let mut extractor = ByKey(|record: &(u32, u32)| record.0);
        let split = finish(&mut records, 0xf, &mut tile, &mut planner, &mut extractor);
        assert!(split.is_none(), "a narrow tile finishes flat");
        assert_eq!(records, expected);
    }

    #[test]
    fn spread_tiles_split_wide() {
        // Twenty-four spread bits over 300 records: one wide window leaves
        // single-record children, which beats three counting passes.
        let mut records: Vec<(u32, u32)> = (0..300u32)
            .map(|id| (id.wrapping_mul(2_654_435_761) >> 8, id))
            .collect();
        let varying = records
            .iter()
            .fold(0u128, |acc, record| acc | u128::from(record.0));
        let mut tile = Tile::new();
        let mut planner = cost::Planner::new();
        let mut extractor = ByKey(|record: &(u32, u32)| record.0);
        let digit = finish(
            &mut records,
            varying,
            &mut tile,
            &mut planner,
            &mut extractor,
        )
        .expect("a spread tile splits");
        // The window ends at the top varying bit, so the split removes it.
        assert_eq!(digit.shift + digit.width, crate::key::bit_len(varying));
    }
}
