// SPDX-License-Identifier: BSD-2-Clause

//! Stable root scatter over independently owned block shards.

use super::*;
use rayon::prelude::*;

mod finish;

/// Publishes an exclusive final band, independently of other bucket movement.
type BucketVisit<'callback, 'records, T> = &'callback (dyn Fn(&'records mut [T]) + Sync);

struct Shard<'a, 'w, T> {
    sorter: Partition<'a, 'w, T>,
    panic: Option<unwind::Payload>,
    first_block: usize,
}

struct Config {
    shards: usize,
    block_size: Option<usize>,
    parallel: bool,
}

// SAFETY: each shard exclusively borrows a disjoint input slice and Buffers.
// Its pointers address only those allocations. No shared references to its
// records cross threads, and all records are restored by Partition on unwind.
unsafe impl<T: Send> Send for Shard<'_, '_, T> {}

impl<T> Shard<'_, '_, T> {
    /// Scatters this shard with its own classifier, which the pass consumes.
    fn scatter<C: Classify<T>>(mut self, classifier: C) -> Self {
        self.panic = self.sorter.step(classifier).err();
        self
    }
}

/// Partitions the root into the `2^radix` buckets `classifier` assigns and
/// completes it through the shared source ledger.
pub(crate) fn partition_parallel<'records, T: Send, C: Classify<T> + Clone + Send>(
    records: &'records mut [T],
    radix: usize,
    classifier: C,
    buffers: &mut Buffers<T>,
    shards: usize,
    visit: BucketVisit<'_, 'records, T>,
) -> Vec<usize> {
    partition_impl_with_visit(
        records,
        radix,
        classifier,
        buffers,
        Config {
            shards,
            block_size: None,
            parallel: true,
        },
        Some(visit),
    )
}

#[cfg(test)]
fn partition_impl<T: Send, C: Classify<T> + Clone + Send>(
    arr: &mut [T],
    radix: usize,
    classifier: C,
    buffers: &mut Buffers<T>,
    config: Config,
) -> Vec<usize> {
    partition_impl_with_visit(arr, radix, classifier, buffers, config, None)
}

fn partition_impl_with_visit<'records, T: Send, C: Classify<T> + Clone + Send>(
    arr: &'records mut [T],
    radix: usize,
    classifier: C,
    buffers: &mut Buffers<T>,
    config: Config,
    visit: Option<BucketVisit<'_, 'records, T>>,
) -> Vec<usize> {
    assert!((1..=MAX_RADIX_BITS).contains(&radix));
    assert!(core::mem::size_of::<T>() != 0);
    assert!(arr.len() >= 2);
    let bucket_count = 1usize << radix;
    let shards = config.shards.max(1).min(arr.len());
    let record_bytes = core::mem::size_of::<T>();
    let block_size = config.block_size.unwrap_or_else(|| {
        // Local and global directories coexist during adoption, so the balance
        // charges two directories against one scratch reserve per shard bucket.
        let balanced = balanced_size(
            arr.len(),
            record_bytes,
            shards * bucket_count,
            2 * DESCRIPTOR_BYTES_PER_POSITION,
        );
        // Source-ledger transfers pay per-block ownership work regardless of
        // how the engine finishes a bucket. Use the shared amortization scale.
        scaled_block_size(balanced, record_bytes, BLOCK_SCALE)
    });
    let full_blocks = arr.len() / block_size;
    let shards = shards.min(full_blocks.max(1));
    if shards == 1 && config.block_size.is_none() {
        return partition(arr, radix, classifier, buffers, &mut |_, child, _| {
            if let Some(callback) = visit {
                callback(child);
            }
        });
    }
    let input_len = arr.len();
    let input = arr.as_mut_ptr();
    let mut local_buffers: Vec<_> = (0..shards).map(|_| Buffers::new()).collect();
    let mut tasks = Vec::with_capacity(shards);
    let mut first_block = 0;
    for (index, local) in local_buffers.iter_mut().enumerate() {
        let blocks = full_blocks / shards + usize::from(index < full_blocks % shards);
        let len = if index + 1 == shards {
            input_len - first_block * block_size
        } else {
            blocks * block_size
        };
        // SAFETY: cumulative full-block quotas cover disjoint consecutive
        // ranges, with only the final shard including the input tail. Each
        // range is initialized here and stays exclusively owned by its guard.
        // Derive all shard borrows from this same raw ancestor: reborrowing
        // `arr` via split_at_mut would invalidate the raw pointer retained for
        // global compaction after these shard guards have been retired.
        let part =
            unsafe { core::slice::from_raw_parts_mut(input.add(first_block * block_size), len) };
        let shard = Shard {
            sorter: Partition::new(part, bucket_count, block_size, local),
            panic: None,
            first_block,
        };
        tasks.push((shard, classifier.clone()));
        first_block += blocks;
    }
    let mut tasks: Vec<_> = if config.parallel {
        tasks
            .into_par_iter()
            .map(|(shard, classifier)| shard.scatter(classifier))
            .collect()
    } else {
        // The same ownership/adoption path can run under Miri without Rayon.
        tasks
            .into_iter()
            .map(|(shard, classifier)| shard.scatter(classifier))
            .collect()
    };
    if let Some(payload) = tasks.iter_mut().find_map(|task| task.panic.take()) {
        // All jobs have joined. Every guard still owns only its original
        // shard, so unwinding restores all slices without invoking keys again.
        unwind::resume(payload);
    }

    // SAFETY: shards cover consecutive disjoint block-aligned ranges built
    // from this common ancestor, with only the last containing a tail. All
    // completed the same pass and retain their recovery guards until adoption.
    let (mut global, ends) = unsafe { Partition::adopt_shards(input, tasks, buffers) };
    drop(local_buffers);
    finish::complete(&mut global, &ends, config.parallel, visit);
    ends
}

