// SPDX-License-Identifier: BSD-2-Clause

//! Stable root scatter over independently owned block shards.

use super::*;
use rayon::prelude::*;

mod compact;
mod finish;

/// Finishes one privately staged bucket without retaining its record borrow.
type BucketSort<'a, T> = &'a (dyn Fn(&mut [T]) + Sync);

/// Bounded finishing with the existing materialized-bucket fallback.
pub(crate) struct BucketFinish<'callback, 'records, T> {
    pub limit: usize,
    pub sort: BucketSort<'callback, T>,
    pub visit: Visit<'callback, 'records, T>,
}

/// Sorting callback and size cap; absent for fallback-only partitions.
struct BoundedFinish<'a, T> {
    limit: usize,
    sort: BucketSort<'a, T>,
}

struct Shard<'a, 'w, T, E> {
    sorter: Partition<'a, 'w, T>,
    key: E,
    panic: Option<unwind::Payload>,
    first_block: usize,
}

struct Config<'a, T> {
    shards: usize,
    block_size: Option<usize>,
    parallel: bool,
    finish: Option<BoundedFinish<'a, T>>,
}

// SAFETY: each shard exclusively borrows a disjoint input slice and Buffers.
// Its pointers address only those allocations. No shared references to its
// records cross threads, and all records are restored by Partition on unwind.
unsafe impl<T: Send, E: Send> Send for Shard<'_, '_, T, E> {}

impl<T, E: Extract<T>> Shard<'_, '_, T, E> {
    fn scatter(mut self, width: usize, shift: usize) -> Self {
        let shift = E::Value::from_usize(shift);
        let mask = E::Value::from_usize((1usize << width) - 1) << shift;
        let key = &mut self.key;
        self.panic = self
            .sorter
            .step(move |record| ((key.key(record) & mask) >> shift).to_usize());
        self
    }
}

/// Partitions the root with an optional bounded pre-materialization finisher.
pub(crate) fn partition_digit_parallel<'records, T: Send, E: Extract<T> + Clone + Send>(
    records: &'records mut [T],
    width: usize,
    shift: usize,
    key: E,
    buffers: &mut Buffers<T>,
    shards: usize,
    finish: BucketFinish<'_, 'records, T>,
) -> Vec<usize> {
    partition_impl_with_visit(
        records,
        width,
        shift,
        key,
        buffers,
        Config {
            shards,
            block_size: None,
            parallel: true,
            finish: Some(BoundedFinish {
                limit: finish.limit,
                sort: finish.sort,
            }),
        },
        Some(finish.visit),
    )
}

#[cfg(test)]
fn partition_impl<T: Send, E: Extract<T> + Clone + Send>(
    arr: &mut [T],
    radix: usize,
    shift: usize,
    key: E,
    buffers: &mut Buffers<T>,
    config: Config<'_, T>,
) -> Vec<usize> {
    partition_impl_with_visit(arr, radix, shift, key, buffers, config, None)
}

