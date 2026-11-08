// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2025, 2026 Robert Clausecker <clausecker@zib.de>
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
// IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
// TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
// PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
// LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
// NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
// SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//

//! Stable block LSD radix sort adapted from Robert Clausecker's
//! BSD-2-Clause C implementation, `radixsort_permuted.c`.
//! Reference revision: f69e816c3cd79d312cd67aea5b9cf1c338c1b371.
//! <https://github.com/clausecker/radsort/blob/f69e816c3cd79d312cd67aea5b9cf1c338c1b371/radixsort_permuted.c>
//! <https://arxiv.org/abs/2607.05302>
//!
//! The engine uses one pass of this sort as a stable partition by one digit
//! ([`partition_digit`]) or by a caller's classification ([`partition_by`]).
//! Buckets are filled block by block, consumed input blocks are recycled as
//! output blocks, and a block order records the logical sequence, so a pass
//! needs O(√n) scratch instead of a second buffer of the input's size.
//!
//! Ownership: the active block sequence holds exactly one live copy of every
//! record. Other physical copies are stale bytes that are never dropped or
//! borrowed, and key extraction borrows only live records. Scratch is
//! `MaybeUninit<T>`. Before returning, or before resuming a key-function panic,
//! `compact` moves every live record back into the input slice, so records
//! need neither `Copy` nor `Clone`.

use crate::key::{Extract, Unsigned};
use crate::unwind;
use alloc::vec::Vec;
use core::{marker::PhantomData, mem::MaybeUninit, ptr};

/// Widest digit, in bits, that one pass accepts. Callers that choose their own
/// digit width (the Tessera engine) bound it by this constant.
pub(crate) const MAX_RADIX_BITS: usize = 12;

/// Logical input and output block cursors for the current pass.
struct ScatterPosition {
    input: usize,
    output: usize,
}

/// Bytes of block tables the partition keeps per block position.
///
/// The block order is a permutation of the physical blocks. It is stored twice,
/// so a pass can write the next order while reading the current one, and every
/// output block carries the id of the bucket that filled it. Rebuilding the
/// order from those two arrays is what keeps the scheme stable without a
/// full-size output buffer.
const DESCRIPTOR_BYTES_PER_POSITION: usize =
    2 * core::mem::size_of::<u32>() + core::mem::size_of::<u16>();

/// Smallest block, counted in bytes of records.
///
/// The compaction pass moves a block with one copy, so a block has to hold
/// enough bytes for a copy to be worth its own overhead.
const BLOCK_BYTES_FLOOR: usize = 1024;

/// Multiple of the balanced block size that the engine uses.
///
/// The balanced size minimises memory, not time: compaction reads a block at a
/// random address, and that read does not get cheaper with a smaller block. A
/// multiple amortises it, at a multiple of the minimum memory to match.
const BLOCK_SCALE: usize = 4;

/// Smallest block, counted in records.
///
/// The bookkeeping of a block — its order entry, its bucket id and one copy
/// call — costs the same whatever the record width, so a floor in records keeps
/// it amortised for wide records, where the byte floor alone would allow only a
/// handful of records per block.
const BLOCK_RECORDS_FLOOR: usize = 32;

/// Block size in records for a digit partition into `radix`-bit digits.
///
/// The partition fills its buckets block by block, so it holds the records it
/// moves in blocks of `b` records: `2 * R` of those blocks for `R` buckets, one
/// output block per bucket and as many in reserve, because an input block may
/// only be recycled as an output block once every record in it has been read.
/// The tables are indexed by block position and cost
/// [`DESCRIPTOR_BYTES_PER_POSITION`] bytes per position; records move by
/// exchanging blocks, not by copying into a second buffer. For `n` records of
/// `s` bytes, a block size `b` therefore costs
///
/// ```text
/// 2 R b s + 10 n / b
/// ```
///
/// bytes of auxiliary memory, and the two terms are equal at
/// `b = sqrt(10 n / (2 R s))`. That root minimizes the sum: writing `b = c b*`
/// turns the total into `(c + 1 / c) / 2` times the minimum, which grows in
/// both directions. The engine takes [`BLOCK_SCALE`] times the root to amortise
/// the per-block work, and the floors below bound the small inputs where the
/// root alone would buy nothing.
///
/// # Memory
///
/// Two floors keep the per-block work amortised: a block holds at least
/// [`BLOCK_BYTES_FLOOR`] bytes of records, and at least
/// [`BLOCK_RECORDS_FLOOR`] records whatever their width. Their own cost does not
/// depend on `n` — together they reserve at most `2 R max(1024, 32 s)` bytes of
/// scratch and `O(R)` table entries — so the auxiliary memory stays
/// `O(sqrt(n))` in `n` for a fixed record size and digit width.
pub(crate) fn default_block_size(len: usize, record_bytes: usize, radix: usize) -> usize {
    let floors = (BLOCK_BYTES_FLOOR / record_bytes).max(BLOCK_RECORDS_FLOOR);
    balanced_block_size(len, record_bytes, radix)
        .saturating_mul(BLOCK_SCALE)
        .max(floors)
        .max(1)
}

