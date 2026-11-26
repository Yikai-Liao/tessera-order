// SPDX-License-Identifier: BSD-2-Clause

//! Stable root scatter over independently owned block shards.

use super::*;
use rayon::prelude::*;

/// Publishes a nonempty bucket's index, parent-relative range and exclusive
/// final band. Ranges do not depend on callback order; empty buckets are absent.
pub(super) type BucketVisit<'callback, 'records, T> =
    &'callback (dyn Fn(usize, Range<usize>, &'records mut [T]) + Sync);

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
    assert!((1..=MAX_RADIX_BITS).contains(&radix));
    assert!(core::mem::size_of::<T>() != 0);
    assert!(records.len() >= 2);
    let bucket_count = 1usize << radix;
    let shards = shards.clamp(1, records.len());
    let size = block_size(
        records.len(),
        core::mem::size_of::<T>(),
        shards * bucket_count,
        finish::parallel_descriptor_bytes(),
    );
    let full_blocks = records.len() / size;
    let shards = shards.min(full_blocks.max(1));
    if shards == 1 {
        return partition(
            records,
            radix,
            classifier,
            buffers,
            &mut |bucket, range, child, _| visit(bucket, range, child),
        );
    }
    let input_len = records.len();
    let input = records.as_mut_ptr();
    // The first shard reuses the caller's buffers, which keep its scratch.
    let mut local_buffers: Vec<_> = (1..shards).map(|_| Buffers::new()).collect();
    // SAFETY: `input` and `input_len` come from `records`, which stays uniquely
    // borrowed for `'records`, and `shards` matches `1 + local_buffers.len()`.
    let tasks = unsafe {
        prepare_shards(
            input,
            input_len,
            bucket_count,
            size,
            classifier,
            buffers,
            &mut local_buffers,
        )
    };
    let mut partitions: Vec<_> = tasks.into_par_iter().map(scatter_shard).collect();
    // SAFETY: these completed guards cover consecutive disjoint block-aligned
    // ranges from the same ancestor. Only the final partition has a tail.
    unsafe { finish::complete(input, input_len, &mut partitions, visit) }
}

/// Splits `input` into consecutive block-aligned shards derived from the same
/// raw ancestor so the completion ledger can reclaim full input blocks.
///
/// # Safety
///
/// `input` must point to `input_len` initialized, exclusively accessible
/// records for `'records`.
unsafe fn prepare_shards<'records, 'w, T, C: Clone>(
    input: *mut T,
    input_len: usize,
    bucket_count: usize,
    block_size: usize,
    classifier: C,
    buffers: &'w mut Buffers<T>,
    local_buffers: &'w mut [Buffers<T>],
) -> Vec<(Partition<'records, 'w, T>, C)> {
    let full_blocks = input_len / block_size;
    let shards = 1 + local_buffers.len();
    let mut tasks = Vec::with_capacity(shards);
    let mut first_block = 0;
    for (index, local) in core::iter::once(buffers).chain(local_buffers).enumerate() {
        let blocks = full_blocks / shards + usize::from(index < full_blocks % shards);
        let len = if index + 1 == shards {
            input_len - first_block * block_size
        } else {
            blocks * block_size
        };
        // SAFETY: cumulative full-block quotas cover disjoint consecutive
        // ranges, with only the final shard including the input tail. Deriving
        // every borrow from `input` preserves the raw ancestor for the ledger.
        let part =
            unsafe { core::slice::from_raw_parts_mut(input.add(first_block * block_size), len) };
        tasks.push((
            Partition::new(part, bucket_count, block_size, local),
            classifier.clone(),
        ));
        first_block += blocks;
    }
    tasks
}

fn scatter_shard<'a, 'w, T, C: Classify<T>>(
    (mut partition, classifier): (Partition<'a, 'w, T>, C),
) -> Partition<'a, 'w, T> {
    // Each guard restores its original shard during unwinding. Rayon
    // joins all jobs before the parent buffers can be released.
    let _ = partition.step(classifier);
    partition
}

