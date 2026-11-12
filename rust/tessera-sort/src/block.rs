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

//! Stable block partition adapted from Robert Clausecker's
//! BSD-2-Clause C implementation, `radixsort_permuted.c`.
//! Reference revision: f69e816c3cd79d312cd67aea5b9cf1c338c1b371.
//! <https://github.com/clausecker/radsort/blob/f69e816c3cd79d312cd67aea5b9cf1c338c1b371/radixsort_permuted.c>
//! <https://arxiv.org/abs/2607.05302>
//!
//! The engine uses one pass of this sort as a stable partition into the
//! buckets a [`Classify`] assigns ([`partition`]).
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

use crate::unwind;
use alloc::vec::Vec;
use core::{marker::PhantomData, mem::MaybeUninit, ptr};

/// Widest digit, in bits, that one pass accepts. Callers that choose their own
/// digit width (the Tessera engine) bound it by this constant.
pub(crate) const MAX_RADIX_BITS: usize = 12;

#[cfg(feature = "rayon")]
mod parallel;
#[cfg(feature = "rayon")]
pub(crate) use parallel::partition_parallel;

/// Original input-block cursor and count of completed output blocks.
struct ScatterPosition {
    input: usize,
    output: usize,
}

/// Bytes of block tables the partition keeps per physical block.
///
/// Scatter links full blocks into bucket chains. After those chains have been
/// laid out in logical order, the link allocation becomes compaction's inverse
/// map. Only these two `u32` arrays coexist; no per-block bucket ID is needed.
const DESCRIPTOR_BYTES_PER_POSITION: usize = 2 * core::mem::size_of::<u32>();

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
/// 2 R b s + 8 n / b
/// ```
///
/// bytes of auxiliary memory, and the two terms are equal at
/// `b = sqrt(8 n / (2 R s))`. That root minimizes the sum: writing `b = c b*`
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
    scaled_block_size(
        balanced_block_size(len, record_bytes, radix),
        record_bytes,
        BLOCK_SCALE,
    )
}

/// Applies a balanced-size multiple and the per-block amortization floors.
/// Serial and parallel partitions share these floors with their own balance.
fn scaled_block_size(balanced: usize, record_bytes: usize, scale: usize) -> usize {
    let floors = (BLOCK_BYTES_FLOOR / record_bytes).max(BLOCK_RECORDS_FLOOR);
    balanced.saturating_mul(scale).max(floors).max(1)
}

/// The balanced block size, rounded up.
///
/// Evaluates `ceil(sqrt(8 n / (2 R s)))`; tests compare against an integer solve.
fn balanced_block_size(len: usize, record_bytes: usize, radix: usize) -> usize {
    balanced_size(len, record_bytes, 1 << radix, DESCRIPTOR_BYTES_PER_POSITION)
}

/// Solves `ceil(sqrt(d n / (2 B s)))` for `B` scratch buckets and `d` directory
/// bytes per block position: the size at which both memory terms are equal.
fn balanced_size(len: usize, record_bytes: usize, buckets: usize, directory_bytes: usize) -> usize {
    let tables = directory_bytes as f64 * len as f64;
    let scratch_per_record = 2.0 * buckets as f64 * record_bytes as f64;
    (tables / scratch_per_record).sqrt().ceil() as usize
}

/// Integer reference for checking the floating-point block size.
#[cfg(test)]
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

/// Transfers a retired bucket borrow; later compaction never accesses it again.
/// Receives each bucket's index and records.
type Visit<'callback, 'records, T> = &'callback mut dyn FnMut(usize, &'records mut [T]);

/// A [`Visit`] that also receives the classifier, which classification has
/// finished borrowing by the time buckets are visited.
type ClassifierVisit<'callback, 'records, T, C> =
    &'callback mut dyn FnMut(usize, &'records mut [T], &mut C);

/// Assigns each record its bucket in a stable partition.
///
/// A partition into `2^radix` buckets uses only the low `radix` bits of the
/// result. Classification may panic; the partition then restores every record.
pub(crate) trait Classify<T> {
    /// Returns the bucket of `record`.
    fn bucket(&mut self, record: &T) -> usize;
}

/// Lets tests classify with a closure.
#[cfg(test)]
impl<T, F: FnMut(&T) -> usize> Classify<T> for F {
    fn bucket(&mut self, record: &T) -> usize {
        self(record)
    }
}