/// The balanced block size, rounded up.
///
/// `std` computes it as `ceil(sqrt(10 n / (2 R s)))`. `core` has neither
/// `f64::sqrt` nor `f64::ceil`, so an integer solve of
/// `m * m * (2 R s) >= 10 n` stands in for it. Both forms agree wherever the
/// caller can reach them, which `balanced_forms_agree` in this file's tests
/// checks.
#[cfg(feature = "std")]
fn balanced_block_size(len: usize, record_bytes: usize, radix: usize) -> usize {
    let range = 1usize << radix;
    let tables = DESCRIPTOR_BYTES_PER_POSITION as f64 * len as f64;
    let scratch_per_record = 2.0 * range as f64 * record_bytes as f64;
    (tables / scratch_per_record).sqrt().ceil() as usize
}

#[cfg(not(feature = "std"))]
fn balanced_block_size(len: usize, record_bytes: usize, radix: usize) -> usize {
    balanced_block_size_integer(len, record_bytes, radix)
}

/// The integer form of [`balanced_block_size`].
///
/// A standard build compiles it only to compare it against the floating-point
/// form in tests.
#[cfg(any(not(feature = "std"), test))]
fn balanced_block_size_integer(len: usize, record_bytes: usize, radix: usize) -> usize {
    let numerator = DESCRIPTOR_BYTES_PER_POSITION as u128 * len as u128;
    let denominator = 2 * (1u128 << radix) * record_bytes as u128;
    let mut balanced = (numerator / denominator).isqrt();
    // `isqrt` floors, so at most one step moves the product to or past
    // `numerator`. `record_bytes` is at least one, so no division by zero.
    if balanced * balanced * denominator < numerator {
        balanced += 1;
    }
    balanced as usize
}

/// Called on a bucket as soon as its records are contiguous in the input.
type Visit<'a, T> = &'a mut dyn FnMut(&mut [T]);

/// Partitions `records` stably by the `radix`-bit digit of its keys at `shift`.
///
/// Digit bits above the key width count as zero.
///
/// # Panics
///
/// Panics if `radix` is not in `1..=MAX_RADIX_BITS` or `shift` is not below
/// the key width. A key-function panic is resumed after every record is back
/// in `records`.
pub(crate) fn partition_digit<T, E: Extract<T>>(
    records: &mut [T],
    radix: usize,
    shift: usize,
    extractor: &mut E,
    buffers: &mut Buffers<T>,
) {
    partition_digit_inner(records, radix, shift, extractor, buffers, None);
}

/// [`partition_digit`] that also returns each bucket's cumulative end in
/// records, and calls `visit` on each bucket as soon as its records are
/// contiguous in `records`, while they are still in cache.
///
/// Returns an empty vector for inputs of fewer than two records or zero-sized
/// records. A `visit` panic is resumed after every record is back in `records`.
pub(crate) fn partition_digit_with_ends<T, E: Extract<T>>(
    records: &mut [T],
    radix: usize,
    shift: usize,
    extractor: &mut E,
    buffers: &mut Buffers<T>,
    visit: &mut dyn FnMut(&mut [T], &mut E),
) -> Vec<usize> {
    partition_digit_inner(records, radix, shift, extractor, buffers, Some(visit))
}

/// A [`Visit`] that also receives the extractor, which classification has
/// finished borrowing by the time buckets are visited.
type KeyVisit<'a, T, E> = &'a mut dyn FnMut(&mut [T], &mut E);

fn partition_digit_inner<T, E: Extract<T>>(
    records: &mut [T],
    radix: usize,
    shift: usize,
    extractor: &mut E,
    buffers: &mut Buffers<T>,
    visit: Option<KeyVisit<'_, T, E>>,
) -> Vec<usize> {
    let record_bytes = core::mem::size_of::<T>();
    if records.len() < 2 || record_bytes == 0 {
        return Vec::new();
    }
    let bits = E::Value::BITS;
    assert!((1..=MAX_RADIX_BITS).contains(&radix));
    assert!(shift < bits);
    let width = radix.min(bits - shift);
    let shift = E::Value::from_usize(shift);
    let mask = E::Value::from_usize((1usize << width) - 1) << shift;
    partition_pass(
        records,
        1 << radix,
        extractor,
        buffers,
        move |extractor, record| ((extractor.key(record) & mask) >> shift).to_usize(),
        visit.is_some(),
        visit,
    )
}

/// Partitions `records` stably into `buckets` buckets chosen by `classify` and
/// returns each bucket's cumulative end in records.
///
/// Classes at or above `buckets` go to the last bucket.
///
/// # Panics
///
/// Panics if `buckets` is not a power of two in `2..=1 << MAX_RADIX_BITS`. A
/// key-function panic is resumed after every record is back in `records`.
pub(crate) fn partition_by<T, E: Extract<T>>(
    records: &mut [T],
    buckets: usize,
    extractor: &mut E,
    buffers: &mut Buffers<T>,
    mut classify: impl FnMut(&mut E, &T) -> usize,
) -> Vec<usize> {
    let record_bytes = core::mem::size_of::<T>();
    if records.len() < 2 || record_bytes == 0 {
        return alloc::vec![0; buckets];
    }
    assert!(buckets.is_power_of_two() && (2..=1 << MAX_RADIX_BITS).contains(&buckets));
    partition_pass(
        records,
        buckets,
        extractor,
        buffers,
        move |extractor, record| classify(extractor, record).min(buckets - 1),
        true,
        None,
    )
}

