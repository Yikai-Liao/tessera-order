// SPDX-License-Identifier: BSD-2-Clause

//! Bounded bucket sorting with protected writes into final input bands.

use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

/// A physical block without a live source or finalized band fragment.
const FREE: usize = usize::MAX;
/// A physical block reserved for disjoint final-band writes.
const FINAL: usize = usize::MAX - 1;
/// One initialized source prefix in the partition directory.
struct Source {
    logical: usize,
    len: usize,
}

/// A bucket's source prefixes and temporary record ownership.
struct Bucket<T> {
    sources: Vec<Source>,
    start: usize,
    len: usize,
    records: Vec<MaybeUninit<T>>,
    cache: usize,
    staged: bool,
    completed: bool,
    panic: Option<unwind::Payload>,
}

/// Physical owners, relocatable logical sources and free block candidates.
struct Storage<T> {
    addresses: Vec<*mut T>,
    input: *mut T,
    full_blocks: usize,
    scratch_blocks: usize,
    block_size: usize,
    owners: Vec<Mutex<usize>>,
    locations: Vec<AtomicUsize>,
    lengths: Vec<usize>,
    free: Vec<Mutex<Vec<usize>>>,
}

// SAFETY: no T references are shared. A physical owner mutex protects source
// relocation and gathering; final writes cover disjoint fixed record bands.
unsafe impl<T: Send> Sync for Storage<T> {}

impl<T> Storage<T> {
    /// Finds a free candidate in the local cache, reserve, or another bucket.
    fn take_free(&self, current: usize, local: &mut Vec<usize>) -> usize {
        if let Some(physical) = local.pop() {
            return physical;
        }
        let buckets = self.free.len() - 1;
        loop {
            if let Some(physical) = lock(&self.free[buckets]).pop() {
                return physical;
            }
            for offset in 1..buckets {
                let peer = (current + offset) % buckets;
                let mut free = match self.free[peer].try_lock() {
                    Ok(free) => free,
                    Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
                    Err(std::sync::TryLockError::WouldBlock) => continue,
                };
                if let Some(physical) = free.pop() {
                    return physical;
                }
            }
            // A candidate can be checked out or its cache can be temporarily
            // locked. Hold no physical owner while waiting. Cache holders run
            // no user callbacks and can therefore finish or publish progress.
            std::thread::yield_now();
        }
    }

    /// Moves one still-live prefix into private staging.
    fn gather(&self, source: &Source, output: *mut T) -> usize {
        loop {
            let physical = self.locations[source.logical].load(Ordering::Acquire);
            let mut owner = lock(&self.owners[physical]);
            if *owner != source.logical {
                continue;
            }
            // SAFETY: the verified owner mutex protects this initialized
            // source prefix until it has moved to private staging.
            unsafe {
                ptr::copy_nonoverlapping(self.addresses[physical], output, source.len);
            }
            *owner = FREE;
            return physical;
        }
    }

    /// Evacuates an occupied destination using locks in physical ID order.
    fn protect(&self, physical: usize, local: &mut Vec<usize>, cache: usize) {
        loop {
            let mut owner = lock(&self.owners[physical]);
            if *owner == FINAL || *owner == FREE {
                *owner = FINAL;
                return;
            }
            drop(owner);
            let free = self.take_free(cache, local);
            if free == physical {
                // A free-list entry can become stale only after finalization.
                let mut owner = lock(&self.owners[physical]);
                assert!(*owner == FREE || *owner == FINAL);
                *owner = FINAL;
                return;
            }
            let (lower, upper) = if physical < free {
                (physical, free)
            } else {
                (free, physical)
            };
            let mut lower_owner = lock(&self.owners[lower]);
            let mut upper_owner = lock(&self.owners[upper]);
            let (owner, free_owner) = if physical < free {
                (&mut *lower_owner, &mut *upper_owner)
            } else {
                (&mut *upper_owner, &mut *lower_owner)
            };
            if *free_owner != FREE {
                assert_eq!(*free_owner, FINAL);
                continue;
            }
            if *owner != FREE && *owner != FINAL {
                let logical = *owner;
                // SAFETY: both physical blocks are locked, the destination
                // was free, and the source prefix is still uniquely live.
                unsafe {
                    ptr::copy_nonoverlapping(
                        self.addresses[physical],
                        self.addresses[free],
                        self.lengths[logical],
                    );
                }
                *free_owner = logical;
                self.locations[logical].store(free, Ordering::Release);
            } else {
                local.push(free);
            }
            *owner = FINAL;
            return;
        }
    }

