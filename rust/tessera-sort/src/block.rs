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
//! output blocks, and per-bucket links record the logical sequence, so a pass
//! needs O(√n) scratch instead of a second buffer of the input's size.
//!
//! Ownership: the active block sequence holds exactly one live copy of every
//! record. Other physical copies are stale bytes that are never dropped or
//! borrowed, and key extraction borrows only live records. Scratch is
//! `MaybeUninit<T>`. Before returning or propagating a key-function panic,
//! the completion ledger moves every live record back into the input, so records
//! need neither `Copy` nor `Clone`.

use crate::trace::{self, Event};
#[cfg(test)]
use crate::unwind;
use alloc::vec::Vec;
use core::{marker::PhantomData, mem::MaybeUninit, ptr};

/// Widest digit, in bits, that one pass accepts. Callers that choose their own
/// digit width (the Tessera engine) bound it by this constant.
pub(crate) const MAX_RADIX_BITS: usize = 12;

mod finish;
#[cfg(feature = "rayon")]
mod parallel;
#[cfg(feature = "rayon")]
pub(crate) use parallel::partition_parallel;

/// Original input-block cursor and count of completed output blocks.
struct ScatterPosition {
    input: usize,
    output: usize,
}

/// Smallest block, counted in bytes of records.
///
/// Completion transfers a block with one copy, so a block has to hold
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
/// The bookkeeping of a block — its source location, ownership and one copy
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
/// [`finish::serial_descriptor_bytes`] bytes per position; records move by
/// exchanging blocks, not by copying into a second buffer. For `n` records of
/// `s` bytes, a block size `b` therefore costs
///
/// ```text
/// 2 R b s + d n / b
/// ```
///
/// bytes of auxiliary memory, and the two terms are equal at
/// `b = sqrt(d n / (2 R s))`. That root minimizes the sum: writing `b = c b*`
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
/// Evaluates `ceil(sqrt(d n / (2 R s)))`; tests compare against an integer solve.
fn balanced_block_size(len: usize, record_bytes: usize, radix: usize) -> usize {
    balanced_size(
        len,
        record_bytes,
        1 << radix,
        finish::serial_descriptor_bytes(),
    )
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
    let numerator = finish::serial_descriptor_bytes() as u128 * len as u128;
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
/// `visit` panic propagates after every record is back in `records`.
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
    partition_in_blocks(records, radix, block_size, classifier, buffers, visit)
}

