// SPDX-License-Identifier: BSD-2-Clause

//! Bounded tile finishing owns scratch layout, cached classification and restoration.
//! Callers supply records, remaining variation and an extractor; raw pass pointers
//! and the live-copy guard never cross this boundary.

use super::plan::{Digit, TilePlan, plan_tile};
use super::scan_varying_bits;
use crate::key::{Extract, Unsigned};
use crate::thresholds::{INSERTION_MAX, MAX_CURSOR_BYTES, MAX_TILE_RECORDS};
use alloc::vec::Vec;
use core::mem::MaybeUninit;

/// Positions and cached bucket IDs used by tile passes.
pub(crate) type Cursor = u16;

/// Largest power-of-two table fitting the cursor budget.
pub(super) const MAX_DIGIT_BUCKETS: usize =
    1 << (MAX_CURSOR_BYTES / core::mem::size_of::<Cursor>()).ilog2();

/// Converts a byte budget once into the record capacity used by every route.
/// A tile holds at least one record and all its cursors must fit in Cursor.
pub(super) fn capacity<T>(tile_bytes: usize) -> usize {
    (tile_bytes / core::mem::size_of::<T>().max(1)).clamp(1, MAX_TILE_RECORDS)
}

/// Reusable record scratch; each sort restores its complete record set before return.
pub(super) struct Scratch<T>(Vec<MaybeUninit<T>>);

impl<T> Scratch<T> {
    /// Creates empty storage without allocating.
    pub(super) fn new() -> Self {
        Self(Vec::new())
    }

    /// Stably finishes a cursor-bounded slice, restoring records on key panic.
    pub(super) fn sort<E: Extract<T>>(
        &mut self,
        records: &mut [T],
        varying: u128,
        extractor: &mut E,
    ) {
        debug_assert!(records.len() <= MAX_TILE_RECORDS);
        Tile::<T, E> {
            scratch: &mut self.0,
            extractor,
        }
        .sort_tile(records, varying);
    }

    /// Finishes a cursor-bounded partition child whose remaining variation
    /// lies within `candidate`. Node and tile partitions share this rule.
    pub(super) fn finish_child<E: Extract<T>>(
        &mut self,
        records: &mut [T],
        candidate: u128,
        extractor: &mut E,
    ) {
        debug_assert!(records.len() <= MAX_TILE_RECORDS);
        Tile::<T, E> {
            scratch: &mut self.0,
            extractor,
        }
        .finish_child(records, candidate);
    }
}

/// One active tile operation, including recursive MSD children.
struct Tile<'a, T, E> {
    scratch: &'a mut Vec<MaybeUninit<T>>,
    extractor: &'a mut E,
}