/// Exercises sharded scatter and ledger completion with small test block sizes
/// and optional serial execution under Miri.
#[cfg(test)]
#[expect(clippy::too_many_arguments, reason = "test geometry helper")]
pub(super) fn partition_for_test<'records, T: Send, C: Classify<T> + Clone + Send>(
    records: &'records mut [T],
    radix: usize,
    classifier: C,
    buffers: &mut Buffers<T>,
    shards: usize,
    size: usize,
    parallel: bool,
    visit: Option<BucketVisit<'_, 'records, T>>,
) -> Vec<usize> {
    assert!((1..=MAX_RADIX_BITS).contains(&radix));
    assert!(core::mem::size_of::<T>() != 0);
    assert!(records.len() >= 2);
    let bucket_count = 1usize << radix;
    let full_blocks = records.len() / size;
    let shards = shards.clamp(1, records.len()).min(full_blocks.max(1));
    let input_len = records.len();
    let input = records.as_mut_ptr();
    let mut local_buffers: Vec<_> = (1..shards).map(|_| Buffers::new()).collect();
    // SAFETY: `input` and `input_len` come from `records`, uniquely borrowed for `'records`.
    let tasks = unsafe {
        prepare_shards(
            input,
            input_len,
            bucket_count,
            size,
            classifier,
            buffers,
            &mut local_buffers,
        )
    };
    let mut partitions: Vec<_> = if parallel {
        tasks.into_par_iter().map(scatter_shard).collect()
    } else {
        tasks.into_iter().map(scatter_shard).collect()
    };
    if parallel {
        let noop = |_: usize, _: Range<usize>, _: &mut [T]| {};
        // SAFETY: completed shard guards cover `records` consecutively from `input`.
        unsafe { finish::complete(input, input_len, &mut partitions, visit.unwrap_or(&noop)) }
    } else {
        // SAFETY: completed shard guards cover `records` consecutively from `input`.
        unsafe { finish::complete_serial_for_test(input, input_len, &mut partitions, visit) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::key::{ByKey, KeyDigit};
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
        partition_for_test(
            &mut rows,
            2,
            KeyDigit::new(ByKey(|row: &(u64, alloc::string::String)| row.0), 4),
            &mut buffers,
            3,
            7,
            !cfg!(miri),
            Some(&|_, _, child| {
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
            partition_for_test(
                &mut rows,
                2,
                KeyDigit::new(ByKey(|row: &(u64, alloc::string::String)| row.0), 4),
                &mut buffers,
                3,
                7,
                !cfg!(miri),
                Some(&|_, _, _| panic!("callback panic")),
            );
        }));
        assert!(result.is_err());
        rows.sort();
        assert_eq!(rows, expected);
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
                partition_for_test(
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
                    4,
                    5,
                    !cfg!(miri),
                    None,
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
            partition_for_test(
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
                4,
                5,
                !cfg!(miri),
                None,
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
        partition_for_test(
            &mut rows,
            2,
            KeyDigit::new(ByKey(|r: &(u64, usize, &mut usize)| r.0), 4),
            &mut Buffers::new(),
            3,
            7,
            !cfg!(miri),
            Some(&|_, _, child| {
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
        for consume_saved in [false, true] {
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
                partition_for_test(
                    &mut rows,
                    2,
                    KeyDigit::new(ByKey(|r: &(u64, usize, &mut usize)| r.0), 4),
                    &mut Buffers::new(),
                    3,
                    7,
                    !cfg!(miri),
                    Some(&|_, _, child| {
                        let mut pending = pending
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let position = callbacks.fetch_add(1, Ordering::Relaxed);
                        if position == 1 {
                            if consume_saved && let Some(previous) = pending.take() {
                                previous.sort_by_key(|r| r.0);
                                consumed.fetch_add(previous.len(), Ordering::Relaxed);
                            }
                            assert_eq!(pending.is_none(), consume_saved);
                            drop(pending);
                            panic!("second callback with saved or consumed exclusive bucket");
                        }
                        if position == 0 {
                            *pending = Some(child);
                        }
                    }),
                );
            }));
            assert!(result.is_err());
            assert!(callbacks.load(Ordering::Relaxed) >= 2);
            assert_eq!(consumed.load(Ordering::Relaxed) > 0, consume_saved);
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
}