/// Runs one block pass and compacts the result back into `records`.
///
/// Returns bucket ends when `want_ends` is set, and an empty vector otherwise.
fn partition_pass<T, E: Extract<T>>(
    records: &mut [T],
    buckets: usize,
    extractor: &mut E,
    buffers: &mut Buffers<T>,
    mut classify: impl FnMut(&mut E, &T) -> usize,
    want_ends: bool,
    visit: Option<KeyVisit<'_, T, E>>,
) -> Vec<usize> {
    let radix = buckets.trailing_zeros() as usize;
    let block_size = default_block_size(records.len(), core::mem::size_of::<T>(), radix);
    let mut sorter = Partition::new(records, buckets, block_size, buffers);
    let classify_extractor = &mut *extractor;
    let (panic, ends) = sorter.step_with_ends(
        move |record| classify(classify_extractor, record),
        want_ends,
    );
    if let Some(payload) = panic {
        sorter.compact();
        unwind::resume(payload);
    }
    // Classification has finished borrowing the extractor before any tile
    // callback starts; both phases use the same FnMut state in sequence.
    let panic = if let Some(visit) = visit {
        let mut callback = |child: &mut [T]| visit(child, extractor);
        sorter.compact_with(&ends, Some(&mut callback))
    } else {
        sorter.compact_with(&ends, None)
    };
    if let Some(payload) = panic {
        unwind::resume(payload);
    }
    ends
}

/// Returns the key bits that differ between at least two records of `records`.
///
/// # Panics
///
/// Panics if `records` is empty.
pub(crate) fn varying_key_bits<T, E: Extract<T>>(records: &[T], extractor: &mut E) -> u128 {
    // The C reference scatters every digit, even digits that are constant in
    // every key. The union and intersection of all keys find those digits
    // without a histogram. A bit varies if it is set in the union but not in
    // the intersection.
    let mut union = extractor.key(&records[0]);
    let mut intersection = union;
    let full_mask = u128::MAX >> (128 - E::Value::BITS);
    // Wide random keys vary in every bit within the first few records, so
    // stop after a 1,025-record prefix if they already do. Otherwise finish
    // with one uninterrupted, vectorizable scan.
    let boundary = records.len().min(1025);
    for item in &records[1..boundary] {
        let key = extractor.key(item);
        union |= key;
        intersection = intersection & key;
    }
    if union.to_u128() ^ intersection.to_u128() != full_mask {
        for item in &records[boundary..] {
            let key = extractor.key(item);
            union |= key;
            intersection = intersection & key;
        }
    }
    union.to_u128() ^ intersection.to_u128()
}

/// Branch hint for a condition that is almost never true, without a nightly
/// feature. The cold marker tells the optimiser to keep this path out of line,
/// so the hot loop falls through on the common case instead of taking a jump
/// over the rare one. Same shape as the `branches` crate.
#[cold]
#[inline(never)]
fn cold_path() {}

#[inline]
fn unlikely(condition: bool) -> bool {
    if condition {
        cold_path();
    }
    condition
}

/// A block whose live records end before the block does.
#[derive(Clone, Copy)]
struct PartialBlock {
    /// Logical position of the block in the block order.
    order_index: usize,
    /// Only this prefix contains initialized records.
    len: usize,
}

/// A bucket's write cursor in its current output block.
struct BucketWriter<T> {
    next: *mut T,
    end: *mut T,
}

/// Physical block storage: the input slice and a scratch allocation.
///
/// Physical block IDs number the scratch blocks first, then the full input
/// blocks. Unlike the C reference's integer pairs, `T` may have no valid
/// all-zero value, so scratch stays uninitialized and only recorded live
/// prefixes are read. `input_borrow` keeps the input exclusively borrowed
/// while raw pointers address it.
struct BlockStorage<'a, T> {
    input: *mut T,
    input_len: usize,
    block_size: usize,
    scratch_blocks: usize,
    block_count: usize,
    scratch: Vec<MaybeUninit<T>>,
    input_borrow: PhantomData<&'a mut [T]>,
}

impl<'a, T> BlockStorage<'a, T> {
    fn new(
        input: &'a mut [T],
        bucket_count: usize,
        block_size: usize,
        mut scratch: Vec<MaybeUninit<T>>,
    ) -> Self {
        let scratch_blocks = 2 * bucket_count;
        let full_blocks = input.len() / block_size;
        let block_count = full_blocks
            .checked_add(scratch_blocks)
            .expect("block count overflow");
        assert!(block_count <= u32::MAX as usize);
        let scratch_len = scratch_blocks
            .checked_mul(block_size)
            .expect("scratch size overflow");
        scratch.resize_with(scratch_len, MaybeUninit::<T>::uninit);
        // The input tail has no full physical block. Start it in scratch;
        // the first bucket_count scratch blocks remain free for output.
        let tail_len = input.len() % block_size;
        // SAFETY: the source has tail_len initialized records and the disjoint
        // scratch destination has room for at least that many records.
        unsafe {
            ptr::copy_nonoverlapping(
                input.as_ptr().add(full_blocks * block_size),
                scratch
                    .as_mut_ptr()
                    .cast::<T>()
                    .add(bucket_count * block_size),
                tail_len,
            );
        }
        Self {
            input: input.as_mut_ptr(),
            input_len: input.len(),
            block_size,
            scratch_blocks,
            block_count,
            scratch,
            input_borrow: PhantomData,
        }
    }

