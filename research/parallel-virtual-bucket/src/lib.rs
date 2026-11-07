// SPDX-License-Identifier: BSD-2-Clause

//! Copy-only, fixed-depth directory-native stable radix-sort with bounded parallel children.
//!
//! Records remain in the caller's input blocks and a bounded scratch arena.
//! This is not the generic Tessera API: keys must be deterministic and infallible,
//! records must be `Copy`, the root stage is serial, and panic recovery is not promised.

use std::mem::{size_of, MaybeUninit};
use std::time::{Duration, Instant};

mod local;
mod parallel;
use local::WorkerMemory;

mod sealed {
    pub trait Sealed {}
    impl Sealed for u64 {}
    impl Sealed for super::Row16 {}
}

/// Supplies the deterministic, infallible unsigned key used by this experiment.
///
/// This trait is sealed to `u64` and `Row16`; external key callbacks are not accepted.
pub trait Record: Copy + Send + sealed::Sealed {
    /// Returns the complete unsigned key. This method must not panic or change.
    fn key(self) -> u64;
}

impl Record for u64 {
    fn key(self) -> u64 {
        self
    }
}

/// A key and original-position payload for stability verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Row16 {
    /// Sort key.
    pub key: u64,
    /// Payload not inspected by the algorithm.
    pub ordinal: u64,
}

impl Record for Row16 {
    fn key(self) -> u64 {
        self.key
    }
}

/// Selects the scheduler while keeping the exact leased-job kernel identical.
#[derive(Clone, Copy, Debug)]
pub enum Scheduler {
    /// Uses explicit worker closures on the ambient Rayon pool.
    Rayon,
    /// Uses scoped OS threads; this path is also used for parallel Miri checks.
    Scoped,
}

/// Explicit algorithm and record-capacity limits.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Number of records in one physical full block.
    pub block_records: usize,
    /// Maximum digit width, between one and eight bits.
    pub digit_bits: u32,
    /// Maximum records in each of two leaf buffers.
    pub tile_records: usize,
    /// Maximum MSD partitions on any root-to-leaf path, one or two.
    pub msd_levels: usize,
    /// Root shards, processed serially in this foundation implementation.
    pub root_shards: usize,
    /// Maximum simultaneous child jobs, enforced by an explicit worker loop.
    pub child_workers: usize,
    /// Child worker scheduler.
    pub scheduler: Scheduler,
}

impl Config {
    /// Chooses bounded blocks and leaves for the input and record size.
    pub fn for_input<T>(n: usize) -> Self {
        let digit_bits = 8;
        let block_records = ((n.max(1) as f64 / 256.0).sqrt().ceil() as usize).max(16);
        Self {
            block_records,
            digit_bits,
            tile_records: (65536 / size_of::<T>().max(1)).max(1),
            msd_levels: 2,
            root_shards: 1,
            child_workers: 4,
            scheduler: Scheduler::Rayon,
        }
    }
}

/// One initialized prefix of one physical block.
#[derive(Clone, Copy, Debug)]
pub struct BlockSpan {
    /// Arena slot, never a logical record index.
    pub slot: usize,
    /// Initialized prefix length.
    pub len: usize,
}

/// A stable logical record stream that does not imply contiguous storage.
#[derive(Debug)]
pub struct Directory {
    /// Physical block prefixes in stable logical order.
    pub blocks: Vec<BlockSpan>,
    /// Total records in the stream.
    pub len: usize,
    /// Smallest key in the stream, or `u64::MAX` when empty.
    pub min: u64,
    /// Largest key in the stream, or zero when empty.
    pub max: u64,
}

impl Directory {
    fn empty() -> Self {
        Self {
            blocks: Vec::new(),
            len: 0,
            min: u64::MAX,
            max: 0,
        }
    }
}