fn partition_impl_with_visit<'records, T: Send, E: Extract<T> + Clone + Send>(
    arr: &'records mut [T],
    radix: usize,
    shift: usize,
    key: E,
    buffers: &mut Buffers<T>,
    config: Config<'_, T>,
    mut visit: Option<Visit<'_, 'records, T>>,
) -> Vec<usize> {
    assert!((1..=MAX_RADIX_BITS).contains(&radix));
    assert!(shift < E::Value::BITS);
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
        let bounded = config
            .finish
            .as_ref()
            .is_some_and(|finish| finish.limit > 0);
        scaled_block_size(
            balanced,
            record_bytes,
            if bounded { BLOCK_SCALE } else { 1 },
        )
    });
    let full_blocks = arr.len() / block_size;
    let shards = shards.min(full_blocks.max(1));
    if shards == 1 && config.block_size.is_none() {
        let mut key = key;
        return partition_digit(arr, radix, shift, &mut key, buffers, &mut |child, _| {
            if let Some(callback) = visit.as_deref_mut() {
                callback(child);
            }
        });
    }
    let width = radix.min(E::Value::BITS - shift);
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
        tasks.push(Shard {
            sorter: Partition::new(part, bucket_count, block_size, local),
            key: key.clone(),
            panic: None,
            first_block,
        });
        first_block += blocks;
    }
    let mut tasks: Vec<_> = if config.parallel {
        tasks
            .into_par_iter()
            .map(|shard| shard.scatter(width, shift))
            .collect()
    } else {
        // The same ownership/adoption path can run under Miri without Rayon.
        tasks
            .into_iter()
            .map(|shard| shard.scatter(width, shift))
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
    if let Some(finisher) = config.finish
        && finish::try_finish(
            &mut global,
            &ends,
            config.parallel,
            finisher.limit,
            finisher.sort,
        )
    {
        return ends;
    }
    compact::permute_blocks(&mut global, shards, config.parallel);
    let panic = global.compact_with(&ends, visit);
    if let Some(payload) = panic {
        unwind::resume(payload);
    }
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
    unsafe fn adopt_shards<E>(
        input: *mut T,
        mut tasks: Vec<Shard<'a, '_, T, E>>,
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
        let translate = |shard_index: usize, task: &Shard<'_, '_, T, E>, physical: u32| {
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
                let partial = task.sorter.partial_blocks[bucket];
                records += (blocks.len() - 1) * block_size + partial.len;
                for logical in blocks {
                    order[output] = translate(index, task, task.sorter.block_order[logical]) as u32;
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
                .chain(task.sorter.active_blocks.end..task.sorter.block_order.len())
            {
                if free == head {
                    free = active_end;
                }
                order[free] = translate(index, task, task.sorter.block_order[logical]) as u32;
                free += 1;
            }
        }
        if free == head {
            free = active_end;
        }
        assert_eq!(free, block_count);
        buffers.counts = block_ends;
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
            block_order: order,
            next_block_order: next_order,
            bucket_ids,
            partial_blocks: partials,
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
    use crate::key::ByKey;
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
        let mut pending: Option<(*mut (u64, alloc::string::String), usize)> = None;
        let mut buffers = Buffers::new();
        partition_impl_with_visit(
            &mut rows,
            2,
            4,
            ByKey(|row: &(u64, alloc::string::String)| row.0),
            &mut buffers,
            config(3, 7),
            Some(&mut |child| {
                if let Some((pointer, len)) = pending.take() {
                    // SAFETY: this is an earlier materialized disjoint bucket.
                    unsafe { core::slice::from_raw_parts_mut(pointer, len) }
                        .sort_by_key(|row| row.0);
                }
                pending = Some((child.as_mut_ptr(), child.len()));
            }),
        );
        if let Some((pointer, len)) = pending {
            // SAFETY: the final bucket remains uniquely owned until consumed.
            unsafe { core::slice::from_raw_parts_mut(pointer, len) }.sort_by_key(|row| row.0);
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
                4,
                ByKey(|row: &(u64, alloc::string::String)| row.0),
                &mut buffers,
                config(3, 7),
                Some(&mut |_| panic!("callback panic")),
            );
        }));
        assert!(result.is_err());
        rows.sort();
        assert_eq!(rows, expected);
    }

    fn config<'a, T>(shards: usize, block_size: usize) -> Config<'a, T> {
        Config {
            shards,
            block_size: Some(block_size),
            parallel: !cfg!(miri),
            finish: None,
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
                    0,
                    ByKey(|row: &Owned| {
                        assert_ne!(row.id, fail, "injected key panic");
                        row.key
                    }),
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
                0,
                ByKey(|row: &(usize, &mut usize)| {
                    let call = calls.fetch_add(1, Ordering::Relaxed);
                    (if inconsistent {
                        call ^ row.0
                    } else {
                        row.0 * 37
                    }) as u64
                }),
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
        let mut pending: Option<(*mut (u64, usize, &mut usize), usize)> = None;
        let mut callbacks = 0;
        partition_impl_with_visit(
            &mut rows,
            2,
            4,
            ByKey(|r: &(u64, usize, &mut usize)| r.0),
            &mut Buffers::new(),
            config(3, 7),
            Some(&mut |child| {
                callbacks += 1;
                if let Some((pointer, len)) = pending.take() {
                    // SAFETY: this is a previous, retired, disjoint bucket;
                    // the current callback cannot overwrite or borrow it.
                    let previous = unsafe { core::slice::from_raw_parts_mut(pointer, len) };
                    previous.sort_by_key(|r| r.0);
                }
                pending = Some((child.as_mut_ptr(), child.len()));
            }),
        );
        if let Some((pointer, len)) = pending {
            // SAFETY: this final retired interval remains exclusively owned.
            unsafe { core::slice::from_raw_parts_mut(pointer, len) }.sort_by_key(|r| r.0);
        }
        assert!(callbacks >= 2);
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
        let mut pending: Option<(*mut (u64, usize, &mut usize), usize)> = None;
        let mut callbacks = 0;
        let mut consumed = 0;
        let result = catch_unwind(AssertUnwindSafe(|| {
            partition_impl_with_visit(
                &mut rows,
                2,
                4,
                ByKey(|r: &(u64, usize, &mut usize)| r.0),
                &mut Buffers::new(),
                config(3, 7),
                Some(&mut |child| {
                    callbacks += 1;
                    if let Some((pointer, len)) = pending.take() {
                        // SAFETY: the earlier retired bucket is disjoint and
                        // cannot be accessed by the remaining producer work.
                        let previous = unsafe { core::slice::from_raw_parts_mut(pointer, len) };
                        previous.sort_by_key(|r| r.0);
                        consumed += len;
                        panic!("second callback after previous exclusive bucket consumed");
                    }
                    pending = Some((child.as_mut_ptr(), child.len()));
                }),
            );
        }));
        assert!(result.is_err());
        assert_eq!(callbacks, 2);
        assert!(consumed > 0);
        assert!(pending.is_none());
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