    fn block_ptr(&mut self, physical: usize) -> *mut T {
        assert!(physical < self.block_count);
        // SAFETY: the bounds check selects a full block in one of the two
        // allocations, which stay alive and are never resized during sorting.
        unsafe {
            if physical < self.scratch_blocks {
                self.scratch
                    .as_mut_ptr()
                    .cast::<T>()
                    .add(physical * self.block_size)
            } else {
                self.input
                    .add((physical - self.scratch_blocks) * self.block_size)
            }
        }
    }
}

/// Allocations a block partition reuses across passes within one sort.
///
/// Each pass resizes and rewrites what it uses.
pub(crate) struct Buffers<T> {
    scratch: Vec<MaybeUninit<T>>,
    block_order: Vec<u32>,
    next_block_order: Vec<u32>,
    bucket_ids: Vec<u16>,
    partial_blocks: Vec<PartialBlock>,
    writers: Vec<BucketWriter<T>>,
    counts: Vec<usize>,
    #[cfg(debug_assertions)]
    seen: Vec<bool>,
    ends: Vec<Vec<usize>>,
}

impl<T> Buffers<T> {
    pub(crate) fn new() -> Self {
        Self {
            scratch: Vec::new(),
            block_order: Vec::new(),
            next_block_order: Vec::new(),
            bucket_ids: Vec::new(),
            partial_blocks: Vec::new(),
            writers: Vec::new(),
            counts: Vec::new(),
            #[cfg(debug_assertions)]
            seen: Vec::new(),
            ends: Vec::new(),
        }
    }

    /// Keeps the allocation of a bucket-end vector for a later pass.
    pub(crate) fn recycle_ends(&mut self, mut ends: Vec<usize>) {
        ends.clear();
        self.ends.push(ends);
    }
}

impl<T> Default for Buffers<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// The state of one block partition over an input slice.
///
/// At pass boundaries, `block_order` maps logical order to physical storage.
/// Its `bucket_count..active_blocks_end` range holds the current record
/// sequence; the prefix and suffix are free blocks. `partial_blocks` records
/// the blocks whose live prefix is shorter than a block.
struct Partition<'a, 'w, T> {
    buffers: &'w mut Buffers<T>,
    blocks: BlockStorage<'a, T>,
    bucket_count: usize,
    block_order: Vec<u32>,
    next_block_order: Vec<u32>,
    /// Bucket ID of each output block, used to build `next_block_order` after
    /// scatter. `u16` holds bucket IDs for digit widths up to `MAX_RADIX_BITS`.
    bucket_ids: Vec<u16>,
    partial_blocks: Vec<PartialBlock>,
    active_blocks_end: usize,
    input_materialized: bool,
}

impl<'a, 'w, T> Partition<'a, 'w, T> {
    fn new(
        input: &'a mut [T],
        bucket_count: usize,
        block_size: usize,
        buffers: &'w mut Buffers<T>,
    ) -> Self {
        let full_blocks = input.len() / block_size;
        let tail_len = input.len() % block_size;
        let blocks = BlockStorage::new(
            input,
            bucket_count,
            block_size,
            core::mem::take(&mut buffers.scratch),
        );
        let active_blocks_end = bucket_count + full_blocks + 1;
        let mut block_order = core::mem::take(&mut buffers.block_order);
        block_order.resize(blocks.block_count, 0);
        for (i, block) in block_order.iter_mut().enumerate() {
            *block = if i < bucket_count {
                i
            } else if i < active_blocks_end - 1 {
                i + bucket_count
            } else {
                i - (active_blocks_end - 1) + bucket_count
            } as u32;
        }
        let mut partial_blocks = core::mem::take(&mut buffers.partial_blocks);
        partial_blocks.clear();
        partial_blocks.resize(
            bucket_count,
            PartialBlock {
                order_index: blocks.block_count,
                len: 0,
            },
        );
        partial_blocks[0] = PartialBlock {
            order_index: active_blocks_end - 1,
            len: tail_len,
        };
        let block_count = blocks.block_count;
        let mut next_block_order = core::mem::take(&mut buffers.next_block_order);
        next_block_order.resize(block_count, 0);
        let mut bucket_ids = core::mem::take(&mut buffers.bucket_ids);
        bucket_ids.resize(block_count, 0);
        Self {
            buffers,
            blocks,
            bucket_count,
            block_order,
            next_block_order,
            bucket_ids,
            partial_blocks,
            active_blocks_end,
            input_materialized: false,
        }
    }

    fn next_block_len(&self, logical: usize, partial: &mut usize) -> usize {
        if *partial < self.partial_blocks.len()
            && self.partial_blocks[*partial].order_index == logical
        {
            let length = self.partial_blocks[*partial].len;
            *partial += 1;
            length
        } else {
            self.blocks.block_size
        }
    }

    #[cfg(debug_assertions)]
    fn validate(&mut self) {
        let seen = &mut self.buffers.seen;
        seen.clear();
        seen.resize(self.block_order.len(), false);
        for &physical in &self.block_order {
            assert!(!seen[physical as usize]);
            seen[physical as usize] = true;
        }
        assert!(self.active_blocks_end <= self.block_order.len());
        assert!(
            self.partial_blocks
                .windows(2)
                .all(|p| p[0].order_index < p[1].order_index)
        );
        assert!(
            self.partial_blocks
                .iter()
                .all(|p| p.order_index >= self.bucket_count
                    && p.order_index < self.active_blocks_end
                    && p.len < self.blocks.block_size)
        );
        let mut partial = 0;
        let total: usize = (self.bucket_count..self.active_blocks_end)
            .map(|i| self.next_block_len(i, &mut partial))
            .sum();
        assert_eq!(total, self.blocks.input_len);
    }