/// One stable counting pass: histogram, prefix sum, then scatter from `source`
/// to `destination`. Returns the size of the largest bucket.
///
/// Without `E::DIRECT`, the histogram stores each record's bucket and the scatter
/// reads it back, so a key function that changes its answer cannot send two
/// records to one slot. Otherwise the scatter recomputes the key.
///
/// # Safety
///
/// - `0 < len <= Cursor::MAX`.
/// - `source` holds `len` initialized records; `destination` has `len`
///   writable slots in a separate allocation.
/// - `cursor` has `MAX_DIGIT_BUCKETS` writable `Cursor` slots.
/// - Without `E::DIRECT`, `bucket_ids` has `len` aligned `Cursor` slots
///   disjoint from both record buffers and the cursor table.
/// - If `key` panics, `destination` holds a partial copy that must be neither
///   read nor dropped.
#[inline]
unsafe fn counting_pass<T, E: Extract<T>>(
    source: *const T,
    destination: *mut T,
    len: usize,
    digit: Digit,
    cursor: *mut Cursor,
    bucket_ids: *mut Cursor,
    extractor: &mut E,
) -> Cursor {
    // SAFETY: the caller supplies initialized source records, disjoint writable
    // output and cursor/cache storage as specified by this function's contract.
    unsafe {
        let mask = digit.mask::<E::Value>();
        let shift = E::Value::from_usize(digit.shift);
        let buckets = 1usize << digit.width;
        for slot in 0..buckets {
            // SAFETY: the caller guarantees room for `buckets` entries.
            *cursor.add(slot) = 0;
        }
        let mut largest = 0 as Cursor;
        for index in 0..len {
            // SAFETY: the caller guarantees `len` initialized records.
            let key = extractor.key(&*source.add(index));
            let bucket = ((key & mask) >> shift).to_usize();
            if E::DIRECT.is_none() {
                // SAFETY: the caller supplies len aligned Cursor slots.
                *bucket_ids.add(index) = bucket as Cursor;
            }
            // SAFETY: the digit mask keeps the index below `buckets`, and no bucket
            // can hold more than `len <= Cursor::MAX` records.
            let count = *cursor.add(bucket) + 1;
            *cursor.add(bucket) = count;
            largest = largest.max(count);
        }
        // SAFETY: the caller guarantees `buckets` writable cursor slots.
        let starts = core::slice::from_raw_parts_mut(cursor, buckets);
        crate::portable_simd::prefix_sum(starts);
        for index in 0..len {
            let record = source.add(index);
            let bucket = if E::DIRECT.is_some() {
                // Only sealed library encoders use the uncached path; their
                // classification is deterministic and cannot panic.
                ((extractor.key(&*record) & mask) >> shift).to_usize()
            } else {
                // SAFETY: histogram initialized every cached classification before
                // any record was copied. Scatter runs no user code on this path.
                *bucket_ids.add(index) as usize
            };
            let slot = *cursor.add(bucket);
            *cursor.add(bucket) = slot.wrapping_add(1);
            let target = if E::DIRECT.is_none() {
                slot as usize
            } else {
                (slot as usize).min(len - 1)
            };
            // SAFETY: cached classifications or deterministic sealed encoders
            // give disjoint output intervals that initialize every output slot.
            core::ptr::write(destination.add(target), core::ptr::read(record));
        }
        largest
    }
}