/// Partitions `records` stably into the `2^radix` buckets `classifier`
/// assigns and returns each bucket's cumulative end in records.
///
/// `visit` is called on each bucket as soon as its records are contiguous in
/// `records`, while they are still in cache.
///
/// Returns an empty vector for inputs of fewer than two records or zero-sized
/// records.
///
/// # Panics
///
/// Panics if `radix` is not in `1..=MAX_RADIX_BITS`. A classification or
/// `visit` panic is resumed after every record is back in `records`.
pub(crate) fn partition<'records, T, C: Classify<T>>(
    records: &'records mut [T],
    radix: usize,
    classifier: C,
    buffers: &mut Buffers<T>,
    visit: ClassifierVisit<'_, 'records, T, C>,
) -> Vec<usize> {
    let record_bytes = core::mem::size_of::<T>();
    if records.len() < 2 || record_bytes == 0 {
        return Vec::new();
    }
    assert!((1..=MAX_RADIX_BITS).contains(&radix));
    let block_size = default_block_size(records.len(), record_bytes, radix);
    let mut sorter = Partition::new(records, 1 << radix, block_size, buffers);
    let mut classifier = match sorter.step(classifier) {
        Ok(classifier) => classifier,
        Err(payload) => {
            sorter.compact();
            unwind::resume(payload);
        }
    };
    let ends = sorter.record_ends();
    // Classification has returned the classifier before any tile callback
    // starts; both phases use the same FnMut state in sequence.
    let mut callback =
        |bucket: usize, child: &'records mut [T]| visit(bucket, child, &mut classifier);
    if let Some(payload) = sorter.compact_with(&ends, Some(&mut callback)) {
        unwind::resume(payload);
    }
    ends
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
    /// The last physical block in this bucket's chain.
    physical: u32,
}

// SAFETY: cursors are used only by the partition that exclusively owns their
// input and scratch. Between partitions they are stale and are rewritten before
// access; transferring the buffers never transfers a shared record borrow.
unsafe impl<T: Send> Send for BucketWriter<T> {}

