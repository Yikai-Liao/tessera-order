// SPDX-License-Identifier: BSD-2-Clause

//! Bounded tile finishing owns scratch layout, cached classification and restoration.
//! Callers supply records, remaining variation and an extractor; raw pass pointers
//! and the live-copy guard never cross this boundary.

use super::plan::{Digit, MAX_DIGIT_BUCKETS, TilePlan, plan_tile};
use super::scan_varying_bits;
use crate::key::{Extract, Unsigned};
use crate::thresholds::{INSERTION_MAX, MAX_TILE_RECORDS};
use alloc::vec::Vec;
use core::mem::MaybeUninit;

/// Converts a byte budget once into the record capacity used by every route.
/// A tile holds at least one record and all its cursors must fit in u16.
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
    pub(super) fn sort<E: Extract<T>, const CACHED: bool>(
        &mut self,
        records: &mut [T],
        varying: u128,
        extractor: &mut E,
    ) {
        debug_assert!(records.len() <= MAX_TILE_RECORDS);
        Tile::<T, E, CACHED> {
            scratch: &mut self.0,
            extractor,
        }
        .sort_tile(records, varying);
    }
}

/// One active tile operation, including recursive MSD children.
struct Tile<'a, T, E, const CACHED: bool> {
    scratch: &'a mut Vec<MaybeUninit<T>>,
    extractor: &'a mut E,
}