/// Separate movement and allocation accounting, excluding verification fixtures.
#[derive(Debug, Default, Clone)]
pub struct Stats {
    /// Existing caller allocation, never counted as scratch.
    pub original_input_bytes: usize,
    /// Physically allocated extra record bytes: arena plus both leaf buffers.
    pub auxiliary_record_bytes: usize,
    /// Arena extra full-block capacity, including slots retained by output.
    pub scratch_blocks: usize,
    /// Original full blocks made available for reuse.
    pub input_full_blocks: usize,
    /// Minimum simultaneously free arena slots.
    pub minimum_free_blocks: usize,
    /// Largest number of live arena slots, including original input slots.
    pub maximum_live_blocks: usize,
    /// Directory and slot metadata capacity observed at stage boundaries.
    pub observed_metadata_bytes: usize,
    /// Bytes written by root scatter, not including its read traffic.
    pub root_write_bytes: usize,
    /// Bytes written while sorting child directories, including leaf buffers.
    pub child_write_bytes: usize,
    /// Bytes written by final global packing.
    pub repack_write_bytes: usize,
    /// Bytes written by final physical restoration, including displaced records.
    pub restore_write_bytes: usize,
    /// Nonempty virtual digit partition calls.
    pub virtual_partitions: usize,
    /// Leaves sorted in bounded tile memory.
    pub tile_leaves: usize,
    /// Oversized fixed-depth leaves handled by virtual LSD.
    pub lsd_leaves: usize,
    /// Root scatter stage time.
    pub root_time: Duration,
    /// Directory child sorting stage time.
    pub child_time: Duration,
    /// Global packing stage time, separate from restoration.
    pub repack_time: Duration,
    /// Physical restoration stage time, separate from packing.
    pub restore_time: Duration,
    /// Worker closures requested for the child phase, independent of pool size.
    pub child_workers: usize,
    /// Ambient Rayon pool size observed in the child phase.
    pub ambient_threads: usize,
    /// Maximum simultaneous child jobs with slot leases.
    pub maximum_active_children: usize,
    /// Lowest free local block count during a child job.
    pub minimum_worker_free_blocks: usize,
}

/// Owns bounded scratch and exclusively borrows the original record allocation.
///
/// Slot IDs below `input_full_blocks` refer directly into the original input.
/// The last short input fragment is readable but never recycled as a full slot.
/// All remaining reusable slots belong to the single fixed scratch allocation.
pub struct VirtualSorter<'a, T: Record> {
    input: &'a mut [T],
    scratch: Vec<MaybeUninit<T>>,
    worker_memory: Vec<WorkerMemory<T>>,
    config: Config,
    full: usize,
    scratch_count: usize,
    tail_slot: Option<usize>,
    free: Vec<usize>,
    live_len: Vec<usize>,
    free_flags: Vec<bool>,
    live: usize,
    writes: usize,
    stats: Stats,
}

fn adjacent_shard_index(block: usize, shards: usize, total_blocks: usize) -> usize {
    assert!(shards > 0 && shards <= total_blocks && block < total_blocks);
    // On supported 32-/64-bit targets, both factors fit u64 and their product
    // fits u128 even when a valid input's index product does not fit usize.
    usize::try_from((block as u128 * shards as u128) / total_blocks as u128)
        .expect("shard index is below the usize shard count")
}

