// SPDX-License-Identifier: BSD-2-Clause

//! Stable root scatter over independently owned block shards.
//!
//! Each shard scatters its own contiguous input range into its own block
//! directory. Adoption then concatenates every bucket's prefixes in bucket-major,
//! original-shard order, which is what keeps equal keys stable across shards, and
//! hands the combined directory to one compaction pass.

use super::*;
use rayon::prelude::*;

/// One shard's partition state between its scatter and adoption.
struct Shard<'a, 'w, T, E, C> {
    sorter: Partition<'a, 'w, T>,
    key: E,
    classify: C,
    panic: Option<unwind::Payload>,
    first_block: usize,
}

// SAFETY: each shard exclusively borrows a disjoint input slice and Buffers.
// Its pointers address only those allocations. No shared references to its
// records cross threads, and all records are restored by Partition on unwind.
unsafe impl<T: Send, E: Send, C: Send> Send for Shard<'_, '_, T, E, C> {}

impl<T, E, C> Shard<'_, '_, T, E, C>
where
    E: Extract<T>,
    C: Fn(&mut E, &T) -> usize,
{
    /// Scatters this shard's range and keeps any key panic for the caller.
    fn scatter(mut self) -> Self {
        let Self {
            sorter,
            key,
            classify,
            panic,
            ..
        } = &mut self;
        // The scatter indexes its bucket cursor without a bounds check, so the
        // classifier is clamped here exactly as the serial partition clamps it.
        let limit = sorter.bucket_count - 1;
        let mut ends = Vec::new();
        *panic = sorter.step(move |record| classify(key, record).min(limit), &mut ends);
        self
    }
}

