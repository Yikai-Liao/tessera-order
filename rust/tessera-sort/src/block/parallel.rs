// SPDX-License-Identifier: BSD-2-Clause

//! Stable root scatter over independently owned block shards.

use super::*;
use rayon::prelude::*;

/// Publishes a bucket's index and exclusive final band, independently of other
/// bucket movement.
pub(super) type BucketVisit<'callback, 'records, T> =
    &'callback (dyn Fn(usize, &'records mut [T]) + Sync);

pub(super) struct Config {
    pub(super) shards: usize,
    pub(super) block_size: Option<usize>,
    pub(super) parallel: bool,
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

pub(super) fn partition_impl_with_visit<'records, T: Send, C: Classify<T> + Clone + Send>(
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
        // The completion owner charges all live tables, including shard links.
        // Balance them against one scratch reserve per shard bucket.
        let balanced = balanced_size(
            arr.len(),
            record_bytes,
            shards * bucket_count,
            finish::parallel_descriptor_bytes(),
        );
        // Source-ledger transfers pay per-block ownership work regardless of
        // how the engine finishes a bucket. Use the shared amortization scale.
        scaled_block_size(balanced, record_bytes, BLOCK_SCALE)
    });
    let full_blocks = arr.len() / block_size;
    let shards = shards.min(full_blocks.max(1));
    if shards == 1 && config.block_size.is_none() {
        return partition(arr, radix, classifier, buffers, &mut |bucket, child, _| {
            if let Some(callback) = visit {
                callback(bucket, child);
            }
        });
    }
    let input_len = arr.len();
    let input = arr.as_mut_ptr();
    // The first shard reuses the caller's buffers, which keep its scratch.
    let mut local_buffers: Vec<_> = (1..shards).map(|_| Buffers::new()).collect();
    let mut tasks = Vec::with_capacity(shards);
    let mut first_block = 0;
    for (index, local) in core::iter::once(buffers)
        .chain(&mut local_buffers)
        .enumerate()
    {
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
        // `arr` via split_at_mut would invalidate the raw pointer the ledger
        // uses after these shard guards have been retired.
        let part =
            unsafe { core::slice::from_raw_parts_mut(input.add(first_block * block_size), len) };
        tasks.push((
            Partition::new(part, bucket_count, block_size, local),
            classifier.clone(),
        ));
        first_block += blocks;
    }
    fn scatter<'a, 'w, T, C: Classify<T>>(
        (mut partition, classifier): (Partition<'a, 'w, T>, C),
    ) -> Partition<'a, 'w, T> {
        if let Err(payload) = partition.step(classifier) {
            // Each independent guard restores its own original shard. Rayon
            // joins all jobs before propagating an error out of this collect.
            unwind::resume(payload);
        }
        partition
    }
    let mut partitions: Vec<_> = if config.parallel {
        tasks.into_par_iter().map(scatter).collect()
    } else {
        tasks.into_iter().map(scatter).collect()
    };
    // SAFETY: these completed guards cover consecutive disjoint block-aligned
    // ranges from the same ancestor. Only the final partition has a tail.
    unsafe { finish::complete(input, input_len, &mut partitions, config.parallel, visit) }
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
            Some(&|_, child| {
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
                Some(&|_, _| panic!("callback panic")),
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
            Some(&|_, child| {
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
                partition_impl_with_visit(
                    &mut rows,
                    2,
                    KeyDigit::new(ByKey(|r: &(u64, usize, &mut usize)| r.0), 4),
                    &mut Buffers::new(),
                    config(3, 7),
                    Some(&|_, child| {
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