impl<'a, T: Record> VirtualSorter<'a, T> {
    /// Allocates the entire fixed extra record budget before any mutation.
    ///
    /// # Panics
    /// Panics for invalid parameters, zero-sized records, or capacity overflow.
    pub fn new(input: &'a mut [T], config: Config) -> Self {
        assert!(size_of::<T>() != 0);
        assert!(config.block_records > 0);
        assert!((1..=8).contains(&config.digit_bits));
        assert!((1..=2).contains(&config.msd_levels));
        assert!(config.tile_records > 0);
        assert!(config.tile_records <= 65536 / size_of::<T>());
        assert!(config.root_shards > 0);
        assert!(config.child_workers > 0);
        let b = config.block_records;
        let full = input.len() / b;
        let tail = input.len() % b;
        let r = 1usize << config.digit_bits;
        // Root and child phases do not overlap. Retained root tails plus one
        // fixed 3R+8 slot lease per explicit worker cover the active frontier.
        let scratch_count = config
            .root_shards
            .checked_mul(r)
            .unwrap()
            .checked_add(config.child_workers.checked_mul(3 * r + 8).unwrap())
            .unwrap()
            .checked_add(8)
            .unwrap();
        let capacity = scratch_count
            .checked_mul(b)
            .expect("record capacity overflow");
        std::alloc::Layout::array::<MaybeUninit<T>>(capacity).expect("scratch layout overflow");
        std::alloc::Layout::array::<T>(config.tile_records).expect("tile layout overflow");
        let reusable_slots = full
            .checked_add(scratch_count)
            .expect("slot count overflow");
        let slots = reusable_slots
            .checked_add(usize::from(tail > 0))
            .expect("slot count overflow");
        let original_input_bytes = input
            .len()
            .checked_mul(size_of::<T>())
            .expect("input bytes overflow");
        let mut scratch = Vec::with_capacity(capacity);
        scratch.resize_with(capacity, MaybeUninit::uninit);
        let worker_memory: Vec<_> = (0..config.child_workers)
            .map(|_| WorkerMemory::new(config.tile_records))
            .collect();
        let tile_capacity = worker_memory
            .iter()
            .try_fold(0usize, |n, w| {
                n.checked_add(w.tile_a.capacity())
                    .and_then(|n| n.checked_add(w.tile_b.capacity()))
            })
            .expect("tile capacity overflow");
        let tail_slot = (tail > 0).then_some(reusable_slots);
        let mut live_len = vec![0; slots];
        live_len[..full].fill(b);
        if let Some(slot) = tail_slot {
            live_len[slot] = tail;
        }
        let mut free_flags = vec![false; slots];
        free_flags[full..full + scratch_count].fill(true);
        let free = (full..full + scratch_count).collect();
        let live = full + usize::from(tail > 0);
        let stats = Stats {
            original_input_bytes,
            auxiliary_record_bytes: scratch
                .capacity()
                .checked_add(tile_capacity)
                .and_then(|n| n.checked_mul(size_of::<T>()))
                .expect("auxiliary bytes overflow"),
            scratch_blocks: scratch_count,
            input_full_blocks: full,
            minimum_free_blocks: scratch_count,
            maximum_live_blocks: live,
            ..Stats::default()
        };
        Self {
            input,
            scratch,
            worker_memory,
            config,
            full,
            scratch_count,
            tail_slot,
            free,
            live_len,
            free_flags,
            live,
            writes: 0,
            stats,
        }
    }

    fn ptr(&mut self, slot: usize) -> *mut T {
        let b = self.config.block_records;
        if slot < self.full {
            let offset = slot.checked_mul(b).expect("input block offset overflow");
            assert!(offset < self.input.len());
            // SAFETY: the constructor counted full input blocks; checked offset is in range.
            unsafe { self.input.as_mut_ptr().add(offset) }
        } else if slot < self.full + self.scratch_count {
            let offset = (slot - self.full)
                .checked_mul(b)
                .expect("scratch offset overflow");
            assert!(offset < self.scratch.len());
            // SAFETY: scratch contains complete blocks; checked offset is in range.
            unsafe { self.scratch.as_mut_ptr().add(offset).cast() }
        } else {
            assert_eq!(Some(slot), self.tail_slot);
            let offset = self.full.checked_mul(b).expect("tail offset overflow");
            assert!(offset < self.input.len());
            // SAFETY: tail_slot exists only for a nonempty final input fragment.
            unsafe { self.input.as_mut_ptr().add(offset) }
        }
    }

    fn read(&mut self, slot: usize, at: usize) -> T {
        assert!(at < self.live_len[slot]);
        // SAFETY: only initialized live prefixes are read, within physical capacity.
        unsafe { self.ptr(slot).add(at).read() }
    }