/// Partitions a large root by one classifier, scattering its shards in parallel.
///
/// Writes each bucket's cumulative record end into `ends`. The records are back
/// in place when this returns.
///
/// # Panics
///
/// Panics if `bucket_count` is not a power of two in `2..=1 << MAX_RADIX_BITS`,
/// or if `records` is empty or zero-sized. A key-function panic is resumed
/// after every record is back in `records`.
pub(crate) fn partition_parallel<T, E, C>(
    records: &mut [T],
    bucket_count: usize,
    classify: C,
    key: E,
    buffers: &mut Buffers<T>,
    requested_shards: usize,
    ends: &mut Vec<usize>,
) where
    T: Send,
    E: Extract<T> + Clone + Send,
    C: Fn(&mut E, &T) -> usize + Clone + Send,
{
    ends.clear();
    assert!(bucket_count.is_power_of_two() && (2..=1 << MAX_RADIX_BITS).contains(&bucket_count));
    assert!(core::mem::size_of::<T>() != 0);
    assert!(records.len() >= 2);
    let shards = requested_shards.max(1).min(records.len());
    let record_bytes = core::mem::size_of::<T>();
    // Local and global directories coexist during adoption, so a position costs
    // 20 bytes here rather than the 10 of a single pass.
    let balanced = ((20.0 * records.len() as f64)
        / (2.0 * shards as f64 * bucket_count as f64 * record_bytes as f64))
        .sqrt()
        .ceil() as usize;
    let block_size = balanced
        .max(BLOCK_BYTES_FLOOR / record_bytes)
        .max(BLOCK_RECORDS_FLOOR)
        .max(1);
    let full_blocks = records.len() / block_size;
    let shards = shards.min(full_blocks.max(1));
    if shards == 1 {
        // One shard owns the whole root: the serial partition is the same pass.
        let mut key = key;
        return super::partition(records, bucket_count, classify, &mut key, buffers, ends);
    }
    let input_len = records.len();
    let input = records.as_mut_ptr();
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
        // Derive all shard borrows from this same raw ancestor: reborrowing the
        // input via split_at_mut would invalidate the raw pointer retained for
        // global compaction after these shard guards have been retired.
        let part =
            unsafe { core::slice::from_raw_parts_mut(input.add(first_block * block_size), len) };
        tasks.push(Shard {
            sorter: Partition::new(part, bucket_count, block_size, local),
            key: key.clone(),
            classify: classify.clone(),
            panic: None,
            first_block,
        });
        first_block += blocks;
    }
    // Miri interprets the same ownership path without Rayon's scheduler.
    let mut tasks: Vec<_> = if cfg!(miri) {
        tasks.into_iter().map(Shard::scatter).collect()
    } else {
        tasks.into_par_iter().map(Shard::scatter).collect()
    };
    if let Some(payload) = tasks.iter_mut().find_map(|task| task.panic.take()) {
        // All jobs have joined. Every guard still owns only its original shard,
        // so unwinding restores all slices without invoking keys again.
        unwind::resume(payload);
    }

    let head = shards.checked_mul(bucket_count).expect("head overflow");
    let scratch_blocks = head.checked_mul(2).expect("scratch overflow");
    let block_count = full_blocks
        .checked_add(scratch_blocks)
        .expect("block overflow");
    assert!(block_count <= u32::MAX as usize);
    let allocated: usize = tasks
        .iter()
        .map(|task| task.sorter.active_blocks_end - bucket_count)
        .sum();
    let active_end = head + allocated;
    assert!(active_end <= block_count);

    // Until the final non-panicking transfer below, every local guard owns its
    // shard and can recover it if allocation or directory construction fails.
    let mut order = vec![0u32; block_count];
    let next_order = vec![0u32; block_count];
    // The adopted guard only compacts; it never scatters again.
    let bucket_ids = Vec::new();
    let mut partials = Vec::with_capacity(head);
    let mut output = head;
    let mut total = 0;
    let translate = |shard_index: usize, task: &Shard<'_, '_, T, E, C>, physical: u32| {
        let physical = physical as usize;
        let translated = if physical < 2 * bucket_count {
            shard_index * 2 * bucket_count + physical
        } else {
            scratch_blocks + task.first_block + physical - 2 * bucket_count
        };
        translated as u32
    };
    for bucket in 0..bucket_count {
        // Bucket-major, then ORIGINAL shard order. Completion order is never
        // used, so equal keys retain their original cross-shard order.
        for (index, task) in tasks.iter().enumerate() {
            let start = if bucket == 0 {
                bucket_count
            } else {
                task.sorter.buffers.counts[bucket - 1]
            };
            let end = task.sorter.buffers.counts[bucket];
            let partial = task.sorter.partial_blocks[bucket];
            assert_eq!(partial.order_index, end - 1);
            for logical in start..end {
                order[output] = translate(index, task, task.sorter.block_order[logical]);
                output += 1;
            }
            partials.push(PartialBlock {
                order_index: output - 1,
                len: partial.len,
            });
            total += (end - start - 1) * block_size + partial.len;
        }
        ends.push(total);
    }
    assert_eq!(total, input_len);
    assert_eq!(output, active_end);
    let mut free = 0;
    for (index, task) in tasks.iter().enumerate() {
        for logical in
            (0..bucket_count).chain(task.sorter.active_blocks_end..task.sorter.block_order.len())
        {
            if free == head {
                free = active_end;
            }
            order[free] = translate(index, task, task.sorter.block_order[logical]);
            free += 1;
        }
    }
    if free == head {
        free = active_end;
    }
    assert_eq!(free, block_count);
    let mut global = Partition {
        buffers,
        blocks: BlockStorage {
            input,
            input_len,
            block_size,
            scratch_blocks,
            block_count,
            scratch: Vec::new(),
            scratch_segments: Vec::with_capacity(shards),
            input_borrow: PhantomData,
        },
        // For compaction this field means the free prefix length, not the
        // number of semantic buckets. No scatter runs on this guard.
        bucket_count: head,
        block_order: order,
        next_block_order: next_order,
        bucket_ids,
        partial_blocks: partials,
        active_blocks_end: active_end,
        input_materialized: true,
    };

    // Ownership transfer has no allocations, bounds checks, or user calls:
    // scratch_segments has capacity for exactly tasks.len() allocations. Each
    // moved Vec owns only MaybeUninit cells, so stale copies never drop T. The
    // global directory already names each live prefix exactly once.
    for task in &mut tasks {
        global
            .blocks
            .scratch_segments
            .push(core::mem::take(&mut task.sorter.blocks.scratch));
        task.sorter.input_materialized = true;
    }
    global.input_materialized = false;
    drop(tasks);
    drop(local_buffers);
    #[cfg(debug_assertions)]
    global.validate();
    global.compact();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Digit;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    /// Extracts the first field of a two-field demonstration record.
    #[derive(Clone, Copy)]
    struct TupleKey;

    impl Extract<(u64, String)> for TupleKey {
        type Value = u64;

        fn key_function() -> bool {
            false
        }

        fn key(&mut self, row: &(u64, String)) -> u64 {
            row.0
        }
    }

    #[test]
    fn adopted_directories_stay_stable() {
        let mut rows: Vec<_> = (0..293u64)
            .map(|index| ((index * 73) % 64, format!("row {index}")))
            .collect();
        let mut expected = rows.clone();
        expected.sort_by_key(|row| row.0);
        let mut buffers = Buffers::new();
        let mut ends = Vec::new();
        let digit = Digit { shift: 4, width: 2 };
        partition_parallel(
            &mut rows,
            4,
            move |extractor: &mut TupleKey, row: &(u64, String)| digit.classify(extractor, row),
            TupleKey,
            &mut buffers,
            3,
            &mut ends,
        );
        rows.sort_by_key(|row| row.0);
        assert_eq!(rows, expected);
        assert_eq!(ends.len(), 4);
        assert_eq!(ends.last().copied(), Some(293));
    }

    /// A record that owns a string and counts its own drops.
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

    /// Fails the scatter at one record, so a panic interrupts the pass.
    #[derive(Clone)]
    struct FailingKey {
        fail: usize,
    }

    impl Extract<Owned> for FailingKey {
        type Value = u64;

        fn key_function() -> bool {
            false
        }

        fn key(&mut self, row: &Owned) -> u64 {
            assert_ne!(row.id, self.fail, "injected key panic");
            row.key
        }
    }

    #[test]
    fn shard_panic_restores_records() {
        let len = if cfg!(miri) { 19 } else { 257 };
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
                partition_parallel(
                    &mut rows,
                    8,
                    |extractor: &mut FailingKey, row: &Owned| extractor.key(row) as usize & 7,
                    FailingKey { fail },
                    &mut Buffers::new(),
                    4,
                    &mut Vec::new(),
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

    /// Keys a borrowed record, optionally changing its answer mid-pass.
    #[derive(Clone)]
    struct BorrowKey {
        calls: Arc<AtomicUsize>,
        changing: bool,
    }

    impl Extract<(usize, &mut usize)> for BorrowKey {
        type Value = usize;

        fn key_function() -> bool {
            false
        }

        fn key(&mut self, record: &(usize, &mut usize)) -> usize {
            let call = self.calls.fetch_add(1, Ordering::Relaxed);
            if self.changing && call.is_multiple_of(3) {
                *record.1
            } else {
                record.0
            }
        }
    }

    #[test]
    fn exclusive_shards_preserve_borrows() {
        let len = if cfg!(miri) { 23 } else { 113 };
        for changing in [false, true] {
            let mut backing: Vec<_> = (0..len).collect();
            let mut records: Vec<_> = backing.iter_mut().enumerate().collect();
            let key = BorrowKey {
                calls: Arc::new(AtomicUsize::new(0)),
                changing,
            };
            partition_parallel(
                &mut records,
                4,
                |_: &mut BorrowKey, record: &(usize, &mut usize)| record.0 & 3,
                key,
                &mut Buffers::new(),
                4,
                &mut Vec::new(),
            );
            let mut ids: Vec<usize> = records.iter().map(|record| *record.1).collect();
            ids.sort_unstable();
            assert_eq!(ids, (0..len).collect::<Vec<_>>());
            for record in records {
                *record.1 += 1;
            }
            assert_eq!(backing, (1..=len).collect::<Vec<_>>());
        }
    }
}