/// [`partition`] with an explicit block size of `block_size` records.
///
/// Requires at least two non-zero-sized records and a valid `radix`.
pub(crate) fn partition_in_blocks<'records, T, C: Classify<T>>(
    records: &'records mut [T],
    radix: usize,
    block_size: usize,
    classifier: C,
    buffers: &mut Buffers<T>,
    visit: ClassifierVisit<'_, 'records, T, C>,
) -> Vec<usize> {
    assert!((1..=MAX_RADIX_BITS).contains(&radix));
    let mut sorter = Partition::new(records, 1 << radix, block_size, buffers);
    let mut classifier = sorter.step(classifier);
    // The ledger lends only retired final bands; both classification and
    // child finishing borrow this same mutable classifier in sequence.
    let mut callback =
        |bucket: usize, child: &'records mut [T]| visit(bucket, child, &mut classifier);
    finish::complete_serial(&mut sorter, Some(&mut callback))
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

/// Allocations reused by successive node partitions within one sort.
///
/// Each pass resizes and rewrites what it uses.
pub(crate) struct Buffers<T> {
    /// Scratch records, reused by each pass.
    scratch: Vec<MaybeUninit<T>>,
    /// Physical next links in each bucket's stable output chain.
    block_links: Vec<u32>,
    writers: Vec<BucketWriter<T>>,
    #[cfg(debug_assertions)]
    seen: Vec<bool>,
}

impl<T> Buffers<T> {
    pub(crate) fn new() -> Self {
        Self {
            scratch: Vec::new(),
            block_links: Vec::new(),
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
/// Scatter reads contiguous input and links each bucket's output blocks.
/// This guard restores those chains unless the completion ledger takes over.
/// Scratch remains in the borrowed buffers, without resizing while records are
/// live. The input borrow marker protects the slice addressed by raw pointers.
struct Partition<'a, 'w, T> {
    buffers: &'w mut Buffers<T>,
    input: *mut T,
    input_len: usize,
    block_size: usize,
    scratch_blocks: usize,
    block_count: usize,
    input_borrow: PhantomData<&'a mut [T]>,
    bucket_count: usize,
    position: ScatterPosition,
    owns_recovery: bool,
}

// SAFETY: the guard exclusively owns its input and buffers. Raw cursors address
// only those allocations; moving the guard shares no record borrow or scratch.
unsafe impl<T: Send> Send for Partition<'_, '_, T> {}

impl<'a, 'w, T> Partition<'a, 'w, T> {
    fn new(
        input: &'a mut [T],
        bucket_count: usize,
        block_size: usize,
        buffers: &'w mut Buffers<T>,
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
        buffers
            .scratch
            .resize_with(scratch_len, MaybeUninit::<T>::uninit);
        buffers.block_links.resize(block_count, 0);
        trace::emit(|| Event::Begin {
            len: input.len(),
            block_size,
            buckets: bucket_count,
            scratch_blocks,
        });
        Self {
            buffers,
            input: input.as_mut_ptr(),
            input_len: input.len(),
            block_size,
            scratch_blocks,
            block_count,
            input_borrow: PhantomData,
            bucket_count,
            position: ScatterPosition {
                input: 0,
                output: 0,
            },
            owns_recovery: false,
        }
    }

    /// Addresses a physical block: scratch first, then full input blocks.
    fn block_ptr(&mut self, physical: usize) -> *mut T {
        assert!(physical < self.block_count);
        // SAFETY: the bounds check selects a full block in the input or in
        // scratch, which holds `scratch_blocks` blocks. The allocations stay
        // alive and are never resized during sorting.
        unsafe {
            if physical < self.scratch_blocks {
                self.buffers
                    .scratch
                    .as_mut_ptr()
                    .cast::<T>()
                    .add(physical * self.block_size)
            } else {
                self.input
                    .add((physical - self.scratch_blocks) * self.block_size)
            }
        }
    }

    #[cfg(debug_assertions)]
    fn validate(&mut self) {
        let mut seen = core::mem::take(&mut self.buffers.seen);
        seen.clear();
        seen.resize(self.block_count, false);
        let mut total = 0;
        for bucket in 0..self.bucket_count {
            for (physical, len) in self.bucket_blocks(bucket) {
                assert!(!seen[physical]);
                seen[physical] = true;
                assert!(len <= self.block_size);
                total += len;
            }
        }
        assert_eq!(total, self.input_len);
        self.buffers.seen = seen;
    }

    /// Classifies contiguous input once, retaining cursors for unwind recovery.
    fn step<C: Classify<T>>(&mut self, classifier: C) -> C {
        assert!(!self.owns_recovery, "partition already scattered");
        self.begin_pass();
        self.position = ScatterPosition {
            input: 0,
            output: 0,
        };
        // Every table and initial writer exists before any record can move.
        self.owns_recovery = true;
        // Owning the classifier keeps its constants in the scatter's register
        // allocation instead of accessing them through a borrowed extractor.
        let classifier = self.scatter(0, classifier, C::bucket);
        #[cfg(debug_assertions)]
        self.validate();
        classifier
    }

    /// Physical prefixes of one stable bucket chain. The writer records the
    /// tail, so no uninitialized next link is read after its partial prefix.
    fn bucket_blocks(&self, bucket: usize) -> impl Iterator<Item = (usize, usize)> + '_ {
        let writer = &self.buffers.writers[bucket];
        let last = writer.physical as usize;
        // SAFETY: both cursors delimit the same reserved non-zero-sized block.
        let tail_len = self.block_size - unsafe { writer.end.offset_from(writer.next) as usize };
        core::iter::successors(Some(bucket), move |&physical| {
            (physical != last).then(|| self.buffers.block_links[physical] as usize)
        })
        .map(move |physical| {
            (
                physical,
                if physical == last {
                    tail_len
                } else {
                    self.block_size
                },
            )
        })
    }

    /// Starts each bucket's chain in its own scratch block.
    fn begin_pass(&mut self) {
        self.buffers.writers.clear();
        self.buffers.writers.reserve(self.bucket_count);
        for physical in 0..self.bucket_count {
            let out = self.block_ptr(physical);
            self.buffers.writers.push(BucketWriter {
                next: out,
                // SAFETY: block_ptr returns a complete block.
                end: unsafe { out.add(self.block_size) },
                physical: physical as u32,
            });
            trace::emit(|| Event::Writer {
                bucket: physical,
                physical,
            });
        }
    }

    // Pass setup and completion need state that scatter does not.
    // Keep their optimization regions separate so that state does not force
    // digit masks and bucket cursors out of registers in the hot loop.
    #[inline(never)]
    fn scatter<C, F: FnMut(&mut C, &T) -> usize>(
        &mut self,
        first_offset: usize,
        mut state: C,
        mut classify: F,
    ) -> C {
        let input_ptr = self.input;
        let input_len = self.input_len;
        let block_size = self.block_size;
        let scratch_blocks = self.scratch_blocks;
        let full_blocks = self.block_count - scratch_blocks;
        let position = &mut self.position;
        let buckets = self.buffers.writers.as_mut_slice();
        let block_links = self.buffers.block_links.as_mut_slice();
        debug_assert!(buckets.len().is_power_of_two());
        let last_bucket = buckets.len() - 1;
        let mut begin = first_offset;
        while position.input * block_size < input_len {
            let input = position.input;
            let begin_record = input * block_size;
            // SAFETY: this block's unread records still occupy their original
            // input range. Only fully read input blocks can become outputs.
            let source = unsafe { input_ptr.add(begin_record) };
            let length = block_size.min(input_len - begin_record);
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
                trace::emit(|| Event::Scatter {
                    input: begin_record + offset,
                    bucket: b,
                    physical: bucket.physical as usize,
                    // SAFETY: both cursors delimit the same reserved block.
                    slot: block_size - unsafe { end.offset_from(next) } as usize - 1,
                });
                if unlikely(next == end) {
                    debug_assert!(position.output <= input);
                    let physical = scratch_blocks + position.output;
                    assert!(position.output < full_blocks);
                    // SAFETY: output <= input; equality can occur only on
                    // this full input block's last record. The recycled block
                    // is fully read and uses the same raw input ancestor.
                    let out = unsafe { input_ptr.add(position.output * block_size) };
                    block_links[bucket.physical as usize] = physical as u32;
                    *bucket = BucketWriter {
                        next: out,
                        // SAFETY: the checked recycled input block is complete.
                        end: unsafe { out.add(block_size) },
                        physical: physical as u32,
                    };
                    position.output += 1;
                    trace::emit(|| Event::Writer {
                        bucket: b,
                        physical,
                    });
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
}

impl<T> Drop for Partition<'_, '_, T> {
    fn drop(&mut self) {
        if self.owns_recovery {
            if self.position.input * self.block_size < self.input_len {
                let written = self.position.output * self.block_size
                    + self
                        .buffers
                        .writers
                        .iter()
                        .map(|bucket| {
                            // SAFETY: both cursors delimit the same reserved block.
                            let free = unsafe { bucket.end.offset_from(bucket.next) as usize };
                            self.block_size - free
                        })
                        .sum::<usize>();
                let offset = written - self.position.input * self.block_size;
                // Input consumption is contiguous. Append only the unread
                // suffix, without the failed classifier. Writers and block
                // progress survive unwinding; no per-record counter is needed.
                self.scatter(offset, (), |_, _| 0);
                #[cfg(debug_assertions)]
                self.validate();
            }
            // The ledger takes over recovery before it starts moving records;
            // its guard finishes all bands even if completion itself unwinds.
            finish::complete_serial(self, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_completion_keeps_non_send_records_and_extractors() {
        use core::cell::Cell;
        use std::rc::Rc;
        let len = 257;
        for panic_at in [None, Some(1), Some(65), Some(len)] {
            let mut records: Vec<_> = (0..len).map(Rc::new).collect();
            let calls = Rc::new(Cell::new(0));
            let count = calls.clone();
            let mut buffers = Buffers::new();
            let result = unwind::catch(|| {
                partition(
                    &mut records,
                    2,
                    move |record: &Rc<usize>| {
                        let call = count.get() + 1;
                        count.set(call);
                        assert_ne!(Some(call), panic_at, "serial classifier panic");
                        **record % 4
                    },
                    &mut buffers,
                    &mut |bucket, band, _| {
                        assert!(band.iter().all(|record| **record % 4 == bucket))
                    },
                )
            });
            assert_eq!(result.is_err(), panic_at.is_some());
            assert_eq!(calls.get(), panic_at.unwrap_or(len));
            let mut ids: Vec<_> = records.iter().map(|record| **record).collect();
            if panic_at.is_none() {
                let mut expected: Vec<_> = (0..len).collect();
                expected.sort_by_key(|record| record % 4);
                assert_eq!(ids, expected);
            }
            ids.sort_unstable();
            assert_eq!(ids, (0..len).collect::<Vec<_>>());
            assert!(records.iter().all(|record| Rc::strong_count(record) == 1));
        }
    }

    #[test]
    fn integer_root_rounds_up() {
        // Pick a record width making the metadata/scratch ratio len / 2.
        let descriptor = finish::serial_descriptor_bytes();
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
            assert_eq!(
                balanced_block_size_integer(len, descriptor / 2, 1),
                expected
            );
        }
        // A remainder must round up even when the integer quotient is a square.
        assert_eq!(balanced_block_size_integer(20, descriptor * 5, 1), 1);
        assert_eq!(balanced_block_size_integer(21, descriptor * 5, 1), 2);
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
        // Scatter reads in input order; the panicking record and its unread
        // suffix join bucket zero without another key evaluation.
        let mut expected = records.clone();
        expected.sort_by_key(|record| {
            if (record.1 as usize) + 1 < at {
                record.0 & 3
            } else {
                0
            }
        });
        let mut buffers = Buffers::new();
        let mut calls = 0;
        let panic = unwind::catch(|| {
            let mut sorter = Partition::new(&mut records, 4, block_size, &mut buffers);
            let _ = sorter.step(|record: &(u32, u32)| {
                calls += 1;
                if calls == at {
                    std::panic::panic_any(0x71u32);
                }
                (record.0 & 3) as usize
            });
        })
        .expect_err("classifier did not panic");
        assert_eq!(*panic.downcast::<u32>().unwrap(), 0x71);
        assert_eq!(calls, at, "key called after it panicked");
        assert_eq!(records, expected, "unwind recovery at call {at}");
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
                    let _ = sorter.step(|record: &(u32, u32)| (record.0 & 3) as usize);
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
                    let payload =
                        unwind::catch(|| finish::complete_serial(&mut sorter, Some(&mut callback)))
                            .err();
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
