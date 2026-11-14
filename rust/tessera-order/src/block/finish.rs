// SPDX-License-Identifier: BSD-2-Clause

//! Direct final-band completion through one relocatable-source ledger.

#[cfg(feature = "rayon")]
use super::parallel::BucketVisit;
use super::*;
use crate::trace::{self, Event};
use core::ops::DerefMut;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

/// Supplies exclusive metadata access to the shared movement operation.
/// Serial completion uses `RefCell`; parallel completion uses poison-tolerant
/// mutexes. Neither guard is held while calling a bucket finisher.
///
/// # Safety
///
/// A guard must exclusively access the same stored value until it drops.
/// `exclusive` must access that value without bypassing any outstanding guard.
unsafe trait MetadataCell<Value> {
    /// Creates an unborrowed cell containing exactly this value.
    fn new(value: Value) -> Self;
    /// Acquires exclusive access; callers never reacquire a held cell.
    fn acquire(&self) -> impl DerefMut<Target = Value>;
    /// Attempts exclusive access without waiting.
    fn try_acquire(&self) -> Option<impl DerefMut<Target = Value>>;
    /// Accesses the value during uniquely owned construction.
    fn exclusive(&mut self) -> &mut Value;
}

// SAFETY: standard mutex guards serialize access and get_mut requires exclusivity.
unsafe impl<Value> MetadataCell<Value> for Mutex<Value> {
    fn new(value: Value) -> Self {
        Mutex::new(value)
    }
    fn acquire(&self) -> impl DerefMut<Target = Value> {
        self.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    fn try_acquire(&self) -> Option<impl DerefMut<Target = Value>> {
        match self.try_lock() {
            Ok(guard) => Some(guard),
            Err(std::sync::TryLockError::Poisoned(error)) => Some(error.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }
    fn exclusive(&mut self) -> &mut Value {
        self.get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

// SAFETY: RefCell guards enforce exclusive borrowing, and it is never Sync.
unsafe impl<Value> MetadataCell<Value> for core::cell::RefCell<Value> {
    fn new(value: Value) -> Self {
        core::cell::RefCell::new(value)
    }
    fn acquire(&self) -> impl DerefMut<Target = Value> {
        self.borrow_mut()
    }
    fn try_acquire(&self) -> Option<impl DerefMut<Target = Value>> {
        self.try_borrow_mut().ok()
    }
    fn exclusive(&mut self) -> &mut Value {
        self.get_mut()
    }
}

/// A physical block without a live source or finalized band fragment.
const FREE: usize = usize::MAX;
/// A physical block with no unread source. Disjoint final-band fragments may
/// still be unwritten; FINAL does not mean the whole block is filled.
const FINAL: usize = usize::MAX - 1;
/// One initialized source prefix from a bucket chain.
struct Source {
    location: AtomicUsize,
    len: usize,
}

/// A bucket's pending source prefixes and final input range.
struct Bucket {
    pending_sources: core::ops::Range<usize>,
    start: usize,
    len: usize,
    /// The bucket index, which also selects the bucket's free-block cache.
    cache: usize,
}

/// Owns restoration after partition guards hand off their sources. Structured
/// parallel execution joins before this guard can drop, so recovery can finish
/// every unpublished band without invoking a callback.
struct Completion<T, Owner: MetadataCell<usize>, Free: MetadataCell<Vec<usize>>> {
    storage: Storage<T, Owner, Free>,
    buckets: Vec<Bucket>,
    ends: Vec<usize>,
}

impl<T, Owner: MetadataCell<usize>, Free: MetadataCell<Vec<usize>>> Drop
    for Completion<T, Owner, Free>
{
    fn drop(&mut self) {
        for bucket in &mut self.buckets {
            bucket.finish(&self.storage, None::<fn(usize, Range<usize>, &mut [T])>);
        }
    }
}

/// Per-block metadata capacity that coexists with record scratch. Bucket and
/// shard descriptors add O(Rq) separately. Free-cache capacities sum to at most
/// the physical-block count plus one slot per bucket.
const fn descriptor_bytes<Owner>() -> usize {
    core::mem::size_of::<u32>()
        + core::mem::size_of::<*mut ()>()
        + core::mem::size_of::<Owner>()
        + core::mem::size_of::<Source>()
        + core::mem::size_of::<usize>()
}

/// Live metadata cost per full input block for exclusive completion.
pub(super) const fn serial_descriptor_bytes() -> usize {
    descriptor_bytes::<core::cell::RefCell<usize>>()
}

/// Live metadata cost per full input block for sharded completion.
#[cfg(feature = "rayon")]
pub(super) const fn parallel_descriptor_bytes() -> usize {
    descriptor_bytes::<Mutex<usize>>()
}

/// Physical owners, relocatable logical sources and free block candidates.
struct Storage<T, Owner = Mutex<usize>, Free = Mutex<Vec<usize>>> {
    addresses: Vec<*mut T>,
    input: *mut T,
    full_blocks: usize,
    scratch_blocks: usize,
    block_size: usize,
    owners: Vec<Owner>,
    sources: Vec<Source>,
    free: Vec<Free>,
}

// SAFETY: no T references are shared. A physical owner mutex protects source
// relocation and gathering; final writes cover disjoint fixed record bands.
unsafe impl<T: Send, Owner: Sync, Free: Sync> Sync for Storage<T, Owner, Free> {}

impl<T, Owner: MetadataCell<usize>, Free: MetadataCell<Vec<usize>>> Storage<T, Owner, Free> {
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
                let Some(mut free) = self.free[peer].try_acquire() else {
                    continue;
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

    /// Transfers one complete live prefix to its final input range, allowing
    /// overlap with that same source. Retires or recycles its physical block
    /// and returns the number of records written.
    fn transfer(
        &self,
        logical: usize,
        start: usize,
        local: &mut Vec<usize>,
        cache: usize,
    ) -> usize {
        let source = &self.sources[logical];
        let initial = source.location.load(Ordering::Acquire);
        let skipped = self.protect_band(start, source.len, initial, local, cache);
        loop {
            let physical = source.location.load(Ordering::Acquire);
            let mut owner = lock(&self.owners[physical]);
            if *owner != logical {
                continue;
            }
            // SAFETY: the final interval fits the input. Every overlapping
            // destination block is FINAL except an explicitly skipped source.
            let output = unsafe { self.input.add(start) };
            let overlapped = skipped && physical == initial;
            trace::emit(|| Event::Transfer {
                source: logical,
                physical,
                start,
                overlapped,
            });
            if overlapped {
                // SAFETY: this owner still protects the skipped live prefix.
                // Its only overlap is with its own final range; memmove
                // transfers ownership before that block becomes FINAL.
                unsafe { ptr::copy(self.addresses[physical], output, source.len) };
                *owner = FINAL;
            } else {
                // If the skipped source moved, acquiring its verified new
                // owner also observes the relocation making its old block
                // FINAL. The actual source is outside the protected output.
                // SAFETY: the verified owner protects a live prefix disjoint
                // from the whole final interval, even after relocation.
                unsafe { ptr::copy_nonoverlapping(self.addresses[physical], output, source.len) };
                *owner = FREE;
                local.push(physical);
            }
            return source.len;
        }
    }

    /// Evacuates an occupied destination using locks in physical ID order.
    fn protect(&self, physical: usize, local: &mut Vec<usize>, cache: usize) {
        loop {
            let mut owner = lock(&self.owners[physical]);
            if *owner == FINAL || *owner == FREE {
                *owner = FINAL;
                trace::emit(|| Event::Final { physical });
                return;
            }
            drop(owner);
            let free = self.take_free(cache, local);
            if free == physical {
                // A free-list entry can become stale only after finalization.
                let mut owner = lock(&self.owners[physical]);
                assert!(*owner == FREE || *owner == FINAL);
                *owner = FINAL;
                trace::emit(|| Event::Final { physical });
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
                trace::emit(|| Event::Relocate {
                    source: logical,
                    from: physical,
                    to: free,
                });
            } else {
                local.push(free);
                trace::emit(|| Event::Final { physical });
            }
            *owner = FINAL;
            return;
        }
    }

    /// Protects the final range except `source`, returning whether that
    /// physical block actually overlapped and was skipped.
    fn protect_band(
        &self,
        start: usize,
        len: usize,
        source: usize,
        local: &mut Vec<usize>,
        cache: usize,
    ) -> bool {
        let mut skipped = false;
        let mut offset = 0;
        while offset < len {
            let position = start + offset;
            let block = position / self.block_size;
            let begin = position % self.block_size;
            let count = (self.block_size - begin).min(len - offset);
            if block < self.full_blocks {
                let physical = self.scratch_blocks + block;
                if physical == source {
                    skipped = true;
                } else {
                    self.protect(physical, local, cache);
                }
            }
            offset += count;
        }
        skipped
    }
}

/// Acquires an internal metadata lock, never held across a user callback.
fn lock<Value>(cell: &impl MetadataCell<Value>) -> impl DerefMut<Target = Value> {
    cell.acquire()
}

impl Bucket {
    /// Streams a band without a bucket-sized buffer, then publishes its borrow.
    fn finish<'records, T: 'records, Owner, Free, Visit>(
        &mut self,
        storage: &Storage<T, Owner, Free>,
        visit: Option<Visit>,
    ) where
        Owner: MetadataCell<usize>,
        Free: MetadataCell<Vec<usize>>,
        Visit: FnOnce(usize, Range<usize>, &'records mut [T]),
    {
        if self.pending_sources.is_empty() {
            return;
        }
        // No allocation, user code or Rayon reentry interrupts movement, so
        // unwind recovery sees whole buckets. Transfer protects output and then
        // validates the source, which peers may have relocated. After g retired
        // prefixes a bucket touches at most g + 2 destination blocks. While it
        // waits for one of those blocks, at most g + 1 are final: with at least
        // two shards, the shared reserve still covers outstanding candidates.
        let mut offset = 0;
        for logical in self.pending_sources.clone() {
            let mut free = lock(&storage.free[self.cache]);
            offset += storage.transfer(logical, self.start + offset, &mut free, self.cache);
        }
        // Recovery must not revisit this band if its callback retains a borrow
        // or unwinds. Completion restores the other bands after all jobs join.
        self.pending_sources.start = self.pending_sources.end;
        trace::emit(|| Event::Band {
            bucket: self.cache,
            start: self.start,
            len: self.len,
        });
        if let Some(visit) = visit {
            // SAFETY: all this band's sources are retired and every physical
            // destination is FINAL. Peers cannot read it and write only their
            // disjoint bands, even if they share a physical block fragment.
            let band =
                unsafe { core::slice::from_raw_parts_mut(storage.input.add(self.start), self.len) };
            visit(self.cache, self.start..self.start + self.len, band);
        }
    }
}

/// Builds the source ledger and takes restoration responsibility only after
/// every metadata table and bucket boundary exists.
///
/// # Safety
///
/// Partitions are in input order, own their recovery and completed one scatter
/// with the same block size and fan-out. They cover `input_len` consecutive
/// records from the common raw ancestor `input`; only the last may have a tail.
/// Their scratch allocations remain alive until this completion is dropped.
unsafe fn build_completion<T, Owner: MetadataCell<usize>, Free: MetadataCell<Vec<usize>>>(
    input: *mut T,
    input_len: usize,
    partitions: &mut [Partition<'_, '_, T>],
) -> Completion<T, Owner, Free> {
    let block_size = partitions[0].block_size;
    let local_scratch = partitions[0].scratch_blocks;
    let bucket_count = partitions[0].buffers.writers.len();
    let scratch_blocks = partitions.len() * local_scratch;
    let full_blocks = input_len / block_size;
    let count = scratch_blocks + full_blocks;
    let mut addresses = Vec::with_capacity(count);
    for partition in partitions.iter_mut() {
        addresses.extend((0..local_scratch).map(|physical| partition.block_ptr(physical)));
    }
    // SAFETY: the caller guarantees these full blocks at the common ancestor.
    addresses.extend((0..full_blocks).map(|block| unsafe { input.add(block * block_size) }));
    let mut owners: Vec<Owner> = (0..count).map(|_| Owner::new(FREE)).collect();
    // A shard adds at most one partial prefix per bucket to its full blocks.
    // Reserve the bound now, avoiding source-table growth and overlap.
    let mut sources = Vec::with_capacity(full_blocks + partitions.len() * bucket_count);
    let mut buckets = Vec::with_capacity(bucket_count);
    let mut start = 0;
    for bucket in 0..bucket_count {
        let first_source = sources.len();
        let mut len = 0;
        for (index, partition) in partitions.iter().enumerate() {
            // SAFETY: partition inputs are block-aligned subranges of input.
            let first_block = unsafe { partition.input.offset_from(input) } as usize / block_size;
            for (local, prefix) in partition.bucket_blocks(bucket) {
                if prefix == 0 {
                    continue;
                }
                let physical = if local < local_scratch {
                    index * local_scratch + local
                } else {
                    scratch_blocks + first_block + local - local_scratch
                };
                *owners[physical].exclusive() = sources.len();
                trace::emit(|| Event::Source {
                    bucket,
                    physical,
                    len: prefix,
                });
                sources.push(Source {
                    location: AtomicUsize::new(physical),
                    len: prefix,
                });
                len += prefix;
            }
        }
        buckets.push(Bucket {
            cache: bucket,
            pending_sources: first_source..sources.len(),
            start,
            len,
        });
        start += len;
    }
    assert_eq!(start, input_len);
    let mut free = Vec::with_capacity(count - sources.len());
    for (physical, owner) in owners.iter_mut().enumerate() {
        if *owner.exclusive() == FREE {
            free.push(physical);
        }
    }
    let mut caches = Vec::with_capacity(bucket_count + 1);
    caches.extend(buckets.iter().map(|bucket| {
        // Empty buckets perform no transfers and need no free cache.
        let capacity = if bucket.pending_sources.is_empty() {
            0
        } else {
            bucket.pending_sources.len() + 1
        };
        Free::new(Vec::with_capacity(capacity))
    }));
    caches.push(Free::new(free));
    let ends = buckets
        .iter()
        .map(|bucket| bucket.start + bucket.len)
        .collect();
    let completion = Completion {
        storage: Storage {
            addresses,
            input,
            full_blocks,
            scratch_blocks,
            block_size,
            owners,
            sources,
            free: caches,
        },
        buckets,
        ends,
    };
    // No fallible step separates this complete recovery guard from retiring
    // the old guards. Scratch remains owned by those partitions until return.
    for partition in partitions {
        partition.owns_recovery = false;
    }
    completion
}

/// Finishes serial bands with exclusive metadata and a mutable callback.
/// The completion guard restores every unfinished band before a panic escapes.
pub(super) fn complete_serial<'records, T>(
    partition: &mut Partition<'records, '_, T>,
    mut visit: Option<Visit<'_, 'records, T>>,
) -> Vec<usize> {
    assert!(partition.owns_recovery, "partition already completed");
    let input = partition.input;
    let len = partition.input_len;
    // SAFETY: the single guard owns this complete scatter and its scratch.
    let mut completion = unsafe {
        build_completion::<T, core::cell::RefCell<usize>, core::cell::RefCell<Vec<usize>>>(
            input,
            len,
            core::slice::from_mut(partition),
        )
    };
    for bucket in &mut completion.buckets {
        bucket.finish(&completion.storage, visit.as_deref_mut());
    }
    core::mem::take(&mut completion.ends)
}

/// Completes all scattered partitions in the caller's pool and lends final
/// bands. The completion guard restores pending bands after all jobs join.
///
/// # Safety
///
/// The [`build_completion`] requirements hold.
#[cfg(feature = "rayon")]
pub(super) unsafe fn complete<'records, T: Send>(
    input: *mut T,
    input_len: usize,
    partitions: &mut [Partition<'records, '_, T>],
    parallel: bool,
    visit: Option<BucketVisit<'_, 'records, T>>,
) -> Vec<usize> {
    // SAFETY: forwarded from the caller.
    unsafe { complete_with_fault(input, input_len, partitions, parallel, visit, None) }
}

/// Completes with an injected scheduler failure between bucket jobs.
///
/// # Safety
///
/// The [`build_completion`] requirements hold.
#[cfg(feature = "rayon")]
unsafe fn complete_with_fault<'records, T: Send>(
    input: *mut T,
    input_len: usize,
    partitions: &mut [Partition<'records, '_, T>],
    parallel: bool,
    visit: Option<BucketVisit<'_, 'records, T>>,
    fail_after: Option<usize>,
) -> Vec<usize> {
    use rayon::prelude::*;
    // SAFETY: forwarded from the caller.
    let mut completion = unsafe {
        build_completion::<T, Mutex<usize>, Mutex<Vec<usize>>>(input, input_len, partitions)
    };
    let completed = AtomicUsize::new(0);
    let run = |bucket: &mut Bucket| {
        bucket.finish(&completion.storage, visit);
        if let Some(fail_after) = fail_after {
            let position = completed.fetch_add(1, Ordering::Relaxed);
            assert_ne!(
                position, fail_after,
                "injected scheduler panic between bucket tasks"
            );
        }
    };
    if parallel {
        completion.buckets.par_iter_mut().for_each(run);
    } else {
        completion.buckets.iter_mut().for_each(run);
    }
    core::mem::take(&mut completion.ends)
}

#[cfg(all(test, feature = "rayon"))]
mod tests {
    use super::super::parallel::{Config, partition_impl_with_visit};
    use super::*;
    use crate::key::{ByKey, Extract, KeyDigit};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn transfers_recycle_only_sources_outside_the_final_range() {
        for (physical, start, len, expected_owner) in [(2, 0, 4, FREE), (1, 1, 3, FINAL)] {
            let mut input = [10u32, 11, 12, 13, 20, 21, 22, 23];
            let mut scratch = [0u32; 4];
            let input_ptr = input.as_mut_ptr();
            let expected = input[(physical - 1) * 4..(physical - 1) * 4 + len].to_vec();
            let mut owners = vec![Mutex::new(FREE), Mutex::new(FREE), Mutex::new(FREE)];
            *owners[physical].get_mut().unwrap() = 0;
            let storage = Storage {
                // SAFETY: both input blocks and the disjoint scratch hold four
                // records and stay alive until this complete transfer returns.
                addresses: vec![scratch.as_mut_ptr(), input_ptr, unsafe { input_ptr.add(4) }],
                input: input_ptr,
                full_blocks: 2,
                scratch_blocks: 1,
                block_size: 4,
                owners,
                sources: vec![Source {
                    location: AtomicUsize::new(physical),
                    len,
                }],
                free: vec![Mutex::new(Vec::new()), Mutex::new(vec![0])],
            };
            let mut free = Vec::new();
            assert_eq!(storage.transfer(0, start, &mut free, 0), len);
            assert_eq!(&input[start..start + len], expected);
            assert_eq!(*lock(&storage.owners[physical]), expected_owner);
            assert_eq!(free.contains(&physical), expected_owner == FREE);
        }
    }

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
            KeyDigit::new(key, 5),
            &mut Buffers::new(),
            Config {
                shards,
                block_size: Some(block_size),
                parallel: !cfg!(miri),
            },
            Some(&|_, _, band| finish(band)),
        );
    }

    #[test]
    fn sparse_buckets_keep_directory_ends() {
        for shards in [1, 2, 5] {
            let mut records: Vec<_> = (0..97u32)
                .map(|id| (32 + (id % 2) * 128 + id % 16, id.to_string()))
                .collect();
            let mut expected = records.clone();
            expected.sort_by_key(|record| record.0);
            // Only buckets 1 and 5 are populated. Derive the expected ranges
            // independently of ledger construction and callback arrival order.
            let mut ends = Vec::new();
            let mut end = 0;
            for bucket in 0..8 {
                end += records
                    .iter()
                    .filter(|record| record.0 >> 5 == bucket)
                    .count();
                ends.push(end);
            }
            let seen = Mutex::new([false; 8]);
            let actual_ends = partition_impl_with_visit(
                &mut records,
                3,
                KeyDigit::new(ByKey(|record: &(u32, String)| record.0), 5),
                &mut Buffers::new(),
                Config {
                    shards,
                    block_size: Some(5),
                    parallel: !cfg!(miri),
                },
                Some(&|bucket, range, band| {
                    let start = if bucket == 0 { 0 } else { ends[bucket - 1] };
                    assert_eq!(range, start..ends[bucket]);
                    assert_eq!(range.len(), band.len());
                    assert!(!band.is_empty());
                    assert!(!core::mem::replace(&mut lock(&seen)[bucket], true));
                    band.sort_by_key(|record| record.0);
                    assert_eq!(band, &expected[range]);
                }),
            );
            assert_eq!(actual_ends, ends);
            assert_eq!(
                *lock(&seen),
                [false, true, false, false, false, true, false, false]
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
                crate::sort_by_key(records, |record| {
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
            pending_sources: bucket..bucket + 1,
            start: 1 - bucket,
            len: 1,
            cache: bucket,
        });
        std::thread::scope(|scope| {
            for bucket in &mut buckets {
                let storage = &storage;
                scope.spawn(move || {
                    bucket.finish(
                        storage,
                        Some(
                            |bucket, range: Range<usize>, band: &mut [BorrowedRecord<'_>]| {
                                assert_eq!(range, 1 - bucket..2 - bucket);
                                assert_eq!(band.len(), range.len());
                            },
                        ),
                    );
                });
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
        let mut shards = Vec::new();
        for (shard, buffers) in buffers.iter_mut().enumerate() {
            // SAFETY: these disjoint shard borrows share one retained ancestor.
            let records =
                unsafe { core::slice::from_raw_parts_mut(input.add(shard * SHARD_LEN), SHARD_LEN) };
            let mut sorter = Partition::new(records, BUCKETS, BLOCK, buffers);
            let _ = sorter.step(|record: &(u32, usize)| record.0 as usize);
            shards.push(sorter);
        }
        // SAFETY: these partitions cover the records in order from their ancestor.
        let mut completion = unsafe {
            build_completion::<_, Mutex<usize>, Mutex<Vec<usize>>>(
                input,
                SHARDS * SHARD_LEN,
                &mut shards,
            )
        };
        let storage = &completion.storage;
        let buckets = &mut completion.buckets;
        assert!(
            buckets
                .iter()
                .all(|bucket| bucket.pending_sources.len() == 68 && bucket.len == BUCKET_LEN)
        );
        assert_eq!(lock(&storage.free[BUCKETS]).len(), 192);
        // Transfer recovery responsibility to this controlled state-machine
        // fixture; all records are plain Copy values and are restored below.
        for helper in [0, 2, 4, 6] {
            buckets[helper].finish(storage, None::<fn(usize, Range<usize>, &mut [_])>);
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
                buckets[victim].finish(storage, None::<fn(usize, Range<usize>, &mut [_])>);
            }
        });
        assert!(
            lock(&storage.free[0]).len() < held,
            "victims must steal a helper block"
        );
        // Helpers stay paused. Their registered caches must support victims
        // before any helper returns to publish or release its allocation.
        for bucket in buckets {
            bucket.finish(storage, None::<fn(usize, Range<usize>, &mut [_])>);
        }
        drop(completion);
        drop(shards);
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
                        KeyDigit::new(ByKey(|record: &(u32, usize, &mut Cell<usize>)| record.0), 5),
                        &mut Buffers::new(),
                        Config {
                            shards,
                            block_size: Some(block_size),
                            parallel: !cfg!(miri),
                        },
                        Some(&|_, _, band| {
                            if !cfg!(miri) {
                                let (a, b) = rayon::join(|| 13 * 7, || 13 * 7);
                                assert_eq!(a, b);
                            }
                            crate::sort_by_key(band, |record| {
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
                let input = records.as_mut_ptr();
                // SAFETY: the shard borrows all records from their ancestor.
                let records = unsafe { core::slice::from_raw_parts_mut(input, len) };
                let mut sorter = Partition::new(records, 4, 3, &mut buffers);
                let _ = sorter.step(|record: &Owned| record.id * 37 % 4);
                let mut shards = [sorter];
                // SAFETY: one shard covers all records from their ancestor.
                unsafe { complete_with_fault(input, len, &mut shards, parallel, None, Some(0)) };
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