impl<T, E: Extract<T>> Tile<'_, T, E> {
    /// Reserves `len` scratch records and, when bucket IDs are cached, `len`
    /// aligned `Cursor` slots after them.
    ///
    /// Returns the scratch records and the ID slots, or a null ID pointer when
    /// classification is not cached.
    fn reserve_scratch(&mut self, len: usize) -> (*mut T, *mut Cursor) {
        // Cached IDs share the record allocation, without another allocation
        // or zero initialization.
        if E::DIRECT.is_none() {
            let bytes = core::mem::size_of::<T>();
            let extra = (len * core::mem::size_of::<Cursor>() + core::mem::align_of::<Cursor>()
                - 1)
            .div_ceil(bytes);
            let required = len
                .checked_add(extra)
                .expect("tile scratch capacity overflow");
            if self.scratch.capacity() < required {
                self.scratch.reserve_exact(required - self.scratch.len());
            }
        }
        if self.scratch.len() < len {
            self.scratch.reserve_exact(len - self.scratch.len());
            self.scratch.resize_with(len, MaybeUninit::<T>::uninit);
        }
        let records = self.scratch.as_mut_ptr().cast::<T>();
        let ids = if E::DIRECT.is_none() {
            // SAFETY: capacity includes len records followed by 2*len+1 bytes.
            // The allocation includes worst-case alignment padding after records.
            unsafe {
                let start = records.add(len).cast::<u8>();
                start
                    .add(start.align_offset(core::mem::align_of::<Cursor>()))
                    .cast::<Cursor>()
            }
        } else {
            core::ptr::null_mut()
        };
        (records, ids)
    }

    /// Runs one stable counting pass per digit, the last digit first, and
    /// returns the largest bucket of the first digit.
    ///
    /// The records end in the tile, also when a key function panics. `cursor`
    /// then holds each bucket's end for the first digit.
    // Keep the result at its consumer: LSD discards the histogram maximum,
    // so optimization can remove its per-record update without a second loop.
    // Counter and placement evidence: port.md, "Counting-pass result visibility".
    #[inline(always)]
    fn counting_passes(
        &mut self,
        records: &mut [T],
        digits: &[Digit],
        cursor: &mut MaybeUninit<[Cursor; MAX_DIGIT_BUCKETS]>,
    ) -> Cursor {
        let len = records.len();
        debug_assert!(len <= MAX_TILE_RECORDS);
        let (scratch, bucket_ids) = self.reserve_scratch(len);
        let cursor = cursor.as_mut_ptr().cast::<Cursor>();
        let tile = records.as_mut_ptr();
        let mut guard = RestoreTile {
            tile,
            len,
            source: tile,
        };
        let mut destination = scratch;
        let mut largest = 0;
        for &digit in digits.iter().rev() {
            // SAFETY: scratch and tile are disjoint len-slot buffers, and the
            // guard's source holds the records. Cached IDs use the reserved
            // scratch tail. Extract guarantees safe
            // re-evaluation whenever classification caching is disabled.
            largest = unsafe {
                counting_pass::<T, E>(
                    guard.source,
                    destination,
                    len,
                    digit,
                    cursor,
                    bucket_ids,
                    self.extractor,
                )
            };
            // Only a completed pass makes its output the live record set.
            core::mem::swap(&mut guard.source, &mut destination);
        }
        largest
    }

    /// Partitions a tile through scratch, then finishes each bucket.
    fn partition_tile(&mut self, records: &mut [T], digit: Digit, varying: u128) {
        let len = records.len();
        let mut counts = MaybeUninit::<[Cursor; MAX_DIGIT_BUCKETS]>::uninit();
        let largest = self.counting_passes(records, &[digit], &mut counts);
        // With insertion-sized buckets, whole-tile insertion has the same inversion
        // count as separate bucket sorts and skips the boundary walk.
        if largest as usize <= INSERTION_MAX {
            self.extractor.finish_insertion(records);
            return;
        }
        // Read boundaries from the completed cursors; recomputing them from keys
        // measured 48% slower when there was roughly one bucket per record.
        let candidate = varying & !digit.mask::<E::Value>().to_u128();
        // SAFETY: a completed pass initialized exactly this cursor prefix.
        let ends = unsafe {
            core::slice::from_raw_parts(counts.as_ptr().cast::<Cursor>(), 1usize << digit.width)
        };
        let mut start = 0;
        for &end in ends {
            let end = end as usize;
            debug_assert!(start <= end && end <= len);
            self.finish_child(&mut records[start..end], candidate);
            start = end;
        }
    }

    /// Finishes a partition child. Insertion-sized children skip the variation
    /// scan, which would cost about as much as the insertion pass it could save.
    fn finish_child(&mut self, records: &mut [T], candidate: u128) {
        if candidate == 0 || records.len() < 2 {
            return;
        }
        if records.len() > INSERTION_MAX {
            let varying = scan_varying_bits(records, candidate, self.extractor);
            self.sort_tile(records, varying);
        } else {
            self.extractor.finish_insertion(records);
        }
    }

    /// Finishes a tile by insertion, an MSD partition, or LSD counting passes.
    fn sort_tile(&mut self, records: &mut [T], varying: u128) {
        let len = records.len();
        if len < 2 || varying == 0 {
            return;
        }
        match plan_tile::<T>(len, varying) {
            TilePlan::Insertion => self.extractor.finish_insertion(records),
            TilePlan::Msd(digit) => self.partition_tile(records, digit, varying),
            TilePlan::Lsd { digits, passes } => {
                let mut counts = MaybeUninit::<[Cursor; MAX_DIGIT_BUCKETS]>::uninit();
                self.counting_passes(records, &digits[..passes], &mut counts);
            }
        }
    }
}