    /// Writes sorted records to a band's final positions without a permutation.
    fn write(
        &self,
        start: usize,
        records: *const T,
        len: usize,
        local: &mut Vec<usize>,
        cache: usize,
    ) {
        let mut offset = 0;
        while offset < len {
            let position = start + offset;
            let block = position / self.block_size;
            let begin = position % self.block_size;
            let count = (self.block_size - begin).min(len - offset);
            if block < self.full_blocks {
                self.protect(self.scratch_blocks + block, local, cache);
            }
            // SAFETY: physical protection evacuated any unread source. Final
            // bands are disjoint, including shared block fragments and the
            // input tail, which no partition source ever occupies.
            unsafe {
                ptr::copy_nonoverlapping(records.add(offset), self.input.add(position), count);
            }
            offset += count;
        }
    }
}

/// Acquires an internal metadata lock, never held across a user callback.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl<T> Bucket<T> {
    /// Stages, sorts, and writes a bucket, completing writes after a key panic.
    fn finish(&mut self, storage: &Storage<T>, sort: Option<BucketSort<'_, T>>) {
        if self.completed || self.len == 0 {
            self.completed = true;
            return;
        }
        if !self.staged {
            // Allocate before transferring ownership; completed buckets release
            // capacity so ordinary scheduling retains only active buffers.
            self.records.resize_with(self.len, MaybeUninit::uninit);
            let mut offset = 0;
            for source in &self.sources {
                // SAFETY: staging has capacity for the concatenated prefixes.
                let output = unsafe { self.records.as_mut_ptr().add(offset).cast() };
                lock(&storage.free[self.cache]).push(storage.gather(source, output));
                offset += source.len;
            }
            self.staged = true;
        }
        if let Some(sort) = sort {
            // SAFETY: staging owns the complete bucket. The serial sorter
            // restores this slice before a key panic propagates.
            let records = unsafe {
                core::slice::from_raw_parts_mut(self.records.as_mut_ptr().cast::<T>(), self.len)
            };
            self.panic = unwind::catch(|| sort(records)).err();
        }
        {
            let mut free = lock(&storage.free[self.cache]);
            storage.write(
                self.start,
                self.records.as_ptr().cast(),
                self.len,
                &mut free,
                self.cache,
            );
        }
        self.completed = true;
        self.records = Vec::new();
    }
}