    /// Scatters one pass, optionally returning cumulative ends for recursion.
    fn step_with_ends<F: FnMut(&T) -> usize>(
        &mut self,
        classify: F,
        want_ends: bool,
    ) -> (Option<unwind::Payload>, Vec<usize>) {
        let (mut buckets, mut counts) = self.begin_pass();
        let mut position = ScatterPosition {
            input: self.bucket_count,
            output: self.bucket_count,
        };
        // The classifier is moved into the closure, not borrowed: a digit closure
        // keeps its mask and shift in registers only when the scatter owns it. A
        // &mut here cost 40 % on a digit-only sort (sparse8, 10M).
        let panic = unwind::catch(|| {
            self.scatter(&mut buckets, &mut counts, &mut position, 0, classify);
        })
        .err();
        if panic.is_some() {
            // Reconstruct the offset in the current input block: records written
            // across all buckets minus records in fully consumed input blocks.
            let written: usize = buckets
                .iter()
                .zip(&counts)
                .map(|(bucket, &blocks)| {
                    // SAFETY: both pointers delimit the same reserved block.
                    let free = unsafe { bucket.end.offset_from(bucket.next) as usize };
                    blocks * self.blocks.block_size - free
                })
                .sum();
            let mut partial = 0;
            let before: usize = (self.bucket_count..position.input)
                .map(|input| self.next_block_len(input, &mut partial))
                .sum();
            let offset = written - before;
            // Do not call a failed extractor again. Appending every unread
            // record to bucket zero completes a valid (unsorted) pass. Recycling
            // still follows the original read-before-reserve protocol.
            self.scatter(&mut buckets, &mut counts, &mut position, offset, |_| 0);
        }
        self.active_blocks_end = position.output;
        let ends = self.finish_pass(&buckets, counts, want_ends);
        self.buffers.writers = buckets;
        #[cfg(debug_assertions)]
        self.validate();
        (panic, ends)
    }

    /// Reserves the first `bucket_count` free blocks, one per bucket, and gives
    /// every bucket an empty write cursor. Counts start at one because that
    /// block is already assigned to the bucket.
    fn begin_pass(&mut self) -> (Vec<BucketWriter<T>>, Vec<usize>) {
        let mut buckets = core::mem::take(&mut self.buffers.writers);
        buckets.clear();
        buckets.reserve(self.bucket_count);
        for i in 0..self.bucket_count {
            let out = self.blocks.block_ptr(self.block_order[i] as usize);
            buckets.push(BucketWriter {
                next: out,
                // SAFETY: block_ptr() returns storage for block_size elements.
                end: unsafe { out.add(self.blocks.block_size) },
            });
            self.bucket_ids[i] = i as u16;
        }
        let mut counts = core::mem::take(&mut self.buffers.counts);
        counts.clear();
        counts.resize(self.bucket_count, 1);
        (buckets, counts)
    }

    /// Rebuilds the logical block order from the order the buckets filled,
    /// records each bucket's partially filled last block, and returns each
    /// bucket's cumulative record end.
    fn finish_pass(
        &mut self,
        buckets: &[BucketWriter<T>],
        counts: Vec<usize>,
        want_ends: bool,
    ) -> Vec<usize> {
        // Record boundaries are not block boundaries: a bucket's last block is
        // usually partial, and a caller that wants to recurse on bucket slices
        // needs the record offsets. The cursor knows how much of that last
        // block is free, so this is one pass over the bucket table, not a pass
        // over the records.
        let mut record_ends = if want_ends {
            self.buffers.ends.pop().unwrap_or_default()
        } else {
            Vec::new()
        };
        if want_ends {
            record_ends.clear();
            record_ends.reserve_exact(counts.len());
            let mut total = 0usize;
            for (bucket, &blocks) in buckets.iter().zip(counts.iter()) {
                // SAFETY: both pointers delimit the same bucket's output block.
                let free = unsafe { bucket.end.offset_from(bucket.next) as usize };
                total += blocks * self.blocks.block_size - free;
                record_ends.push(total);
            }
        }
        // A separate starts Vec would allocate once per digit. Counts are dead
        // after scatter, so reuse their allocation for prefix offsets and then
        // advance those offsets while rebuilding the block order.
        let mut starts = counts;
        let mut start = self.bucket_count;
        for count in &mut starts {
            let next = start + *count;
            *count = start;
            start = next;
        }
        for i in 0..self.active_blocks_end {
            let b = self.bucket_ids[i] as usize;
            let j = starts[b];
            starts[b] += 1;
            self.next_block_order[j] = self.block_order[i];
            let base = self.blocks.block_ptr(self.block_order[i] as usize);
            // SAFETY: next/end belong to the same block-sized allocation.
            if unsafe { base.add(self.blocks.block_size) } == buckets[b].end {
                self.partial_blocks[b] = PartialBlock {
                    order_index: j,
                    // SAFETY: next points within the block starting at base.
                    len: unsafe { buckets[b].next.offset_from(base) as usize },
                };
            }
        }
        assert!(self.active_blocks_end + self.bucket_count <= self.block_order.len());
        self.next_block_order[..self.bucket_count].copy_from_slice(
            &self.block_order[self.active_blocks_end..self.active_blocks_end + self.bucket_count],
        );
        self.next_block_order[self.active_blocks_end + self.bucket_count..]
            .copy_from_slice(&self.block_order[self.active_blocks_end + self.bucket_count..]);
        self.active_blocks_end += self.bucket_count;
        core::mem::swap(&mut self.block_order, &mut self.next_block_order);
        self.buffers.counts = starts;
        record_ends
    }