impl<'a, 'w, T> Partition<'a, 'w, T> {
    /// Adopts successfully scattered shards in original input order.
    /// All allocations and directory checks precede ownership transfer, so an
    /// unwind restores records through either the shard or adopted guards.
    ///
    /// # Safety
    ///
    /// Tasks must be in original input order, each holding a completed,
    /// not-yet-materialized single-shard partition and its recovery ownership.
    /// Their slices must be consecutive and disjoint, with the same block size
    /// and bucket count; only the last may have a tail. The first shard starts
    /// at block zero, and every `first_block` is its actual block offset from
    /// `input`. This pointer must be their common raw ancestor, still valid for
    /// the complete input; deriving it from a shard would narrow its borrow.
    unsafe fn adopt_shards(
        input: *mut T,
        mut tasks: Vec<Shard<'a, '_, T>>,
        buffers: &'w mut Buffers<T>,
    ) -> (Self, Vec<usize>) {
        let shards = tasks.len();
        let local = &tasks[0].sorter;
        let block_size = local.blocks.block_size;
        let last = tasks.last().unwrap();
        let input_len = last.first_block * block_size + last.sorter.blocks.input_len;
        let full_blocks = input_len / block_size;
        let bucket_count = local.buffers.counts.len();
        let head = shards
            .checked_mul(local.active_blocks.start)
            .expect("head overflow");
        let local_scratch = local.blocks.scratch_blocks;
        let scratch_blocks = shards.checked_mul(local_scratch).expect("scratch overflow");
        let block_count = full_blocks
            .checked_add(scratch_blocks)
            .expect("block overflow");
        assert!(block_count <= u32::MAX as usize);
        let allocated: usize = tasks
            .iter()
            .map(|task| task.sorter.active_blocks.len())
            .sum();
        let active_end = head + allocated;
        assert!(active_end <= block_count);

        // Until the final non-panicking transfer below, every local guard owns its
        // shard and can recover it if allocation or directory construction fails.
        let mut order = alloc::vec![0u32; block_count];
        let next_order = alloc::vec![0u32; block_count];
        // The adopted guard only compacts or finishes; it never scatters again.
        let bucket_ids = Vec::new();
        let mut partials = Vec::with_capacity(head);
        let mut ends = Vec::with_capacity(bucket_count);
        // Reuse one shard's block-end table for the adopted bucket directory.
        // Local boundaries are available from their trailing partials below;
        // recovery uses the live block sequence, not this pass-local table.
        let mut block_ends = core::mem::take(&mut tasks[0].sorter.buffers.counts);
        let mut output = head;
        let mut records = 0;
        let translate = |shard_index: usize, task: &Shard<'_, '_, T>, physical: u32| {
            let physical = physical as usize;
            if physical < local_scratch {
                shard_index * local_scratch + physical
            } else {
                scratch_blocks + task.first_block + physical - local_scratch
            }
        };
        for (bucket, block_end) in block_ends.iter_mut().enumerate() {
            // Bucket-major, then ORIGINAL shard order. Completion order is never
            // used, so equal keys retain their original cross-shard order.
            for (index, task) in tasks.iter().enumerate() {
                let blocks = task.sorter.bucket_blocks(bucket);
                let partial = task.sorter.buffers.partial_blocks[bucket];
                records += (blocks.len() - 1) * block_size + partial.len;
                for logical in blocks {
                    order[output] =
                        translate(index, task, task.sorter.buffers.block_order[logical]) as u32;
                    output += 1;
                }
                partials.push(PartialBlock {
                    order_index: output - 1,
                    len: partial.len,
                });
            }
            ends.push(records);
            *block_end = output;
        }
        assert_eq!(records, input_len);
        assert_eq!(output, active_end);
        let mut free = 0;
        for (index, task) in tasks.iter().enumerate() {
            for logical in (0..task.sorter.active_blocks.start)
                .chain(task.sorter.active_blocks.end..task.sorter.buffers.block_order.len())
            {
                if free == head {
                    free = active_end;
                }
                order[free] =
                    translate(index, task, task.sorter.buffers.block_order[logical]) as u32;
                free += 1;
            }
        }
        if free == head {
            free = active_end;
        }
        assert_eq!(free, block_count);
        buffers.counts = block_ends;
        buffers.block_order = order;
        buffers.next_block_order = next_order;
        buffers.bucket_ids = bucket_ids;
        buffers.partial_blocks = partials;
        let mut global = Partition {
            buffers,
            blocks: BlockStorage {
                input,
                input_len,

                block_size,
                scratch_blocks,
                block_count,
                scratch: Vec::with_capacity(shards),
                segment_blocks_log2: local_scratch.trailing_zeros(),
                input_borrow: PhantomData,
            },
            active_blocks: head..active_end,
            input_materialized: true,
        };

        // Ownership transfer has no allocations, bounds checks, or user calls:
        // scratch has capacity for one segment per task, in shard order.
        // Each moved Vec owns only MaybeUninit cells, so stale copies never drop T.
        // The global directory already names each live prefix exactly once.
        for task in &mut tasks {
            global
                .blocks
                .scratch
                .append(&mut task.sorter.blocks.scratch);
            task.sorter.input_materialized = true;
        }
        global.input_materialized = false;
        drop(tasks);
        #[cfg(debug_assertions)]
        global.validate();
        (global, ends)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::{ByKey, KeyDigit};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn deferred_owned_is_stable() {
        let mut rows: Vec<_> = (0..293u64)
            .map(|i| ((i * 73) % 64, alloc::format!("row {i}")))
            .collect();
        let mut expected = rows.clone();
        expected.sort_by_key(|row| row.0);
        let pending = std::sync::Mutex::new(None::<&mut [(u64, String)]>);
        let mut buffers = Buffers::new();
        partition_impl_with_visit(
            &mut rows,
            2,
            KeyDigit::new(ByKey(|row: &(u64, alloc::string::String)| row.0), 4),
            &mut buffers,
            config(3, 7),
            Some(&|child| {
                let mut pending = pending.lock().unwrap();
                if let Some(previous) = pending.take() {
                    previous.sort_by_key(|row| row.0);
                }
                *pending = Some(child);
            }),
        );
        if let Some(previous) = pending.into_inner().unwrap() {
            previous.sort_by_key(|row| row.0);
        }
        assert_eq!(rows, expected);
    }

    #[test]
    fn callback_panic_restores_records() {
        let mut rows: Vec<_> = (0..293u64)
            .map(|i| ((i * 73) % 64, alloc::format!("row {i}")))
            .collect();
        let mut expected = rows.clone();
        expected.sort();
        let mut buffers = Buffers::new();
        let result = catch_unwind(AssertUnwindSafe(|| {
            partition_impl_with_visit(
                &mut rows,
                2,
                KeyDigit::new(ByKey(|row: &(u64, alloc::string::String)| row.0), 4),
                &mut buffers,
                config(3, 7),
                Some(&|_| panic!("callback panic")),
            );
        }));
        assert!(result.is_err());
        rows.sort();
        assert_eq!(rows, expected);
    }

    fn config(shards: usize, block_size: usize) -> Config {
        Config {
            shards,
            block_size: Some(block_size),
            parallel: !cfg!(miri),
        }
    }

    struct Owned {
        key: u64,
        id: usize,
        text: String,
        drops: Arc<Vec<AtomicUsize>>,
    }

    impl Drop for Owned {
        fn drop(&mut self) {
            assert_eq!(self.text, self.id.to_string());
            self.drops[self.id].fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn owned_shard_panic_restores() {
        let len = if cfg!(miri) { 19 } else { 97 };
        for fail in (0..len).chain([usize::MAX]) {
            let drops = Arc::new((0..len).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
            let mut rows: Vec<_> = (0..len)
                .map(|id| Owned {
                    key: ((id * 37 + 11) % 16) as u64,
                    id,
                    text: id.to_string(),
                    drops: drops.clone(),
                })
                .collect();
            let result = catch_unwind(AssertUnwindSafe(|| {
                partition_impl(
                    &mut rows,
                    3,
                    KeyDigit::new(
                        ByKey(|row: &Owned| {
                            assert_ne!(row.id, fail, "injected key panic");
                            row.key
                        }),
                        0,
                    ),
                    &mut Buffers::new(),
                    config(4, 5),
                );
            }));
            assert_eq!(result.is_err(), fail != usize::MAX);
            let mut ids: Vec<_> = rows.iter().map(|row| row.id).collect();
            ids.sort_unstable();
            assert_eq!(ids, (0..len).collect::<Vec<_>>());
            assert!(drops.iter().all(|count| count.load(Ordering::SeqCst) == 0));
            drop(rows);
            assert!(drops.iter().all(|count| count.load(Ordering::SeqCst) == 1));
        }
    }

    #[test]
    fn exclusive_shards_preserve_records() {
        let len = if cfg!(miri) { 23 } else { 113 };
        for inconsistent in [false, true] {
            let mut backing: Vec<_> = (0..len).collect();
            let mut records: Vec<_> = backing.iter_mut().enumerate().collect();
            let calls = AtomicUsize::new(0);
            partition_impl(
                &mut records,
                3,
                KeyDigit::new(
                    ByKey(|row: &(usize, &mut usize)| {
                        let call = calls.fetch_add(1, Ordering::Relaxed);
                        (if inconsistent {
                            call ^ row.0
                        } else {
                            row.0 * 37
                        }) as u64
                    }),
                    0,
                ),
                &mut Buffers::new(),
                config(4, 5),
            );
            let mut ids = Vec::new();
            for (id, record) in records {
                assert_eq!(id, *record);
                *record += 1;
                ids.push(id);
            }
            ids.sort_unstable();
            assert_eq!(ids, (0..len).collect::<Vec<_>>());
            assert_eq!(backing, (1..=len).collect::<Vec<_>>());
        }
    }

    #[test]
    fn deferred_exclusive_is_stable() {
        let len = if cfg!(miri) { 53 } else { 293 };
        let mut backing: Vec<_> = (0..len).collect();
        let mut rows: Vec<_> = backing
            .iter_mut()
            .enumerate()
            .map(|(id, r)| (((id * 73) % 64) as u64, id, r))
            .collect();
        let mut expected: Vec<_> = rows.iter().map(|r| (r.0, r.1)).collect();
        expected.sort_by_key(|r| r.0);
        let pending = std::sync::Mutex::new(None::<&mut [(u64, usize, &mut usize)]>);
        let callbacks = AtomicUsize::new(0);
        partition_impl_with_visit(
            &mut rows,
            2,
            KeyDigit::new(ByKey(|r: &(u64, usize, &mut usize)| r.0), 4),
            &mut Buffers::new(),
            config(3, 7),
            Some(&|child| {
                callbacks.fetch_add(1, Ordering::Relaxed);
                let mut pending = pending.lock().unwrap();
                if let Some(previous) = pending.take() {
                    previous.sort_by_key(|r| r.0);
                }
                *pending = Some(child);
            }),
        );
        if let Some(previous) = pending.into_inner().unwrap() {
            previous.sort_by_key(|r| r.0);
        }
        assert!(callbacks.load(Ordering::Relaxed) >= 2);
        assert_eq!(
            rows.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(),
            expected
        );
        for (_, id, r) in rows {
            assert_eq!(*r, id);
            *r += 1;
        }
        assert_eq!(backing, (1..=len).collect::<Vec<_>>());
    }

    #[test]
    fn callback_panic_restores_exclusive() {
        let len = if cfg!(miri) { 53 } else { 293 };
        let mut backing: Vec<_> = (0..len).collect();
        let mut rows: Vec<_> = backing
            .iter_mut()
            .enumerate()
            .map(|(id, r)| (((id * 73) % 64) as u64, id, r))
            .collect();
        let callbacks = AtomicUsize::new(0);
        let consumed = AtomicUsize::new(0);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let pending = std::sync::Mutex::new(None::<&mut [(u64, usize, &mut usize)]>);
            partition_impl_with_visit(
                &mut rows,
                2,
                KeyDigit::new(ByKey(|r: &(u64, usize, &mut usize)| r.0), 4),
                &mut Buffers::new(),
                config(3, 7),
                Some(&|child| {
                    let mut pending = pending
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let position = callbacks.fetch_add(1, Ordering::Relaxed);
                    if position == 1 {
                        if let Some(previous) = pending.take() {
                            previous.sort_by_key(|r| r.0);
                            consumed.fetch_add(previous.len(), Ordering::Relaxed);
                        }
                        assert!(pending.is_none());
                        drop(pending);
                        panic!("second callback after previous exclusive bucket consumed");
                    }
                    if position == 0 {
                        *pending = Some(child);
                    }
                }),
            );
        }));
        assert!(result.is_err());
        assert!(callbacks.load(Ordering::Relaxed) >= 2);
        assert!(consumed.load(Ordering::Relaxed) > 0);
        let mut ids = Vec::new();
        for (_, id, r) in rows {
            assert_eq!(*r, id);
            *r += 1;
            ids.push(id);
        }
        ids.sort_unstable();
        assert_eq!(ids, (0..len).collect::<Vec<_>>());
        assert_eq!(backing, (1..=len).collect::<Vec<_>>());
    }
}