/// Tracks which buffer holds the records of a tile between counting passes.
///
/// `source` points at the records written by the last completed pass, either
/// the tile itself or scratch. On drop, including during a key-function panic,
/// records in scratch are copied back so the tile again owns all of them.
struct RestoreTile<T> {
    tile: *mut T,
    len: usize,
    source: *mut T,
}

impl<T> Drop for RestoreTile<T> {
    fn drop(&mut self) {
        if self.source != self.tile {
            // SAFETY: source holds initialized records in scratch; tile holds
            // Copy values or stale non-Copy bytes. The allocations do not overlap.
            unsafe {
                core::ptr::copy_nonoverlapping(self.source, self.tile, self.len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::ByKey;
    #[test]
    fn changing_keys_finish_tile() {
        let mut records: Vec<_> = (0..1000u64).collect();
        let mut scratch = Scratch::new();
        let mut calls = 0;
        // Every sample and histogram puts all records in one bucket. Each child
        // scan then reintroduces every key bit, including the partitioned digit.
        let constant_calls = super::super::plan::KEY_SAMPLE_LEN + records.len();
        let mut extractor = ByKey(|_: &u64| {
            assert!(calls < 12_000, "tile recursion did not make progress");
            let phase = calls % (constant_calls + 65);
            calls += 1;
            if phase < constant_calls || phase.is_multiple_of(2) {
                0
            } else {
                u64::MAX
            }
        });
        Tile::<_, _> {
            scratch: &mut scratch.0,
            extractor: &mut extractor,
        }
        .sort_tile(&mut records, u64::MAX as u128);
        records.sort_unstable();
        assert_eq!(records, (0..1000u64).collect::<Vec<_>>());
    }

    /// Narrowing must retain callback caching through both scatter and recovery.
    fn check_narrowed_callback<const SHIFT: usize>() {
        for panic_at in [None, Some(33)] {
            let mut values: Vec<_> = (0..65).collect();
            let mut records: Vec<_> = values.iter_mut().enumerate().collect();
            let mut calls = 0u128;
            let key = ByKey(|_: &(usize, &mut usize)| {
                calls += 1;
                assert_ne!(Some(calls), panic_at);
                calls << SHIFT
            });
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Tile {
                    scratch: &mut Vec::new(),
                    extractor: &mut crate::key::KeyHalf::<_, SHIFT>(key),
                }
                .partition_tile(&mut records, Digit { shift: 0, width: 6 }, 63);
            }));
            assert_eq!(result.is_err(), panic_at.is_some());
            records.sort_by_key(|record| record.0);
            for (id, (original, value)) in records.iter_mut().enumerate() {
                assert_eq!(id, *original);
                assert_eq!(id, **value);
                **value += 65;
            }
            drop(records);
            assert_eq!(values, (65..130).collect::<Vec<_>>());
        }
    }

    #[test]
    fn narrowed_callbacks_preserve_borrowed_records() {
        check_narrowed_callback::<0>();
        check_narrowed_callback::<64>();
    }

    #[test]
    fn cache_tail_after_odd_records() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        struct Odd([u8; 3]);
        let mut records: Vec<_> = (0..65u16)
            .map(|id| {
                let bits = (id * 251).to_le_bytes();
                Odd([bits[0], bits[1], id as u8])
            })
            .collect();
        let mut expected = records.clone();
        expected.sort_by_key(|x| u16::from_le_bytes([x.0[0], x.0[1]]));
        let mut scratch = Scratch::new();
        let mut key = ByKey(|x: &Odd| u16::from_le_bytes([x.0[0], x.0[1]]));
        Tile::<_, _> {
            scratch: &mut scratch.0,
            extractor: &mut key,
        }
        .partition_tile(
            &mut records,
            Digit {
                shift: 0,
                width: 14,
            },
            0x3fff,
        );
        assert_eq!(records, expected);
    }
}