/// Sorts bounded virtual buckets before writing their final record bands.
pub(super) fn try_finish<T: Send>(
    sorter: &mut Partition<'_, '_, T>,
    ends: &[usize],
    parallel: bool,
    limit: usize,
    finish: &(dyn Fn(&mut [T]) + Sync),
) -> bool {
    let mut start = 0;
    for &end in ends {
        if end - start > limit {
            return false;
        }
        start = end;
    }
    let block_size = sorter.blocks.block_size;
    let count = sorter.block_order.len();
    let head = sorter.active_blocks.start;
    let input_len = sorter.blocks.input_len;
    let mut owners = vec![FREE; count];
    let mut locations = Vec::new();
    let mut lengths = Vec::new();
    let mut buckets = Vec::with_capacity(ends.len());
    let mut logical = head;
    let mut partial = 0;
    let mut start = 0;
    for (bucket, &end) in ends.iter().enumerate() {
        let logical_end = sorter.buffers.counts[bucket];
        let mut sources = Vec::with_capacity(logical_end - logical);
        while logical < logical_end {
            let len = sorter.next_block_len(logical, &mut partial);
            if len != 0 {
                let physical = sorter.block_order[logical] as usize;
                let source = locations.len();
                sources.push(Source {
                    logical: source,
                    len,
                });
                owners[physical] = source;
                locations.push(AtomicUsize::new(physical));
                lengths.push(len);
            }
            logical += 1;
        }
        buckets.push(Bucket {
            cache: bucket,
            sources,
            start,
            len: end - start,
            records: Vec::new(),
            staged: false,
            completed: false,
            panic: None,
        });
        start = end;
    }
    let mut free = Vec::with_capacity(count);
    free.extend(
        owners
            .iter()
            .enumerate()
            .filter_map(|(physical, &owner)| (owner == FREE).then_some(physical)),
    );
    let mut caches: Vec<_> = buckets
        .iter()
        .map(|bucket| Mutex::new(Vec::with_capacity(bucket.sources.len())))
        .collect();
    caches.push(Mutex::new(free));
    let storage = Storage {
        addresses: (0..count)
            .map(|physical| {
                if sorter.blocks.scratch_segments.is_empty() {
                    sorter.blocks.block_ptr(physical)
                } else {
                    sorter.blocks.block_ptr_segmented(physical)
                }
            })
            .collect(),
        input: sorter.blocks.input,
        full_blocks: input_len / block_size,
        scratch_blocks: sorter.blocks.scratch_blocks,
        block_size,
        owners: owners.into_iter().map(Mutex::new).collect(),
        locations,
        lengths,
        free: caches,
    };
    let result = unwind::catch(|| {
        if parallel {
            buckets
                .par_iter_mut()
                .for_each(|bucket| bucket.finish(&storage, Some(finish)));
        } else {
            buckets
                .iter_mut()
                .for_each(|bucket| bucket.finish(&storage, Some(finish)));
        }
    });
    if result.is_err() {
        for bucket in &mut buckets {
            bucket.finish(&storage, None);
        }
    }
    sorter.input_materialized = true;
    if let Err(payload) = result {
        unwind::resume(payload);
    }
    if let Some(payload) = buckets.iter_mut().find_map(|bucket| bucket.panic.take()) {
        unwind::resume(payload);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::ByKey;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    /// Forces bounded finishing independently of production routing cutoffs.
    fn finish_records<T: Send, E: Extract<T> + Clone + Send>(
        records: &mut [T],
        key: E,
        block_size: usize,
        shards: usize,
        finish: &(dyn Fn(&mut [T]) + Sync),
    ) {
        partition_impl_with_visit(
            records,
            3,
            5,
            key,
            &mut Buffers::new(),
            Config {
                shards,
                block_size: Some(block_size),
                parallel: !cfg!(miri),
                finish: Some(BoundedFinish {
                    limit: usize::MAX,
                    sort: finish,
                }),
            },
            Some(&mut |_| panic!("bounded finishing unexpectedly declined")),
        );
    }

    #[test]
    fn sparse_buckets_keep_directory_ends() {
        for shards in [2, 5] {
            let mut records: Vec<_> = (0..97u32)
                .map(|id| (32 + (id % 2) * 128 + id % 16, id.to_string()))
                .collect();
            let mut expected = records.clone();
            expected.sort_by_key(|record| record.0);
            // Only buckets 1 and 5 are populated: empty leading, interior and
            // trailing buckets must not change the next bucket's source range.
            finish_records(
                &mut records,
                ByKey(|record: &(u32, String)| record.0),
                5,
                shards,
                &|records| records.sort_by_key(|record| record.0),
            );
            assert_eq!(records, expected);
        }
    }

    #[test]
    fn callback_panic_keeps_owners() {
        /// An owned payload whose destructor detects loss or duplication.
        struct Owned {
            key: u32,
            id: usize,
            text: String,
            drops: Arc<Vec<AtomicUsize>>,
        }
        impl Drop for Owned {
            fn drop(&mut self) {
                assert_eq!(self.text, self.id.to_string());
                self.drops[self.id].fetch_add(1, Ordering::Relaxed);
            }
        }
        for fail in [None, Some(0), Some(31), Some(96)] {
            let len = 97;
            let drops = Arc::new((0..len).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
            let mut records: Vec<_> = (0..len)
                .map(|id| Owned {
                    key: ((id * 7919 + 73) % 256) as u32,
                    id,
                    text: id.to_string(),
                    drops: drops.clone(),
                })
                .collect();
            let mut expected: Vec<_> = records
                .iter()
                .map(|record| (record.key, record.id))
                .collect();
            expected.sort_by_key(|record| record.0);
            let result = unwind::catch(|| {
                finish_records(
                    &mut records,
                    ByKey(|record: &Owned| record.key),
                    5,
                    3,
                    &|records| {
                        records.sort_by_key(|record| {
                            assert_ne!(Some(record.id), fail, "injected finishing panic");
                            record.key
                        })
                    },
                )
            });
            assert_eq!(result.is_err(), fail.is_some());
            if fail.is_none() {
                assert_eq!(
                    records
                        .iter()
                        .map(|record| (record.key, record.id))
                        .collect::<Vec<_>>(),
                    expected
                );
            }
            let mut ids: Vec<_> = records.iter().map(|record| record.id).collect();
            ids.sort_unstable();
            assert_eq!(ids, (0..len).collect::<Vec<_>>());
            assert!(drops.iter().all(|count| count.load(Ordering::Relaxed) == 0));
            drop(records);
            assert!(drops.iter().all(|count| count.load(Ordering::Relaxed) == 1));
        }
    }

    #[test]
    fn changing_keys_keep_borrows() {
        let mut values: Vec<_> = (0..97).collect();
        let mut records: Vec<_> = values.iter_mut().enumerate().collect();
        let calls = AtomicUsize::new(0);
        finish_records(
            &mut records,
            ByKey(|record: &(usize, &mut usize)| ((record.0 * 7919 + 73) % 256) as u32),
            5,
            3,
            &|records| {
                crate::stable_sort_by_key(records, |record| {
                    (calls.fetch_add(1, Ordering::Relaxed) ^ record.0) as u32
                });
            },
        );
        let mut ids = Vec::new();
        for (id, value) in records {
            assert_eq!(id, *value);
            *value += 1;
            ids.push(id);
        }
        ids.sort_unstable();
        assert_eq!(ids, (0..97).collect::<Vec<_>>());
        assert_eq!(values, (1..=97).collect::<Vec<_>>());
    }

    #[test]
    fn concurrent_borrows_keep_records() {
        // The second bucket is already staged; race relocation of the first
        // source against gathering it. std threads keep dependency-level
        // epoch reclamation out of Miri's provenance and leak checks.
        let mut first = core::cell::Cell::new(10usize);
        let mut second = core::cell::Cell::new(20usize);
        let mut records = vec![(1u32, &mut first), (0u32, &mut second)];
        let input = records.as_mut_ptr();
        let mut staged_second = MaybeUninit::uninit();
        // SAFETY: the second record transfers to private staging before its
        // original physical block is made free in the owner table.
        unsafe {
            ptr::copy_nonoverlapping(input.add(1), staged_second.as_mut_ptr(), 1);
        }
        type BorrowedRecord<'a> = (u32, &'a mut core::cell::Cell<usize>);
        let mut scratch: Vec<MaybeUninit<BorrowedRecord<'_>>> = Vec::with_capacity(2);
        scratch.resize_with(2, MaybeUninit::uninit);
        let scratch_pointer = scratch.as_mut_ptr().cast::<BorrowedRecord<'_>>();
        let storage = Storage {
            // SAFETY: both scratch cells and both original input records are
            // allocated, with every address derived from its own ancestor.
            addresses: unsafe {
                vec![scratch_pointer, scratch_pointer.add(1), input, input.add(1)]
            },
            input,
            full_blocks: 2,
            scratch_blocks: 2,
            block_size: 1,
            owners: [FREE, FREE, 0, FREE].into_iter().map(Mutex::new).collect(),
            locations: [2, 3].into_iter().map(AtomicUsize::new).collect(),
            lengths: vec![1, 1],
            free: vec![
                Mutex::new(Vec::with_capacity(1)),
                Mutex::new(vec![3]),
                Mutex::new(vec![0, 1]),
            ],
        };
        std::thread::scope(|scope| {
            let storage = &storage;
            scope.spawn(move || {
                let mut staged_first = MaybeUninit::uninit();
                let physical =
                    storage.gather(&Source { logical: 0, len: 1 }, staged_first.as_mut_ptr());
                let mut free = lock(&storage.free[0]);
                free.push(physical);
                storage.write(1, staged_first.as_ptr(), 1, &mut free, 0);
            });
            scope.spawn(move || {
                let mut free = lock(&storage.free[1]);
                storage.write(0, staged_second.as_ptr(), 1, &mut free, 1);
            });
        });
        drop(storage);
        drop(scratch);
        assert_eq!(
            records.iter().map(|record| record.0).collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(records[0].1.get(), 20);
        assert_eq!(records[1].1.get(), 10);
        for (_, value) in records {
            value.set(value.get() + 1);
        }
        assert_eq!((first.get(), second.get()), (11, 21));
    }

    #[test]
    fn suspended_caches_keep_progress() {
        // Eight staged even buckets coexist with eight unread odd buckets.
        // Helpers finalize their victims' free input blocks, then are paused
        // before publishing unused blocks. No key callback needs to panic.
        const SHARDS: usize = 8;
        const BUCKETS: usize = 16;
        const BLOCK: usize = 2;
        const SHARD_LEN: usize = 256;
        const BUCKET_LEN: usize = 128;
        let mut records = Vec::new();
        for shard in 0..SHARDS {
            let (first, second) = if shard < 4 {
                (8 + 2 * shard, 1 + 2 * shard)
            } else {
                (9 + 2 * (shard - 4), 2 * (shard - 4))
            };
            for bucket in [first, second] {
                for _ in 0..121 {
                    records.push((bucket as u32, records.len()));
                }
            }
            for bucket in 0..BUCKETS {
                if bucket != first && bucket != second {
                    records.push((bucket as u32, records.len()));
                }
            }
        }
        let mut expected = records.clone();
        expected.sort_by_key(|record| record.0);
        let input = records.as_mut_ptr();
        let mut buffers: Vec<_> = (0..SHARDS).map(|_| Buffers::new()).collect();
        let mut parts = Vec::new();
        for (shard, buffers) in buffers.iter_mut().enumerate() {
            // SAFETY: these disjoint shard borrows share one retained ancestor.
            let records =
                unsafe { core::slice::from_raw_parts_mut(input.add(shard * SHARD_LEN), SHARD_LEN) };
            let mut part = Partition::new(records, BUCKETS, BLOCK, buffers);
            assert!(part.step(|record| record.0 as usize).is_none());
            parts.push(part);
        }
        let scratch_blocks = SHARDS * 2 * BUCKETS;
        let full_blocks = records.len() / BLOCK;
        let count = scratch_blocks + full_blocks;
        let mut addresses = Vec::with_capacity(count);
        for part in &mut parts {
            for physical in 0..2 * BUCKETS {
                addresses.push(part.blocks.block_ptr(physical));
            }
        }
        addresses.extend((0..full_blocks).map(|block| {
            // SAFETY: all global input block addresses derive from the ancestor.
            unsafe { input.add(block * BLOCK) }
        }));
        let mut owners = vec![FREE; count];
        let mut locations = Vec::new();
        let mut lengths = Vec::new();
        let mut buckets = Vec::new();
        for bucket in 0..BUCKETS {
            let mut sources = Vec::new();
            for (shard, part) in parts.iter().enumerate() {
                let start = if bucket == 0 {
                    BUCKETS
                } else {
                    part.partial_blocks[bucket - 1].order_index + 1
                };
                let partial = &part.partial_blocks[bucket];
                for logical in start..=partial.order_index {
                    let len = if logical == partial.order_index {
                        partial.len
                    } else {
                        BLOCK
                    };
                    if len == 0 {
                        continue;
                    }
                    let local = part.block_order[logical] as usize;
                    let physical = if local < 2 * BUCKETS {
                        shard * 2 * BUCKETS + local
                    } else {
                        scratch_blocks + shard * (SHARD_LEN / BLOCK) + local - 2 * BUCKETS
                    };
                    let logical = locations.len();
                    sources.push(Source { logical, len });
                    locations.push(AtomicUsize::new(physical));
                    lengths.push(len);
                    owners[physical] = logical;
                }
            }
            assert_eq!(sources.len(), 68);
            buckets.push(Bucket {
                cache: bucket,
                sources,
                start: bucket * BUCKET_LEN,
                len: BUCKET_LEN,
                records: Vec::new(),
                staged: false,
                completed: false,
                panic: None,
            });
        }
        let mut free = Vec::with_capacity(count);
        free.extend(
            owners
                .iter()
                .enumerate()
                .filter_map(|(physical, &owner)| (owner == FREE).then_some(physical)),
        );
        assert_eq!(free.len(), 192);
        let mut caches: Vec<_> = buckets
            .iter()
            .map(|bucket| Mutex::new(Vec::with_capacity(bucket.sources.len())))
            .collect();
        caches.push(Mutex::new(free));
        let storage = Storage {
            addresses,
            input,
            full_blocks,
            scratch_blocks,
            block_size: BLOCK,
            owners: owners.into_iter().map(Mutex::new).collect(),
            locations,
            lengths,
            free: caches,
        };
        // Transfer recovery responsibility to this controlled state-machine
        // fixture; all records are plain Copy values and are restored below.
        for part in &mut parts {
            part.input_materialized = true;
        }
        for bucket in buckets.iter_mut().step_by(2) {
            bucket.records.resize_with(bucket.len, MaybeUninit::uninit);
            let mut offset = 0;
            for source in &bucket.sources {
                // SAFETY: staging covers the concatenated source prefixes.
                let target = unsafe { bucket.records.as_mut_ptr().add(offset).cast() };
                lock(&storage.free[bucket.cache]).push(storage.gather(source, target));
                offset += source.len;
            }
            bucket.staged = true;
        }
        for helper in [0, 2, 4, 6] {
            let bucket = &mut buckets[helper];
            storage.write(
                bucket.start,
                bucket.records.as_ptr().cast(),
                bucket.len,
                &mut lock(&storage.free[bucket.cache]),
                bucket.cache,
            );
            bucket.completed = true;
            bucket.records = Vec::new();
            // Deliberately pause before the completed helper returns.
        }
        let result = unwind::catch(|| {
            for victim in [8, 10, 12, 14] {
                let bucket = &mut buckets[victim];
                storage.write(
                    bucket.start,
                    bucket.records.as_ptr().cast(),
                    bucket.len,
                    &mut lock(&storage.free[bucket.cache]),
                    bucket.cache,
                );
                bucket.completed = true;
                bucket.records = Vec::new();
            }
        });
        assert!(lock(storage.free.last().unwrap()).is_empty());
        assert!(
            lock(&storage.free[0]).len() < 64,
            "victims must steal a helper block"
        );
        // Helpers stay paused. Their registered caches must support victims
        // before any helper returns to publish or release its allocation.
        for bucket in &mut buckets {
            bucket.finish(&storage, None);
        }
        drop(storage);
        drop(parts);
        assert_eq!(records, expected);
        assert!(
            result.is_ok(),
            "free blocks retained by suspended helpers must remain available"
        );
    }

    #[test]
    fn oversized_buckets_use_fallback() {
        let mut records: Vec<_> = (0..97)
            .map(|id| (((id * 7919 + 73) % 256) as u32, id))
            .collect();
        let mut expected = records.clone();
        expected.sort_by_key(|record| record.0);
        let mut visits = 0;
        partition_impl_with_visit(
            &mut records,
            3,
            5,
            ByKey(|record: &(u32, usize)| record.0),
            &mut Buffers::new(),
            Config {
                shards: 3,
                block_size: Some(5),
                parallel: !cfg!(miri),
                finish: Some(BoundedFinish {
                    limit: 0,
                    sort: &|_| panic!("oversized bucket entered bounded finishing"),
                }),
            },
            Some(&mut |records| {
                visits += 1;
                records.sort_by_key(|record| record.0);
            }),
        );
        assert!(visits > 0);
        assert_eq!(records, expected);
    }
}