    fn write(&mut self, slot: usize, at: usize, record: T) {
        assert!(!self.free_flags[slot]);
        assert_ne!(Some(slot), self.tail_slot);
        assert!(at < self.config.block_records);
        // SAFETY: this nonfree reusable full slot has room for the initialized value.
        unsafe { self.ptr(slot).add(at).write(record) };
        self.live_len[slot] = self.live_len[slot].max(at + 1);
        self.writes += 1;
    }

    fn allocate(&mut self) -> usize {
        let slot = self.free.pop().expect("proved fixed slot budget exhausted");
        assert!(self.free_flags[slot]);
        self.free_flags[slot] = false;
        self.live_len[slot] = 0;
        self.live += 1;
        self.stats.maximum_live_blocks = self.stats.maximum_live_blocks.max(self.live);
        self.stats.minimum_free_blocks = self.stats.minimum_free_blocks.min(self.free.len());
        slot
    }

    fn release(&mut self, slot: usize) {
        assert!(!self.free_flags[slot]);
        self.live_len[slot] = 0;
        self.live -= 1;
        if Some(slot) != self.tail_slot {
            self.free_flags[slot] = true;
            self.free.push(slot);
        }
    }

    fn push(&mut self, directory: &mut Directory, record: T) {
        let b = self.config.block_records;
        if directory.blocks.last().is_none_or(|span| span.len == b) {
            let slot = self.allocate();
            directory.blocks.push(BlockSpan { slot, len: 0 });
        }
        let span = directory.blocks.last_mut().unwrap();
        self.write(span.slot, span.len, record);
        span.len += 1;
        directory.len += 1;
        let key = record.key();
        directory.min = directory.min.min(key);
        directory.max = directory.max.max(key);
    }

    fn initial(&mut self) -> Directory {
        let mut d = Directory::empty();
        for slot in 0..self.full {
            d.blocks.push(BlockSpan {
                slot,
                len: self.config.block_records,
            });
        }
        if let Some(slot) = self.tail_slot {
            d.blocks.push(BlockSpan {
                slot,
                len: self.live_len[slot],
            });
        }
        d.len = self.input.len();
        for record in self.input.iter().copied() {
            d.min = d.min.min(record.key());
            d.max = d.max.max(record.key());
        }
        d
    }

    fn shift(&self, directory: &Directory) -> u32 {
        let diff = directory.min ^ directory.max;
        assert!(diff > 0);
        (64 - diff.leading_zeros()).saturating_sub(self.config.digit_bits)
    }

    fn partition(&mut self, input: Directory, shift: u32) -> Vec<Directory> {
        let r = 1usize << self.config.digit_bits;
        let mut out: Vec<_> = (0..r).map(|_| Directory::empty()).collect();
        self.stats.virtual_partitions += 1;
        for span in input.blocks {
            assert_eq!(span.len, self.live_len[span.slot]);
            for at in 0..span.len {
                let record = self.read(span.slot, at);
                // The local Copy value is the only record still needed from this
                // source after its final read. Reuse cannot overwrite unread data.
                if at + 1 == span.len {
                    self.release(span.slot);
                }
                let digit = ((record.key() >> shift) as usize) & (r - 1);
                self.push(&mut out[digit], record);
            }
        }
        self.observe_metadata(&out);
        out
    }

    fn pack(&mut self, input: Directory) -> Directory {
        let mut out = Directory::empty();
        for span in input.blocks {
            for at in 0..span.len {
                let record = self.read(span.slot, at);
                if at + 1 == span.len {
                    self.release(span.slot);
                }
                self.push(&mut out, record);
            }
        }
        out
    }

    fn concat(mut directories: Vec<Directory>) -> Directory {
        let mut out = Directory::empty();
        for d in &mut directories {
            out.len += d.len;
            out.min = out.min.min(d.min);
            out.max = out.max.max(d.max);
            out.blocks.append(&mut d.blocks);
        }
        out
    }

    fn observe_metadata(&mut self, directories: &[Directory]) {
        let bytes = self.free.capacity() * size_of::<usize>()
            + self.live_len.capacity() * size_of::<usize>()
            + self.free_flags.capacity() * size_of::<bool>()
            + directories
                .iter()
                .map(|d| d.blocks.capacity() * size_of::<BlockSpan>())
                .sum::<usize>()
            + std::mem::size_of_val(directories);
        self.stats.observed_metadata_bytes = self.stats.observed_metadata_bytes.max(bytes);
    }

