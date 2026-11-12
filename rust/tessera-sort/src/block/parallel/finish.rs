// SPDX-License-Identifier: BSD-2-Clause

//! Direct final-band completion through one relocatable-source ledger.

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
    location: AtomicUsize,
    len: usize,
}

/// A bucket's source prefixes and final input range.
struct Bucket {
    sources: core::ops::Range<usize>,
    start: usize,
    len: usize,
    cache: usize,
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
    sources: Vec<Source>,
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

    /// Transfers a live prefix to exclusively owned, non-overlapping output.
    fn gather(&self, logical: usize, output: *mut T) -> usize {
        let source = &self.sources[logical];
        loop {
            let physical = source.location.load(Ordering::Acquire);
            let mut owner = lock(&self.owners[physical]);
            if *owner != logical {
                continue;
            }
            // SAFETY: the verified owner mutex protects this initialized
            // source prefix until ownership transfers to exclusive output.
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
                        self.sources[logical].len,
                    );
                }
                *free_owner = logical;
                self.sources[logical]
                    .location
                    .store(free, Ordering::Release);
            } else {
                local.push(free);
            }
            *owner = FINAL;
            return;
        }
    }

    /// Retires unread sources from every physical block touching a final band.
    fn protect_band(&self, start: usize, len: usize, local: &mut Vec<usize>, cache: usize) {
        let mut offset = 0;
        while offset < len {
            let position = start + offset;
            let block = position / self.block_size;
            let begin = position % self.block_size;
            let count = (self.block_size - begin).min(len - offset);
            if block < self.full_blocks {
                self.protect(self.scratch_blocks + block, local, cache);
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

impl Bucket {
    /// Streams a band without a bucket-sized buffer, then publishes its borrow.
    fn finish<'records, T>(
        &mut self,
        storage: &Storage<T>,
        visit: Option<BucketVisit<'_, 'records, T>>,
    ) {
        if self.completed {
            return;
        }
        // No allocation, user code or scheduler call interrupts movement, so
        // unwind recovery sees whole buckets. Protect before loading the source
        // location: protection may relocate that very source. After g retired
        // prefixes a bucket touches at most g + 2 destination blocks. While it
        // waits for one of those blocks, at most g + 1 are final: with at least
        // two shards, the shared reserve still covers outstanding candidates.
        let mut offset = 0;
        for logical in self.sources.clone() {
            let len = storage.sources[logical].len;
            let mut free = lock(&storage.free[self.cache]);
            storage.protect_band(self.start + offset, len, &mut free, self.cache);
            // SAFETY: the whole destination is final and therefore contains no
            // unread source. gather validates and locks the current source,
            // which is disjoint even if protection just relocated it.
            let output = unsafe { storage.input.add(self.start + offset) };
            let physical = storage.gather(logical, output);
            free.push(physical);
            offset += len;
        }
        self.completed = true;
        if self.len != 0
            && let Some(visit) = visit
        {
            // SAFETY: all this band's sources are retired and every physical
            // destination is FINAL. Peers cannot read it and write only their
            // disjoint bands, even if they share a physical block fragment.
            let band =
                unsafe { core::slice::from_raw_parts_mut(storage.input.add(self.start), self.len) };
            self.panic = unwind::catch(|| visit(band)).err();
        }
    }
}

/// Completes every bucket through the protected-source ledger, lending each
/// final band as soon as it no longer has live copy sources.
pub(super) fn complete<'records, T: Send>(
    sorter: &mut Partition<'records, '_, T>,
    ends: &[usize],
    parallel: bool,
    visit: Option<BucketVisit<'_, 'records, T>>,
) {
    complete_with_fault(sorter, ends, parallel, visit, None);
}

fn complete_with_fault<'records, T: Send>(
    sorter: &mut Partition<'records, '_, T>,
    ends: &[usize],
    parallel: bool,
    visit: Option<BucketVisit<'_, 'records, T>>,
    fail_after: Option<usize>,
) {
    let block_size = sorter.blocks.block_size;
    let count = sorter.buffers.block_order.len();
    let head = sorter.active_blocks.start;
    let input_len = sorter.blocks.input_len;
    let mut owners = vec![FREE; count];
    // One source entry owns both its live prefix length and relocatable
    // position. Buckets retain ranges, not another copy of that directory.
    let mut sources = Vec::with_capacity(sorter.active_blocks.len());
    let mut buckets = Vec::with_capacity(ends.len());
    let mut logical = head;
    let mut partial = 0;
    let mut start = 0;
    for (bucket, &end) in ends.iter().enumerate() {
        let logical_end = sorter.buffers.counts[bucket];
        let source_start = sources.len();
        while logical < logical_end {
            let len = sorter.next_block_len(logical, &mut partial);
            if len != 0 {
                let physical = sorter.buffers.block_order[logical] as usize;
                let source = sources.len();
                sources.push(Source {
                    location: AtomicUsize::new(physical),
                    len,
                });
                owners[physical] = source;
            }
            logical += 1;
        }
        buckets.push(Bucket {
            cache: bucket,
            sources: source_start..sources.len(),
            start,
            len: end - start,
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
        // A direct transfer can return a borrowed free candidate before it
        // retires its source, so reserve one slot beyond the own-source count.
        .map(|bucket| Mutex::new(Vec::with_capacity(bucket.sources.len() + 1)))
        .collect();
    caches.push(Mutex::new(free));
    let storage = Storage {
        addresses: (0..count)
            .map(|physical| sorter.blocks.block_ptr(physical))
            .collect(),
        input: sorter.blocks.input,
        full_blocks: input_len / block_size,
        scratch_blocks: sorter.blocks.scratch_blocks,
        block_size,
        owners: owners.into_iter().map(Mutex::new).collect(),
        sources,
        free: caches,
    };
    let completed = AtomicUsize::new(0);
    let run = |bucket: &mut Bucket| {
        bucket.finish(&storage, visit);
        if let Some(fail_after) = fail_after {
            let position = completed.fetch_add(1, Ordering::Relaxed);
            assert_ne!(
                position, fail_after,
                "injected scheduler panic between bucket tasks"
            );
        }
    };
    let result = unwind::catch(|| {
        if parallel {
            buckets.par_iter_mut().for_each(run);
        } else {
            buckets.iter_mut().for_each(run);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::ByKey;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    /// Exercises final-band finishing with small storage geometries.
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
            },
            Some(finish),
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
        // Race protection of each destination against gathering its source.
        // std threads keep dependency-level epoch reclamation out of Miri.
        let mut first = core::cell::Cell::new(10usize);
        let mut second = core::cell::Cell::new(20usize);
        let mut records = vec![(1u32, &mut first), (0u32, &mut second)];
        let input = records.as_mut_ptr();
        type BorrowedRecord<'a> = (u32, &'a mut core::cell::Cell<usize>);
        let mut scratch: Vec<MaybeUninit<BorrowedRecord<'_>>> = Vec::with_capacity(2);
        scratch.resize_with(2, MaybeUninit::uninit);
        let scratch_pointer = scratch.as_mut_ptr().cast::<BorrowedRecord<'_>>();
        let storage = Storage {
            // SAFETY: all addresses derive from their allocation's ancestor.
            addresses: unsafe {
                vec![scratch_pointer, scratch_pointer.add(1), input, input.add(1)]
            },
            input,
            full_blocks: 2,
            scratch_blocks: 2,
            block_size: 1,
            owners: [FREE, FREE, 0, 1].into_iter().map(Mutex::new).collect(),
            sources: [2, 3]
                .into_iter()
                .map(|physical| Source {
                    location: AtomicUsize::new(physical),
                    len: 1,
                })
                .collect(),
            free: vec![
                Mutex::new(Vec::with_capacity(2)),
                Mutex::new(Vec::with_capacity(2)),
                Mutex::new(vec![0, 1]),
            ],
        };
        let mut buckets = [0, 1].map(|bucket| Bucket {
            sources: bucket..bucket + 1,
            start: 1 - bucket,
            len: 1,
            cache: bucket,
            completed: false,
            panic: None,
        });
        std::thread::scope(|scope| {
            for bucket in &mut buckets {
                let storage = &storage;
                scope.spawn(move || bucket.finish(storage, None));
            }
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
        // Completed helpers retain registered free caches while suspended.
        // Later buckets must use those candidates without waiting for helpers
        // to return. No key callback needs to panic.
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
            assert!(part.step((), |_, record| record.0 as usize).is_ok());
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
        let mut sources = Vec::new();
        let mut buckets = Vec::new();
        for bucket in 0..BUCKETS {
            let source_start = sources.len();
            for (shard, part) in parts.iter().enumerate() {
                let start = if bucket == 0 {
                    BUCKETS
                } else {
                    part.buffers.partial_blocks[bucket - 1].order_index + 1
                };
                let partial = &part.buffers.partial_blocks[bucket];
                for logical in start..=partial.order_index {
                    let len = if logical == partial.order_index {
                        partial.len
                    } else {
                        BLOCK
                    };
                    if len == 0 {
                        continue;
                    }
                    let local = part.buffers.block_order[logical] as usize;
                    let physical = if local < 2 * BUCKETS {
                        shard * 2 * BUCKETS + local
                    } else {
                        scratch_blocks + shard * (SHARD_LEN / BLOCK) + local - 2 * BUCKETS
                    };
                    let logical = sources.len();
                    sources.push(Source {
                        location: AtomicUsize::new(physical),
                        len,
                    });
                    owners[physical] = logical;
                }
            }
            assert_eq!(sources.len() - source_start, 68);
            buckets.push(Bucket {
                cache: bucket,
                sources: source_start..sources.len(),
                start: bucket * BUCKET_LEN,
                len: BUCKET_LEN,
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
            sources,
            free: caches,
        };
        // Transfer recovery responsibility to this controlled state-machine
        // fixture; all records are plain Copy values and are restored below.
        for part in &mut parts {
            part.input_materialized = true;
        }
        for helper in [0, 2, 4, 6] {
            buckets[helper].finish(&storage, None);
        }
        // Concentrate unused candidates in a completed helper's registered
        // cache, forcing subsequent work to discover a suspended peer's blocks.
        for cache in 1..storage.free.len() {
            let mut free = lock(&storage.free[cache]);
            lock(&storage.free[0]).append(&mut free);
        }
        let held = lock(&storage.free[0]).len();
        let result = unwind::catch(|| {
            for victim in [8, 10, 12, 14] {
                buckets[victim].finish(&storage, None);
            }
        });
        assert!(
            lock(&storage.free[0]).len() < held,
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
    fn streamed_borrows_reenter_and_recover() {
        use core::cell::Cell;
        for (shards, block_size) in [(2, 1), (2, 3), (2, 8), (3, 1), (3, 3), (3, 8)] {
            for fail in [None, Some(0usize), Some(31)] {
                let len = if cfg!(miri) { 37 } else { 97 };
                let mut backing: Vec<_> = (0..len).map(Cell::new).collect();
                let mut records: Vec<_> = backing
                    .iter_mut()
                    .enumerate()
                    .map(|(id, value)| (((id * 7919 + 73) % 256) as u32, id, value))
                    .collect();
                let result = unwind::catch(|| {
                    partition_impl_with_visit(
                        &mut records,
                        3,
                        5,
                        ByKey(|record: &(u32, usize, &mut Cell<usize>)| record.0),
                        &mut Buffers::new(),
                        Config {
                            shards,
                            block_size: Some(block_size),
                            parallel: !cfg!(miri),
                        },
                        Some(&|band| {
                            if !cfg!(miri) {
                                let (a, b) = rayon::join(|| 13 * 7, || 13 * 7);
                                assert_eq!(a, b);
                            }
                            crate::stable_sort_by_key(band, |record| {
                                assert_ne!(Some(record.1), fail, "streamed callback panic");
                                record.0
                            });
                        }),
                    )
                });
                assert_eq!(result.is_err(), fail.is_some());
                if fail.is_none() {
                    assert!(
                        records
                            .windows(2)
                            .all(|pair| (pair[0].0, pair[0].1) <= (pair[1].0, pair[1].1))
                    );
                }
                let mut ids = Vec::new();
                for (_, id, value) in records {
                    assert_eq!(id, value.get());
                    value.set(id + 1);
                    ids.push(id);
                }
                ids.sort_unstable();
                assert_eq!(ids, (0..len).collect::<Vec<_>>());
                assert_eq!(
                    backing.iter().map(Cell::get).collect::<Vec<_>>(),
                    (1..=len).collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn scheduler_panic_restores_owners() {
        struct Owned {
            id: usize,
            drops: Arc<Vec<AtomicUsize>>,
            text: String,
        }
        impl Drop for Owned {
            fn drop(&mut self) {
                assert_eq!(self.text, self.id.to_string());
                self.drops[self.id].fetch_add(1, Ordering::Relaxed);
            }
        }
        let len = if cfg!(miri) { 23 } else { 113 };
        for parallel in [false, !cfg!(miri)] {
            let drops = Arc::new((0..len).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
            let mut records: Vec<_> = (0..len)
                .map(|id| Owned {
                    id,
                    drops: drops.clone(),
                    text: id.to_string(),
                })
                .collect();
            let result = unwind::catch(|| {
                let mut buffers = Buffers::new();
                let mut partition = Partition::new(&mut records, 4, 3, &mut buffers);
                assert!(partition.step((), |_, record| record.id * 37 % 4).is_ok());
                let ends = partition.record_ends();
                complete_with_fault(&mut partition, &ends, parallel, None, Some(0));
            });
            assert!(result.is_err());
            let mut ids: Vec<_> = records.iter().map(|record| record.id).collect();
            ids.sort_unstable();
            assert_eq!(ids, (0..len).collect::<Vec<_>>());
            assert!(drops.iter().all(|count| count.load(Ordering::Relaxed) == 0));
            drop(records);
            assert!(drops.iter().all(|count| count.load(Ordering::Relaxed) == 1));
        }
    }
}