    // Pass setup and block reordering need state that scatter does not.
    // Keep their optimization regions separate so that state does not force
    // digit masks and bucket cursors out of registers in the hot loop.
    #[inline(never)]
    fn scatter<F: FnMut(&T) -> usize>(
        &mut self,
        buckets: &mut [BucketWriter<T>],
        counts: &mut [usize],
        position: &mut ScatterPosition,
        first_offset: usize,
        mut classify: F,
    ) {
        let mut begin = first_offset;
        let mut partial = self
            .partial_blocks
            .partition_point(|p| p.order_index < position.input);
        while position.input < self.active_blocks_end {
            let input = position.input;
            let source = self.blocks.block_ptr(self.block_order[input] as usize);
            let length = self.next_block_len(input, &mut partial);
            let mut write = |offset: usize| {
                // SAFETY: only the initialized, unread suffix is read. Blocks
                // become output only after all of their input records are read.
                // Borrow the actual live record. Never materialize a second T:
                // it may own a String, Box, Rc, or other non-Copy payload.
                // This borrow ends before the bitwise move below.
                let b = classify(unsafe { &*source.add(offset) });
                // SAFETY: bucket_count is 1 << radix and mask keeps the width
                // bits below radix, so the index is in bounds; the check would
                // otherwise run once per record in the hot loop.
                let bucket = unsafe { buckets.get_unchecked_mut(b) };
                let next = bucket.next;
                let end = bucket.end;
                // SAFETY: next is an unused cell of a reserved output block;
                // source and destination may overlap, so use ptr::copy.
                let next = unsafe {
                    ptr::copy(source.add(offset), next, 1);
                    next.add(1)
                };
                bucket.next = next;
                if unlikely(next == end) {
                    debug_assert!(position.output <= input);
                    let out = self
                        .blocks
                        .block_ptr(self.block_order[position.output] as usize);
                    *bucket = BucketWriter {
                        next: out,
                        // SAFETY: block_ptr reserves a complete block.
                        end: unsafe { out.add(self.blocks.block_size) },
                    };
                    self.bucket_ids[position.output] = b as u16;
                    counts[b] += 1;
                    position.output += 1;
                }
            };
            // Four records per bucket-cursor reload. Measured cheaper than one
            // record at a time on 8 byte records; the size scaling keeps the
            // reload amortization for wider records without spilling cursors.
            let group =
                (4 * core::mem::size_of::<usize>() / core::mem::size_of::<T>().max(1)).clamp(1, 4);
            let complete = begin + (length - begin) / group * group;
            for start in (begin..complete).step_by(group) {
                for offset in 0..group {
                    write(start + offset);
                }
            }
            for offset in complete..length {
                write(offset);
            }
            position.input += 1;
            begin = 0;
        }
    }

    /// Moves every live record back into the input, without callbacks.
    fn compact(&mut self) {
        let _ = self.compact_with(&[], None);
    }

    /// Materializes the active sequence into input. When bucket ends are given,
    /// `visit` is called on each bucket as soon as its records are contiguous in
    /// input, so a leaf can consume the bucket while it is still cache-hot.
    /// A callback panic does not interrupt materialization: the remaining
    /// buckets are copied without callbacks and the payload is returned after
    /// the input slice is a complete permutation again.
    fn compact_with(
        &mut self,
        ends: &[usize],
        mut visit: Option<Visit<'_, T>>,
    ) -> Option<unwind::Payload> {
        // Sorting is finished. Reuse the output buffer as the inverse map:
        // physical block ID -> position in block_order.
        let mut block_positions = core::mem::take(&mut self.next_block_order);
        for i in 0..self.block_order.len() {
            block_positions[self.block_order[i] as usize] = i as u32;
        }
        let input_ptr = self.blocks.input;
        let mut remaining = ends;
        let mut begin = 0;
        let mut callback_panic = None;
        let mut visit_due = |written: usize| {
            let Some(callback) = visit.take() else {
                return;
            };
            while let Some((&end, tail)) = remaining.split_first() {
                if end > written {
                    break;
                }
                if end > begin {
                    // SAFETY: this bucket is fully materialized and no longer a
                    // copy source; its input slice does not overlap scratch.
                    let bucket = unsafe {
                        core::slice::from_raw_parts_mut(input_ptr.add(begin), end - begin)
                    };
                    if let Err(payload) = unwind::catch(|| callback(bucket)) {
                        callback_panic = Some(payload);
                        return;
                    }
                }
                begin = end;
                remaining = tail;
            }
            visit = Some(callback);
        };
        let mut start = 0;
        let mut partial = 0;
        let mut free_logical = 0;
        let mut free_physical = self.block_order[free_logical] as usize;
        for destination in self.blocks.scratch_blocks..self.block_order.len() {
            let input = destination - self.bucket_count;
            let mut output = block_positions[destination] as usize;
            let source = self.block_order[input] as usize;
            if output > input && output < self.active_blocks_end {
                debug_assert!(
                    free_logical < self.bucket_count || free_logical >= self.active_blocks_end
                );
                let from = self.blocks.block_ptr(destination);
                let to = self.blocks.block_ptr(free_physical);
                // SAFETY: both physical IDs are valid full blocks. Free block
                // receives live output which would otherwise be overwritten.
                // ptr::copy preserves uninitialized padding without reading it
                // as T; later element reads use only the live prefix.
                unsafe {
                    ptr::copy(from, to, self.blocks.block_size);
                }
                self.block_order[free_logical] = destination as u32;
                self.block_order[output] = free_physical as u32;
                block_positions[free_physical] = output as u32;
                block_positions[destination] = free_logical as u32;
                free_logical = output;
                output = block_positions[destination] as usize;
            }
            let length = self.next_block_len(input, &mut partial);
            let from = self.blocks.block_ptr(source);
            assert!(start + length <= self.blocks.input_len);
            // Completed partials can only shorten preceding output: start <=
            // (destination - scratch_blocks) * block_size, and
            // start + length never exceeds this destination block's end.
            // Any still-live destination block was evacuated above; earlier
            // physical destinations are already consumed.
            // SAFETY: length valid source elements are moved into the next
            // logical output interval. ptr::copy permits overlap as memmove.
            unsafe {
                ptr::copy(from, self.blocks.input.add(start), length);
            }
            self.block_order[input] = destination as u32;
            self.block_order[output] = source as u32;
            block_positions[source] = output as u32;
            block_positions[destination] = input as u32;
            start += length;
            visit_due(start);
            if input != output {
                free_physical = source;
                free_logical = output;
            }
        }
        for input in self.block_order.len() - self.bucket_count..self.active_blocks_end {
            let source = self.block_order[input] as usize;
            assert!(source < self.blocks.scratch_blocks);
            let length = self.next_block_len(input, &mut partial);
            let from = self.blocks.block_ptr(source);
            assert!(start + length <= self.blocks.input_len);
            // SAFETY: the remaining live prefix fits the checked output range;
            // ptr::copy permits overlap and preserves padding bytes.
            unsafe {
                ptr::copy(from, self.blocks.input.add(start), length);
            }
            start += length;
            visit_due(start);
        }
        visit_due(start);
        debug_assert!(visit.is_none() || remaining.is_empty());
        assert_eq!(start, self.blocks.input_len);
        self.input_materialized = true;
        self.next_block_order = block_positions;
        callback_panic
    }
}