/// One stable counting pass: histogram, prefix sum, then scatter from `source`
/// to `destination`. Returns the size of the largest bucket.
///
/// When `CACHED`, the histogram stores each record's bucket and the scatter
/// reads it back, so a key function that changes its answer cannot send two
/// records to one slot. Otherwise the scatter recomputes the key.
///
/// # Safety
///
/// - `0 < len <= u16::MAX`.
/// - `source` holds `len` initialized records; `destination` has `len`
///   writable slots in a separate allocation.
/// - `cursor` has `MAX_DIGIT_BUCKETS` writable `u16` slots.
/// - When `CACHED` and the IDs do not fit after the cursor table, `cache_tail`
///   has `len` aligned `u16` slots disjoint from both record buffers.
/// - When not `CACHED`, `key` is a library encoder: deterministic and
///   panic-free.
/// - If `key` panics, `destination` holds a partial copy that must be neither
///   read nor dropped.
#[inline]
unsafe fn counting_pass<T, E: Extract<T>, const CACHED: bool>(
    source: *const T,
    destination: *mut T,
    len: usize,
    digit: Digit,
    cursor: *mut u16,
    cache_tail: *mut u16,
    extractor: &mut E,
) -> u16 {
    // SAFETY: the caller supplies initialized source records, disjoint writable
    // output and cursor/cache storage as specified by this function's contract.
    unsafe {
        let mask = digit.mask::<E::Value>();
        let shift = E::Value::from_usize(digit.shift);
        let buckets = 1usize << digit.width;
        let bucket_ids = if CACHED && len + buckets <= MAX_DIGIT_BUCKETS {
            cursor.add(buckets)
        } else {
            cache_tail
        };
        for slot in 0..buckets {
            // SAFETY: the caller guarantees room for `buckets` entries.
            *cursor.add(slot) = 0;
        }
        let mut largest = 0u16;
        for index in 0..len {
            // SAFETY: the caller guarantees `len` initialized records.
            let key = extractor.key(&*source.add(index));
            let bucket = ((key & mask) >> shift).to_usize();
            if CACHED {
                // SAFETY: the scratch tail has len aligned u16 slots.
                *bucket_ids.add(index) = bucket as u16;
            }
            // SAFETY: the digit mask keeps the index below `buckets`, and no bucket
            // can hold more than `len <= u16::MAX` records.
            let count = *cursor.add(bucket) + 1;
            *cursor.add(bucket) = count;
            largest = largest.max(count);
        }
        #[cfg(feature = "portable-simd")]
        {
            // SAFETY: the caller guarantees `buckets` writable cursor slots.
            let starts = core::slice::from_raw_parts_mut(cursor, buckets);
            crate::portable_simd::prefix_sum(starts);
        }
        #[cfg(not(feature = "portable-simd"))]
        {
            let mut offset = 0u16;
            for slot in 0..buckets {
                // SAFETY: as above.
                let count = *cursor.add(slot);
                *cursor.add(slot) = offset;
                offset += count;
            }
        }
        for index in 0..len {
            let record = source.add(index);
            let bucket = if !CACHED {
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
            let target = if CACHED {
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

impl<T, E: Extract<T>, const CACHED: bool> Tile<'_, T, E, CACHED> {
    /// Reserves `len` scratch records and, when bucket IDs are cached and do not
    /// fit after `buckets` cursors, `len` aligned `u16` slots after them.
    ///
    /// Returns the scratch records and the ID slots, or a null ID pointer when
    /// the IDs fit in the cursor table.
    fn reserve_scratch(&mut self, len: usize, buckets: usize) -> (*mut T, *mut u16) {
        // Cached IDs first use the unused suffix of the cursor table. Larger
        // caches share the record allocation, without another allocation or
        // zero initialization.
        let tail = CACHED && len + buckets > MAX_DIGIT_BUCKETS;
        if tail {
            let bytes = core::mem::size_of::<T>();
            let extra = (len * core::mem::size_of::<u16>() + 1).div_ceil(bytes);
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
        let ids = if tail {
            // SAFETY: capacity includes len records followed by 2*len+1 bytes.
            // u16 alignment needs at most one byte; the regions do not overlap.
            unsafe {
                let start = records.add(len).cast::<u8>();
                start
                    .add(start.align_offset(core::mem::align_of::<u16>()))
                    .cast::<u16>()
            }
        } else {
            core::ptr::null_mut()
        };
        (records, ids)
    }

    /// Partitions a tile through scratch, then finishes each bucket.
    fn partition_tile(&mut self, records: &mut [T], digit: Digit, varying: u128) {
        let len = records.len();
        let (scratch, cache_tail) = self.reserve_scratch(len, 1 << digit.width);
        let mut counts = MaybeUninit::<[u16; MAX_DIGIT_BUCKETS]>::uninit();
        let counts_ptr = counts.as_mut_ptr().cast::<u16>();
        let tile = records.as_mut_ptr();
        let mut guard = RestoreTile {
            tile,
            len,
            source: tile,
        };
        let mut destination = scratch;
        // SAFETY: both buffers have len slots and the cursor/cache regions
        // satisfy pass requirements; uncached mode uses only sealed encoders.
        let largest = unsafe {
            guard.pass::<E, CACHED>(
                &mut destination,
                digit,
                counts_ptr,
                cache_tail,
                self.extractor,
            )
        };

        drop(guard);
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
        let ends = unsafe { core::slice::from_raw_parts(counts_ptr, 1usize << digit.width) };
        let mut start = 0;
        for &end in ends {
            let end = end as usize;
            debug_assert!(start <= end && end <= len);
            // Insertion-sized children need no variation scan.
            if end - start > INSERTION_MAX {
                let child = &records[start..end];
                let varying = scan_varying_bits(child, candidate, self.extractor);
                self.sort_tile(&mut records[start..end], varying);
            } else if end > start + 1 {
                self.extractor.finish_insertion(&mut records[start..end]);
            }
            start = end;
        }
    }

    /// Finishes a tile by insertion, an MSD partition, or LSD counting passes.
    fn sort_tile(&mut self, records: &mut [T], varying: u128) {
        let len = records.len();
        if len < 2 || varying == 0 {
            return;
        }
        let (digits, passes) = match plan_tile(records, varying, self.extractor) {
            TilePlan::Insertion => {
                self.extractor.finish_insertion(records);
                return;
            }
            TilePlan::Msd(digit) => {
                self.partition_tile(records, digit, varying);
                return;
            }
            TilePlan::Lsd { digits, passes } => (digits, passes),
        };
        let buckets = if CACHED {
            1 << digits[..passes]
                .iter()
                .map(|digit| digit.width)
                .max()
                .unwrap()
        } else {
            0
        };
        let (scratch_ptr, cache_tail) = self.reserve_scratch(len, buckets);
        debug_assert!(len <= MAX_TILE_RECORDS);
        let mut counts = MaybeUninit::<[u16; MAX_DIGIT_BUCKETS]>::uninit();
        let counts_ptr = counts.as_mut_ptr().cast::<u16>();
        let tile = records.as_mut_ptr();
        let mut guard = RestoreTile {
            tile,
            len,
            source: tile,
        };
        let mut destination = scratch_ptr;
        for &digit in digits[..passes].iter().rev() {
            // SAFETY: scratch and tile are disjoint len-slot buffers. Cached
            // IDs fit in the cursor suffix or the reserved scratch tail.
            unsafe {
                guard.pass::<E, CACHED>(
                    &mut destination,
                    digit,
                    counts_ptr,
                    cache_tail,
                    self.extractor,
                )
            };
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

impl<T> RestoreTile<T> {
    /// Runs one counting pass from `source` into `*destination`, then swaps
    /// the two so `source` names the completed output.
    ///
    /// # Safety
    ///
    /// The requirements of [`counting_pass`], with `self.source` as the source
    /// and `*destination` as the destination.
    #[inline]
    unsafe fn pass<E: Extract<T>, const CACHED: bool>(
        &mut self,
        destination: &mut *mut T,
        digit: Digit,
        cursor: *mut u16,
        cache_tail: *mut u16,
        extractor: &mut E,
    ) -> u16 {
        // SAFETY: the caller supplies initialized source records, disjoint writable
        // output and cursor/cache storage as specified by this function's contract.
        unsafe {
            let largest = counting_pass::<T, E, CACHED>(
                self.source,
                *destination,
                self.len,
                digit,
                cursor,
                cache_tail,
                extractor,
            );
            core::mem::swap(&mut self.source, destination);
            largest
        }
    }
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
            if phase < constant_calls || phase % 2 == 0 {
                0
            } else {
                u64::MAX
            }
        });
        Tile::<_, _, true> {
            scratch: &mut scratch.0,
            extractor: &mut extractor,
        }
        .sort_tile(&mut records, u64::MAX as u128);
        records.sort_unstable();
        assert_eq!(records, (0..1000u64).collect::<Vec<_>>());
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
        Tile::<_, _, true> {
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