    /// Performs stable root scatter and returns the adopted bucket directories.
    ///
    /// Shards are adjacent input block ranges. Adoption is bucket-major then
    /// original shard order, never completion order. This executable first stage
    /// schedules those shards serially, but retains the same directory boundary.
    pub fn scatter_root(&mut self) -> Vec<Directory> {
        let started = Instant::now();
        let writes = self.writes;
        let initial = self.initial();
        if initial.len == 0 || initial.min == initial.max {
            self.stats.root_time = started.elapsed();
            return vec![initial];
        }
        let shift = self.shift(&initial);
        let shard_count = self.config.root_shards.min(initial.blocks.len().max(1));
        let mut shards: Vec<_> = (0..shard_count).map(|_| Directory::empty()).collect();
        let total_blocks = initial.blocks.len();
        for (i, span) in initial.blocks.into_iter().enumerate() {
            let index = adjacent_shard_index(i, shard_count, total_blocks);
            shards[index].len += span.len;
            shards[index].blocks.push(span);
        }
        let r = 1usize << self.config.digit_bits;
        let mut adopted: Vec<_> = (0..r).map(|_| Directory::empty()).collect();
        for shard in shards {
            let buckets = self.partition(shard, shift);
            for (dest, mut source) in adopted.iter_mut().zip(buckets) {
                dest.len += source.len;
                dest.min = dest.min.min(source.min);
                dest.max = dest.max.max(source.max);
                dest.blocks.append(&mut source.blocks);
            }
        }
        self.observe_metadata(&adopted);
        self.stats.root_time = started.elapsed();
        self.stats.root_write_bytes = (self.writes - writes) * size_of::<T>();
        adopted
    }

    /// Sorts already-adopted root buckets directly through their slot directories.
    ///
    /// Every root bucket must appear in key order, contain its correct min/max,
    /// and exclusively own its spans. Passing the result of `scatter_root` meets
    /// these requirements. No root materialization or fake contiguous slice occurs.
    pub fn sort_adopted_roots(&mut self, roots: Vec<Directory>) -> Directory {
        parallel::sort_children(self, roots)
    }

    /// Packs all sorted root boundaries into one globally block-aligned stream.
    pub fn repack_global(&mut self, directory: Directory) -> Directory {
        let started = Instant::now();
        let writes = self.writes;
        let result = self.pack(directory);
        self.stats.repack_time = started.elapsed();
        self.stats.repack_write_bytes = (self.writes - writes) * size_of::<T>();
        result
    }

    /// Restores a globally packed stream into the original input allocation.
    ///
    /// This serial Copy-only block-swap baseline is separate from global repacking.
    /// The generic and parallel repack/restore research is intentionally independent.
    pub fn restore(mut self, mut directory: Directory) -> Stats {
        let started = Instant::now();
        let writes = self.writes;
        assert_eq!(directory.len, self.input.len());
        assert_eq!(
            directory.blocks.len(),
            self.input.len().div_ceil(self.config.block_records)
        );
        let mut owner = vec![usize::MAX; self.live_len.len()];
        for (logical, span) in directory.blocks.iter().enumerate() {
            assert_eq!(owner[span.slot], usize::MAX);
            owner[span.slot] = logical;
            assert_eq!(span.len, self.live_len[span.slot]);
        }
        for logical in 0..self.full {
            let source = directory.blocks[logical].slot;
            assert_eq!(directory.blocks[logical].len, self.config.block_records);
            if source == logical {
                continue;
            }
            let displaced = owner[logical];
            let displaced_len = if displaced == usize::MAX {
                0
            } else {
                directory.blocks[displaced].len
            };
            // Each initialized destination prefix is exchanged through one Copy
            // register. No uninitialized scratch is read, even for a short tail.
            for at in 0..self.config.block_records {
                let value = self.read(source, at);
                let old = (at < displaced_len).then(|| self.read(logical, at));
                // Restore may target a free input block. Marking it nonfree here
                // is safe because no arena allocation follows the restore phase.
                self.free_flags[logical] = false;
                self.write(logical, at, value);
                if let Some(old) = old {
                    self.write(source, at, old);
                }
            }
            directory.blocks[logical].slot = logical;
            owner[logical] = logical;
            owner[source] = displaced;
            self.live_len[logical] = self.config.block_records;
            self.live_len[source] = displaced_len;
            if displaced != usize::MAX {
                directory.blocks[displaced].slot = source;
            }
        }
        let tail_len = self.input.len() % self.config.block_records;
        if tail_len > 0 {
            let source = directory.blocks[self.full].slot;
            for at in 0..tail_len {
                let value = self.read(source, at);
                self.input[self.full * self.config.block_records + at] = value;
                self.writes += 1;
            }
        }
        self.stats.observed_metadata_bytes += owner.capacity() * size_of::<usize>();
        self.stats.restore_time = started.elapsed();
        self.stats.restore_write_bytes = (self.writes - writes) * size_of::<T>();
        self.stats
    }