/// Physical block storage: the input slice and one scratch allocation.
///
/// Physical block IDs number the scratch blocks first, then the full input
/// blocks. Unlike the C reference's integer pairs, `T` may have no valid all-zero value, so scratch
/// stays uninitialized and only recorded live prefixes are read.
/// `input_borrow` keeps the input exclusively borrowed while raw pointers
/// address it.
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
    /// Creates storage, reusing the retained scratch allocation.
    fn new(
        input: &'a mut [T],
        bucket_count: usize,
        block_size: usize,
        mut scratch: Vec<MaybeUninit<T>>,
    ) -> Self {
        assert!(bucket_count.is_power_of_two());
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
        // SAFETY: the bounds check selects a full block in the input or in
        // scratch, which holds `scratch_blocks` blocks. The allocations stay
        // alive and are never resized during sorting.
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

/// Allocations reused by successive node partitions within one sort.
///
/// Each pass resizes and rewrites what it uses.
pub(crate) struct Buffers<T> {
    /// Scratch records, reused by each pass.
    scratch: Vec<MaybeUninit<T>>,
    /// Logical-to-physical block directory of the active partition.
    block_order: Vec<u32>,
    /// Physical next links during scatter, then the inverse compaction map.
    block_links: Vec<u32>,
    /// Shorter live prefixes, one per bucket, in directory order.
    partial_blocks: Vec<PartialBlock>,
    writers: Vec<BucketWriter<T>>,
    #[cfg(debug_assertions)]
    seen: Vec<bool>,
}

impl<T> Buffers<T> {
    pub(crate) fn new() -> Self {
        Self {
            scratch: Vec::new(),
            block_order: Vec::new(),
            block_links: Vec::new(),
            partial_blocks: Vec::new(),
            writers: Vec::new(),
            #[cfg(debug_assertions)]
            seen: Vec::new(),
        }
    }
}

impl<T> Default for Buffers<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// The state of one block partition over an input slice.
///
/// Scatter reads the original contiguous input and links each bucket's output
/// blocks. Once it has read every record, the directory in [`Buffers`] orders
/// those chains for compaction. `active_blocks` holds the live block sequence;
/// its prefix and suffix contain free blocks.
struct Partition<'a, 'w, T> {
    buffers: &'w mut Buffers<T>,
    blocks: BlockStorage<'a, T>,
    active_blocks: core::ops::Range<usize>,
    /// This guard restores moved records unless completion takes ownership.
    owns_recovery: bool,
}

impl<'a, 'w, T> Partition<'a, 'w, T> {
    fn new(
        input: &'a mut [T],
        bucket_count: usize,
        block_size: usize,
        buffers: &'w mut Buffers<T>,
    ) -> Self {
        let blocks = BlockStorage::new(
            input,
            bucket_count,
            block_size,
            core::mem::take(&mut buffers.scratch),
        );
        // Allocate every directory before scatter moves the first record.
        // The original input, including its tail, stays contiguous until read.
        buffers.block_order.clear();
        buffers.block_order.reserve(blocks.block_count);
        buffers.block_links.resize(blocks.block_count, 0);
        buffers.partial_blocks.clear();
        buffers.partial_blocks.resize(
            bucket_count,
            PartialBlock {
                order_index: blocks.block_count,
                len: 0,
            },
        );
        Self {
            buffers,
            blocks,
            active_blocks: bucket_count..bucket_count,
            owns_recovery: false,
        }
    }

    fn next_block_len(&self, logical: usize, partial: &mut usize) -> usize {
        if *partial < self.buffers.partial_blocks.len()
            && self.buffers.partial_blocks[*partial].order_index == logical
        {
            let length = self.buffers.partial_blocks[*partial].len;
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
        seen.resize(self.buffers.block_order.len(), false);
        for &physical in &self.buffers.block_order {
            assert!(!seen[physical as usize]);
            seen[physical as usize] = true;
        }
        assert!(self.active_blocks.end <= self.buffers.block_order.len());
        assert!(
            self.buffers
                .partial_blocks
                .windows(2)
                .all(|p| p[0].order_index < p[1].order_index)
        );
        assert!(
            self.buffers
                .partial_blocks
                .iter()
                .all(|p| self.active_blocks.contains(&p.order_index)
                    && p.len < self.blocks.block_size)
        );
        let mut partial = 0;
        let total: usize = self
            .active_blocks
            .clone()
            .map(|i| self.next_block_len(i, &mut partial))
            .sum();
        assert_eq!(total, self.blocks.input_len);
    }

    /// Classifies the original contiguous input once and returns the
    /// classifier, or a panic after appending the unread suffix to bucket zero.
    fn step<C: Classify<T>>(&mut self, classifier: C) -> Result<C, unwind::Payload> {
        debug_assert!(self.buffers.block_order.is_empty());
        let mut buckets = self.begin_pass();
        self.owns_recovery = true;
        let mut position = ScatterPosition {
            input: 0,
            output: 0,
        };
        // Owning the classifier keeps its constants in the scatter's register
        // allocation instead of accessing them through a borrowed extractor.
        let result =
            unwind::catch(|| self.scatter(&mut buckets, &mut position, 0, classifier, C::bucket));
        if result.is_err() {
            let written = position.output * self.blocks.block_size
                + buckets
                    .iter()
                    .map(|bucket| {
                        // SAFETY: both cursors delimit the same reserved block.
                        let free = unsafe { bucket.end.offset_from(bucket.next) as usize };
                        self.blocks.block_size - free
                    })
                    .sum::<usize>();
            let offset = written - position.input * self.blocks.block_size;
            // Input consumption is contiguous. Complete its unread suffix
            // without calling the failed classifier or moving any live record
            // twice; the same read-before-reserve rule funds the new blocks.
            self.scatter(&mut buckets, &mut position, offset, (), |_, _| 0);
        }
        self.finish_pass(&buckets, position.output);
        self.buffers.writers = buckets;
        #[cfg(debug_assertions)]
        self.validate();
        result
    }

    /// Logical blocks of `bucket` after a single-shard pass. Only the last
    /// one is partial: a writer takes a new block as soon as it fills one.
    fn bucket_blocks(&self, bucket: usize) -> core::ops::Range<usize> {
        let start = match bucket.checked_sub(1) {
            Some(previous) => self.buffers.partial_blocks[previous].order_index + 1,
            None => self.active_blocks.start,
        };
        start..self.buffers.partial_blocks[bucket].order_index + 1
    }

    /// Each bucket's cumulative end in records after a single-shard pass.
    ///
    /// Record boundaries are not block boundaries. The directory already names
    /// every bucket's blocks and partial length, so this is one pass over the
    /// bucket table, not over the records.
    fn record_ends(&mut self) -> Vec<usize> {
        let mut ends = Vec::with_capacity(self.buffers.partial_blocks.len());
        let mut records = 0;
        for (bucket, partial) in self.buffers.partial_blocks.iter().enumerate() {
            let full_blocks = self.bucket_blocks(bucket).len() - 1;
            records += full_blocks * self.blocks.block_size + partial.len;
            ends.push(records);
        }
        ends
    }

    /// Starts each bucket's chain in its own scratch block.
    fn begin_pass(&mut self) -> Vec<BucketWriter<T>> {
        let mut buckets = core::mem::take(&mut self.buffers.writers);
        buckets.clear();
        buckets.reserve(self.active_blocks.start);
        for physical in 0..self.active_blocks.start {
            let out = self.blocks.block_ptr(physical);
            buckets.push(BucketWriter {
                next: out,
                // SAFETY: block_ptr returns a complete block.
                end: unsafe { out.add(self.blocks.block_size) },
                physical: physical as u32,
            });
        }
        buckets
    }

    /// Linearizes the bucket chains without histogramming or reordering an
    /// input directory. The unused scratch blocks form the free prefix.
    fn finish_pass(&mut self, buckets: &[BucketWriter<T>], recycled: usize) {
        let order = &mut self.buffers.block_order;
        order.extend((buckets.len()..self.blocks.scratch_blocks).map(|block| block as u32));
        for (bucket, writer) in buckets.iter().enumerate() {
            let mut physical = bucket as u32;
            while physical != writer.physical {
                order.push(physical);
                physical = self.buffers.block_links[physical as usize];
            }
            self.buffers.partial_blocks[bucket] = PartialBlock {
                order_index: order.len(),
                // SAFETY: both cursors delimit this bucket's last block.
                len: self.blocks.block_size
                    - unsafe { writer.end.offset_from(writer.next) as usize },
            };
            order.push(physical);
        }
        self.active_blocks.end = order.len();
        order.extend(
            (self.blocks.scratch_blocks + recycled..self.blocks.block_count)
                .map(|block| block as u32),
        );
        debug_assert_eq!(order.len(), self.blocks.block_count);
    }

    // Pass setup and block reordering need state that scatter does not.
    // Keep their optimization regions separate so that state does not force
    // digit masks and bucket cursors out of registers in the hot loop.
    #[inline(never)]
    fn scatter<C, F: FnMut(&mut C, &T) -> usize>(
        &mut self,
        buckets: &mut [BucketWriter<T>],
        position: &mut ScatterPosition,
        first_offset: usize,
        mut state: C,
        mut classify: F,
    ) -> C {
        debug_assert!(buckets.len().is_power_of_two());
        let last_bucket = buckets.len() - 1;
        let mut begin = first_offset;
        while position.input * self.blocks.block_size < self.blocks.input_len {
            let input = position.input;
            let begin_record = input * self.blocks.block_size;
            // SAFETY: this block's unread records still occupy their original
            // input range. Only fully read input blocks can become outputs.
            let source = unsafe { self.blocks.input.add(begin_record) };
            let length = self
                .blocks
                .block_size
                .min(self.blocks.input_len - begin_record);
            let mut write = |offset: usize| {
                // SAFETY: only the initialized, unread suffix is read. Blocks
                // become output only after all of their input records are read.
                // Borrow the actual live record. Never materialize a second T:
                // it may own a String, Box, Rc, or other non-Copy payload.
                // This borrow ends before the bitwise move below.
                let b = classify(&mut state, unsafe { &*source.add(offset) }) & last_bucket;
                // SAFETY: there is one writer per bucket and the bucket count
                // is a power of two, so the mask keeps the index in bounds;
                // the check would otherwise run once per record in the hot loop.
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
                    let physical = self.blocks.scratch_blocks + position.output;
                    let out = self.blocks.block_ptr(physical);
                    self.buffers.block_links[bucket.physical as usize] = physical as u32;
                    *bucket = BucketWriter {
                        next: out,
                        // SAFETY: block_ptr reserves a complete block.
                        end: unsafe { out.add(self.blocks.block_size) },
                        physical: physical as u32,
                    };
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
        state
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
        mut visit: Option<Visit<'_, 'a, T>>,
    ) -> Option<unwind::Payload> {
        // Sorting is finished. Reuse the output buffer as the inverse map:
        // physical block ID -> position in block_order.
        let mut block_positions = core::mem::take(&mut self.buffers.block_links);
        for i in 0..self.buffers.block_order.len() {
            block_positions[self.buffers.block_order[i] as usize] = i as u32;
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
                    let index = ends.len() - remaining.len();
                    // SAFETY: this bucket is fully materialized and no longer a
                    // copy source; its input slice does not overlap scratch.
                    let bucket = unsafe {
                        core::slice::from_raw_parts_mut(input_ptr.add(begin), end - begin)
                    };
                    if let Err(payload) = unwind::catch(|| callback(index, bucket)) {
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
        let mut free_physical = self.buffers.block_order[free_logical] as usize;
        for destination in self.blocks.scratch_blocks..self.buffers.block_order.len() {
            let input = destination - self.active_blocks.start;
            let mut output = block_positions[destination] as usize;
            let source = self.buffers.block_order[input] as usize;
            if output > input && output < self.active_blocks.end {
                debug_assert!(!self.active_blocks.contains(&free_logical));
                let from = self.blocks.block_ptr(destination);
                let to = self.blocks.block_ptr(free_physical);
                // SAFETY: both physical IDs are valid full blocks. Free block
                // receives live output which would otherwise be overwritten.
                // ptr::copy preserves uninitialized padding without reading it
                // as T; later element reads use only the live prefix.
                unsafe {
                    ptr::copy(from, to, self.blocks.block_size);
                }
                self.buffers.block_order[free_logical] = destination as u32;
                self.buffers.block_order[output] = free_physical as u32;
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
            self.buffers.block_order[input] = destination as u32;
            self.buffers.block_order[output] = source as u32;
            block_positions[source] = output as u32;
            block_positions[destination] = input as u32;
            start += length;
            visit_due(start);
            if input != output {
                free_physical = source;
                free_logical = output;
            }
        }
        for input in
            self.buffers.block_order.len() - self.active_blocks.start..self.active_blocks.end
        {
            let source = self.buffers.block_order[input] as usize;
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
        self.owns_recovery = false;
        self.buffers.block_links = block_positions;
        callback_panic
    }
}

impl<T> Drop for Partition<'_, '_, T> {
    fn drop(&mut self) {
        // After scatter the directory is a complete owning sequence.
        // Restore it even if unwinding starts outside key extraction (for
        // example an allocation failure with an unwinding error handler).
        if self.owns_recovery {
            self.compact();
        }
        self.buffers.scratch = core::mem::take(&mut self.blocks.scratch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_root_rounds_up() {
        // With 4-byte records and one radix bit, the ratio is exactly len / 2.
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
            assert_eq!(balanced_block_size_integer(len, 4, 1), expected);
        }
        // A remainder must round up even when the integer quotient is a square.
        assert_eq!(balanced_block_size_integer(25, 50, 1), 1);
        assert_eq!(balanced_block_size_integer(26, 50, 1), 2);
    }

    /// Records carry a unique id so a permutation check also proves that no
    /// record was lost or duplicated.
    fn numbered(len: usize) -> Vec<(u32, u32)> {
        (0..len as u32)
            .map(|id| (id.wrapping_mul(0x9e37_79b9), id))
            .collect()
    }

    fn ids(records: &[(u32, u32)]) -> Vec<u32> {
        let mut ids: Vec<u32> = records.iter().map(|record| record.1).collect();
        ids.sort_unstable();
        ids
    }

    /// Scatters until the key function panics at call `at`, then checks the
    /// recovered pass: the key is not called again, the unread records land in
    /// bucket 0, and compaction restores the input slice.
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
            let panic = sorter
                .step(|record: &(u32, u32)| {
                    calls += 1;
                    if calls == at {
                        std::panic::panic_any(0x71u32);
                    }
                    (record.0 & 3) as usize
                })
                .err();
            let ends = sorter.record_ends();
            assert!(panic.is_some(), "panic at {at} of {len} was not caught");
            // The recovery re-scatters unread records without the failed
            // extractor, which must not run a second time.
            assert_eq!(calls, at, "key called after it panicked");
            assert_eq!(ends, expected, "recovered ends at panic {at}");
            assert!(sorter.compact_with(&ends, None).is_none());
            // Drop also compacts, so the check must run before the sorter does.
            assert!(!sorter.owns_recovery, "compaction did not finish");
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
                    let panic = sorter.step(|record: &(u32, u32)| (record.0 & 3) as usize);
                    assert!(panic.is_ok());
                    let ends = sorter.record_ends();
                    let mut callback = |_: usize, bucket: &mut [(u32, u32)]| {
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
                    assert!(!sorter.owns_recovery, "compaction did not finish");
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