impl<T> Drop for Partition<'_, '_, T> {
    fn drop(&mut self) {
        // At pass boundaries the directory is a complete owning sequence.
        // Restore it even if unwinding starts outside key extraction (for
        // example an allocation failure with an unwinding error handler).
        if !self.input_materialized {
            self.compact();
        }
        self.buffers.scratch = core::mem::take(&mut self.blocks.scratch);
        self.buffers.block_order = core::mem::take(&mut self.block_order);
        self.buffers.next_block_order = core::mem::take(&mut self.next_block_order);
        self.buffers.bucket_ids = core::mem::take(&mut self.bucket_ids);
        self.buffers.partial_blocks = core::mem::take(&mut self.partial_blocks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_root_rounds_up() {
        // With 5-byte records and one radix bit, the ratio is exactly len / 2.
        let table = [
            (0, 0),
            (1, 1),
            (2, 1),
            (3, 2),
            (7, 2),
            (8, 2),
            (9, 3),
            (199, 10),
            (200, 10),
            (201, 11),
        ];
        for (len, expected) in table {
            assert_eq!(balanced_block_size_integer(len, 5, 1), expected);
        }
        // A remainder must round up even when the integer quotient is a square.
        assert_eq!(balanced_block_size_integer(20, 50, 1), 1);
        assert_eq!(balanced_block_size_integer(21, 50, 1), 2);
    }

    /// Records carry a unique id so a permutation check also proves that no
    /// record was lost or duplicated.
    #[cfg(feature = "std")]
    fn numbered(len: usize) -> Vec<(u32, u32)> {
        (0..len as u32)
            .map(|id| (id.wrapping_mul(0x9e37_79b9), id))
            .collect()
    }

    #[cfg(feature = "std")]
    fn ids(records: &[(u32, u32)]) -> Vec<u32> {
        let mut ids: Vec<u32> = records.iter().map(|record| record.1).collect();
        ids.sort_unstable();
        ids
    }

    /// Scatters until the key function panics at call `at`, then checks the
    /// recovered pass: the key is not called again, the unread records land in
    /// bucket 0, and compaction restores the input slice.
    #[cfg(feature = "std")]
    fn scatter_until_panic(len: usize, block_size: usize, at: usize) {
        let mut records = numbered(len);
        // A pass reads records in input order. Records before the panicking
        // call were classified; every later record goes to bucket 0, so the
        // exact expected ends follow from the input alone.
        let mut counts = [0usize; 4];
        for (index, record) in records.iter().enumerate() {
            let bucket = if index + 1 < at {
                (record.0 & 3) as usize
            } else {
                0
            };
            counts[bucket] += 1;
        }
        let mut total = 0;
        let expected: Vec<usize> = counts
            .iter()
            .map(|count| {
                total += count;
                total
            })
            .collect();
        let mut buffers = Buffers::new();
        let mut calls = 0;
        let panic = {
            let mut sorter = Partition::new(&mut records, 4, block_size, &mut buffers);
            let (panic, ends) = sorter.step_with_ends(
                |record: &(u32, u32)| {
                    calls += 1;
                    if calls == at {
                        std::panic::panic_any(0x71u32);
                    }
                    (record.0 & 3) as usize
                },
                true,
            );
            assert!(panic.is_some(), "panic at {at} of {len} was not caught");
            // The recovery re-scatters unread records without the failed
            // extractor, which must not run a second time.
            assert_eq!(calls, at, "key called after it panicked");
            assert_eq!(ends, expected, "recovered ends at panic {at}");
            assert!(sorter.compact_with(&ends, None).is_none());
            // Drop also compacts, so the check must run before the sorter does.
            assert!(sorter.input_materialized, "compaction did not finish");
            panic
        };
        assert_eq!(*panic.unwrap().downcast::<u32>().unwrap(), 0x71);
        assert_eq!(ids(&records), (0..len as u32).collect::<Vec<_>>());
    }

    /// `(len, block_size)` pairs. The short pair has a one-record tail over
    /// tiny blocks, which Miri can afford; the floors in `default_block_size`
    /// make a production block longer than the whole short input, so the short
    /// case sets one. The long pairs are a production block with and without a
    /// tail, and a longer input where the root formula picks the block size.
    #[cfg(feature = "std")]
    fn configs() -> Vec<(usize, usize)> {
        let record_bytes = core::mem::size_of::<(u32, u32)>();
        let mut configs = vec![(29, 4), (32, 4)];
        if cfg!(miri) {
            return configs;
        }
        let long = [
            (4093, default_block_size(4093, record_bytes, 2)),
            (4096, default_block_size(4096, record_bytes, 2)),
            (8193, default_block_size(8193, record_bytes, 2)),
        ];
        // The recovery offset depends on consumed input blocks, so every long
        // input must stay many blocks wide if the block size constant changes.
        for &(len, block_size) in &long {
            assert!(
                len / block_size >= 8,
                "{block_size}-record blocks leave too few full blocks at {len}"
            );
        }
        // Above the floors the formula decides, and an odd block size keeps
        // the case from repeating the power-of-two ones.
        assert!(!long[2].1.is_power_of_two(), "the root formula must decide");
        configs.extend(long);
        configs
    }

    /// A scatter panic can leave the pass read up to any record. The recovery
    /// reconstructs the read offset from the bucket cursors and consumed input
    /// blocks, so every offset near a block boundary is checked.
    #[cfg(feature = "std")]
    #[test]
    fn scatter_panic_keeps_records() {
        for (len, block_size) in configs() {
            // About sixty points across the pass, plus the block edges below.
            let mut points: Vec<usize> = (1..=len).step_by((len / 61).max(1)).collect();
            // Block boundaries decide how much of the pass is still unread.
            for at in [
                1,
                block_size - 1,
                block_size,
                block_size + 1,
                2 * block_size,
                len - 1,
                len,
            ] {
                if (1..=len).contains(&at) {
                    points.push(at);
                }
            }
            points.sort_unstable();
            points.dedup();
            for at in points {
                scatter_until_panic(len, block_size, at);
            }
        }
    }

    /// A callback panic must not interrupt materialization: the remaining
    /// buckets are still copied back, and the payload is returned afterwards.
    #[cfg(feature = "std")]
    #[test]
    fn callback_panic_keeps_records() {
        for (len, block_size) in configs() {
            // Every bucket receives records, so each of the four callbacks exists.
            for at in 1..=4 {
                let mut records = numbered(len);
                let mut buffers = Buffers::new();
                let mut visited = 0;
                let payload = {
                    let mut sorter = Partition::new(&mut records, 4, block_size, &mut buffers);
                    let (panic, ends) =
                        sorter.step_with_ends(|record: &(u32, u32)| (record.0 & 3) as usize, true);
                    assert!(panic.is_none());
                    let mut callback = |bucket: &mut [(u32, u32)]| {
                        // Buckets materialize in order, so the slice must hold
                        // exactly the records of the bucket that just landed.
                        assert!(
                            bucket
                                .iter()
                                .all(|record| (record.0 & 3) as usize == visited)
                        );
                        visited += 1;
                        if visited == at {
                            std::panic::panic_any(0x71u32);
                        }
                    };
                    let payload = sorter.compact_with(&ends, Some(&mut callback));
                    // Drop also compacts, so the check must run before it does.
                    assert!(sorter.input_materialized, "compaction did not finish");
                    payload
                };
                assert_eq!(
                    *payload
                        .unwrap_or_else(|| panic!("callback {at} was not called"))
                        .downcast::<u32>()
                        .unwrap(),
                    0x71
                );
                assert_eq!(visited, at, "callbacks continued after the panic");
                assert_eq!(ids(&records), (0..len as u32).collect::<Vec<_>>());
            }
        }
    }

    /// The integer form must reproduce the floating-point block size: a
    /// different block size moves pass and tile boundaries, and so would leave
    /// the measurements behind the constant.
    #[cfg(feature = "std")]
    #[test]
    fn balanced_forms_agree() {
        let lengths = (1..=600).chain([1 << 10, 4096, 10_000, 1 << 16, 1 << 20, 1 << 24]);
        for len in lengths {
            for record_bytes in [1, 2, 4, 8, 16, 24, 32, 64, 128] {
                for radix in 1..=MAX_RADIX_BITS {
                    assert_eq!(
                        balanced_block_size(len, record_bytes, radix),
                        balanced_block_size_integer(len, record_bytes, radix),
                        "len {len}, {record_bytes}-byte records, {radix} bits"
                    );
                }
            }
        }
    }
}
