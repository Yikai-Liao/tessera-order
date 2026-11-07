// SPDX-License-Identifier: BSD-2-Clause

//! Dense per-job arena. No operation here acquires a lock or calls Rayon.

use std::mem::{size_of, MaybeUninit};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{BlockSpan, Config, Directory, Record, Stats};

#[derive(Clone, Copy)]
pub(crate) struct SlotAddress<T: Record>(pub(crate) *mut T);

// SAFETY: this wrapper transports only an address. Reads and writes occur solely
// through a LocalArena with an exclusive ownership lease for that physical slot.
unsafe impl<T: Record> Send for SlotAddress<T> {}
// SAFETY: the shared table is immutable. Distinct jobs access disjoint leased
// slot IDs; sharing an address value does not itself access the pointed-to bytes.
unsafe impl<T: Record> Sync for SlotAddress<T> {}

pub(crate) struct WorkerMemory<T: Record> {
    pub(crate) tile_a: Vec<T>,
    pub(crate) tile_b: Vec<MaybeUninit<T>>,
}

impl<T: Record> WorkerMemory<T> {
    pub(crate) fn new(capacity: usize) -> Self {
        let mut tile_b = Vec::with_capacity(capacity);
        tile_b.resize_with(capacity, MaybeUninit::uninit);
        Self {
            tile_a: Vec::with_capacity(capacity),
            tile_b,
        }
    }
}

pub(crate) struct LiveCounters {
    pub(crate) live: AtomicUsize,
    pub(crate) maximum: AtomicUsize,
    pub(crate) minimum_local_free: AtomicUsize,
    pub(crate) active: AtomicUsize,
    pub(crate) maximum_active: AtomicUsize,
}

pub(crate) struct LocalArena<'a, T: Record> {
    pointers: Vec<SlotAddress<T>>,
    global_ids: Vec<usize>,
    live_len: Vec<usize>,
    free_flags: Vec<bool>,
    free: Vec<usize>,
    tile_a: &'a mut Vec<T>,
    tile_b: &'a mut Vec<MaybeUninit<T>>,
    counters: &'a LiveCounters,
    config: Config,
    writes: usize,
    stats: Stats,
}

impl<'a, T: Record> LocalArena<'a, T> {
    pub(crate) fn new(
        table: &[SlotAddress<T>],
        source: Directory,
        leased: Vec<usize>,
        memory: &'a mut WorkerMemory<T>,
        counters: &'a LiveCounters,
        config: Config,
    ) -> (Self, Directory) {
        let source_slots = source.blocks.len();
        let slots = source_slots
            .checked_add(leased.len())
            .expect("local slot overflow");
        let mut global_ids = Vec::with_capacity(slots);
        let mut live_len = Vec::with_capacity(slots);
        let mut blocks = Vec::with_capacity(source_slots);
        for (local, span) in source.blocks.into_iter().enumerate() {
            global_ids.push(span.slot);
            live_len.push(span.len);
            blocks.push(BlockSpan {
                slot: local,
                len: span.len,
            });
        }
        global_ids.extend(leased);
        live_len.resize(slots, 0);
        let pointers = global_ids.iter().map(|&id| table[id]).collect();
        let mut free_flags = vec![false; slots];
        free_flags[source_slots..].fill(true);
        let free = (source_slots..slots).collect();
        let mut local = Self {
            pointers,
            global_ids,
            live_len,
            free_flags,
            free,
            tile_a: &mut memory.tile_a,
            tile_b: &mut memory.tile_b,
            counters,
            config,
            writes: 0,
            stats: Stats::default(),
        };
        let directory = Directory {
            blocks,
            len: source.len,
            min: source.min,
            max: source.max,
        };
        local.observe_metadata(std::slice::from_ref(&directory));
        (local, directory)
    }

    fn read(&self, slot: usize, at: usize) -> T {
        assert!(at < self.live_len[slot]);
        // SAFETY: every pointer addresses a leased full-capacity physical slot;
        // live_len records an initialized prefix, and all access is job-exclusive.
        unsafe { self.pointers[slot].0.add(at).read() }
    }

    fn write(&mut self, slot: usize, at: usize, value: T) {
        assert!(!self.free_flags[slot]);
        assert!(at < self.config.block_records);
        // SAFETY: this local slot is exclusively leased, has full B capacity,
        // and no unread source prefix has been released for output reuse.
        unsafe { self.pointers[slot].0.add(at).write(value) };
        self.live_len[slot] = self.live_len[slot].max(at + 1);
        self.writes += 1;
    }

    fn allocate(&mut self) -> usize {
        let slot = self
            .free
            .pop()
            .expect("bounded local free-slot budget exhausted");
        assert!(self.free_flags[slot]);
        self.free_flags[slot] = false;
        self.live_len[slot] = 0;
        let live = self.counters.live.fetch_add(1, Ordering::Relaxed) + 1;
        self.counters.maximum.fetch_max(live, Ordering::Relaxed);
        self.counters
            .minimum_local_free
            .fetch_min(self.free.len(), Ordering::Relaxed);
        slot
    }