    /// Verifies whole-slot ownership and capacity at an externally visible boundary.
    ///
    /// Uses only one Boolean per physical slot, not one token per record.
    pub fn verify_directories(&self, directories: &[Directory]) {
        let mut seen = vec![false; self.live_len.len()];
        let mut total = 0;
        for directory in directories {
            assert_eq!(
                directory.len,
                directory.blocks.iter().map(|s| s.len).sum::<usize>()
            );
            for span in &directory.blocks {
                assert!(span.len > 0 && span.len <= self.config.block_records);
                assert!(!seen[span.slot]);
                assert!(!self.free_flags[span.slot]);
                assert_eq!(span.len, self.live_len[span.slot]);
                seen[span.slot] = true;
                total += span.len;
            }
        }
        assert_eq!(total, self.input.len());
        for (slot, &length) in self.live_len.iter().enumerate() {
            assert_eq!(length > 0, seen[slot]);
        }
        let mut free_seen = vec![false; self.live_len.len()];
        for &slot in &self.free {
            assert!(!free_seen[slot], "duplicate slot in free pool");
            assert!(self.free_flags[slot]);
            assert!(!seen[slot]);
            assert_ne!(Some(slot), self.tail_slot);
            free_seen[slot] = true;
        }
        assert_eq!(free_seen, self.free_flags);
        assert_eq!(self.live, seen.iter().filter(|&&live| live).count());
    }
}

/// Runs serial root/final stages and bounded parallel children without an n-record buffer.
pub fn sort<T: Record>(input: &mut [T], config: Config) -> Stats {
    let mut sorter = VirtualSorter::new(input, config);
    let roots = sorter.scatter_root();
    let sorted = sorter.sort_adopted_roots(roots);
    let packed = sorter.repack_global(sorted);
    sorter.restore(packed)
}

#[cfg(test)]
mod arithmetic_tests {
    use super::adjacent_shard_index;

    #[test]
    fn miri_adjacent_shard_index_does_not_overflow() {
        assert_eq!(65536u32.wrapping_mul(65537) / 65537, 0);
        assert_eq!(adjacent_shard_index(65536, 65537, 65537), 65536);
        assert_eq!(
            adjacent_shard_index(usize::MAX - 1, usize::MAX, usize::MAX),
            usize::MAX - 1
        );
        assert_eq!(
            adjacent_shard_index(usize::MAX - 1, 65537, usize::MAX),
            65536
        );
        for total in 1..=17 {
            for shards in 1..=total {
                let mut previous = 0;
                for block in 0..total {
                    let shard = adjacent_shard_index(block, shards, total);
                    assert!(shard >= previous && shard - previous <= 1);
                    assert!(shard < shards);
                    previous = shard;
                }
                assert_eq!(previous, shards - 1);
            }
        }
    }
}