    fn release(&mut self, slot: usize) {
        assert!(!self.free_flags[slot]);
        self.live_len[slot] = 0;
        self.free_flags[slot] = true;
        self.free.push(slot);
        self.counters.live.fetch_sub(1, Ordering::Relaxed);
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

    fn shift(&self, directory: &Directory) -> u32 {
        let diff = directory.min ^ directory.max;
        assert!(diff > 0);
        (64 - diff.leading_zeros()).saturating_sub(self.config.digit_bits)
    }

    pub(crate) fn partition(&mut self, input: Directory, shift: u32) -> Vec<Directory> {
        let r = 1usize << self.config.digit_bits;
        let mut out: Vec<_> = (0..r).map(|_| Directory::empty()).collect();
        self.stats.virtual_partitions += 1;
        for span in input.blocks {
            assert_eq!(span.len, self.live_len[span.slot]);
            for at in 0..span.len {
                let record = self.read(span.slot, at);
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
        for directory in &mut directories {
            out.len += directory.len;
            out.min = out.min.min(directory.min);
            out.max = out.max.max(directory.max);
            out.blocks.append(&mut directory.blocks);
        }
        out
    }
    fn leaf(&mut self, input: Directory) -> Directory {
        assert!(input.len <= self.config.tile_records);
        self.stats.tile_leaves += 1;
        self.tile_a.clear();
        for span in input.blocks {
            for at in 0..span.len {
                let value = self.read(span.slot, at);
                self.tile_a.push(value);
                self.writes += 1;
            }
            self.release(span.slot);
        }
        // Stable bottom-up merging uses only the second preallocated tile.
        let n = self.tile_a.len();
        let mut width = 1;
        while width < n {
            let mut start = 0;
            while start < n {
                let mid = (start + width).min(n);
                let end = (mid + width).min(n);
                let (mut a, mut b) = (start, mid);
                for dest in start..end {
                    let value =
                        if b == end || (a < mid && self.tile_a[a].key() <= self.tile_a[b].key()) {
                            let value = self.tile_a[a];
                            a += 1;
                            value
                        } else {
                            let value = self.tile_a[b];
                            b += 1;
                            value
                        };
                    self.tile_b[dest].write(value);
                }
                start = end;
            }
            for i in 0..n {
                // SAFETY: every destination 0..n was initialized in this merge pass.
                self.tile_a[i] = unsafe { self.tile_b[i].assume_init() };
            }
            self.writes += 2 * n;
            width *= 2;
        }
        let mut out = Directory::empty();
        for i in 0..n {
            let value = self.tile_a[i];
            self.push(&mut out, value);
        }
        self.tile_a.clear();
        out
    }

    fn lsd(&mut self, mut input: Directory) -> Directory {
        self.stats.lsd_leaves += 1;
        let relevant_bits = 64 - (input.min ^ input.max).leading_zeros();
        let mut shift = 0;
        while shift < relevant_bits {
            let buckets = self.partition(input, shift);
            input = Self::concat(buckets);
            shift += self.config.digit_bits;
        }
        self.pack(input)
    }

    fn child(&mut self, input: Directory, remaining_msd: usize) -> Directory {
        if input.len == 0 {
            return input;
        }
        if input.min == input.max {
            return self.pack(input);
        }
        if input.len <= self.config.tile_records {
            return self.leaf(input);
        }
        if remaining_msd == 0 {
            return self.lsd(input);
        }
        let shift = self.shift(&input);
        let children = self.partition(input, shift);
        let mut completed = Vec::with_capacity(children.len());
        for child in children {
            completed.push(self.child(child, remaining_msd - 1));
        }
        self.pack(Self::concat(completed))
    }

    fn observe_metadata(&mut self, directories: &[Directory]) {
        let local = self.pointers.capacity() * size_of::<SlotAddress<T>>()
            + (self.global_ids.capacity() + self.live_len.capacity() + self.free.capacity())
                * size_of::<usize>()
            + self.free_flags.capacity() * size_of::<bool>()
            + std::mem::size_of_val(directories)
            + directories
                .iter()
                .map(|d| d.blocks.capacity() * size_of::<BlockSpan>())
                .sum::<usize>();
        self.stats.observed_metadata_bytes = self.stats.observed_metadata_bytes.max(local);
    }

    pub(crate) fn sort_child(&mut self, source: Directory) -> Directory {
        self.child(source, self.config.msd_levels - 1)
    }

    pub(crate) fn finish(self, mut output: Directory) -> (Directory, Vec<usize>, Stats, usize) {
        assert_eq!(
            output.blocks.len(),
            output.len.div_ceil(self.config.block_records)
        );
        let mut seen = vec![false; self.global_ids.len()];
        for span in &mut output.blocks {
            assert!(!seen[span.slot]);
            assert!(!self.free_flags[span.slot]);
            assert_eq!(span.len, self.live_len[span.slot]);
            seen[span.slot] = true;
            span.slot = self.global_ids[span.slot];
        }
        let mut returned = Vec::with_capacity(self.free.len());
        for &slot in &self.free {
            assert!(!seen[slot]);
            assert!(self.free_flags[slot]);
            assert_eq!(self.live_len[slot], 0);
            seen[slot] = true;
            returned.push(self.global_ids[slot]);
        }
        assert!(seen.into_iter().all(|owned| owned));
        (output, returned, self.stats, self.writes)
    }
}
