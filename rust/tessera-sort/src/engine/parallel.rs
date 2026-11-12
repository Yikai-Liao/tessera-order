// SPDX-License-Identifier: BSD-2-Clause

//! Parallel root partition and completion of materialized buckets.

use super::*;
use crate::key::ByKey;
use rayon::{Scope, prelude::*};

/// The archived experiment did not reliably win at 512 KiB; keep 1 MiB as a
/// starting cutoff until controlled measurements retune it.
const MIN_PARALLEL_BYTES: usize = 1 << 20;
/// Nearby sizes showed no reliable win in controlled 10M random A/B screens.
const MIN_TASK_BYTES: usize = 256 * 1024;

/// Sorts large inputs by handing complete root buckets to Rayon's current pool.
pub(crate) fn sort<T: Send, E: Extract<T> + Clone + Send + Sync>(
    records: &mut [T],
    mut extractor: E,
) {
    let record_bytes = core::mem::size_of::<T>();
    let bytes = records.len().saturating_mul(record_bytes);
    if records.len() < 2 || record_bytes == 0 {
        return;
    }
    if bytes < MIN_PARALLEL_BYTES || rayon::current_num_threads() == 1 {
        super::sort::<T, E>(records, extractor);
        return;
    }

    let tile_len = tile_capacity::<T>(TILE_BYTES);
    let mut scratch = Scratch::new();
    let Some(varying) = prepare(records, &mut extractor, tile_len) else {
        return;
    };

    // Keep the existing narrow kernels when only one half of a 128-bit key varies.
    if E::Value::BITS > 64 {
        if varying >> 64 == 0 {
            let mut key = KeyHalf::<E, 0>(extractor);
            sort_root::<T, _>(records, &mut key, &mut scratch, varying, tile_len);
            return;
        }
        if varying & ((1u128 << 64) - 1) == 0 {
            let mut key = KeyHalf::<E, 64>(extractor);
            sort_root::<T, _>(records, &mut key, &mut scratch, varying >> 64, tile_len);
            return;
        }
    }
    sort_root::<T, E>(records, &mut extractor, &mut scratch, varying, tile_len);
}

/// A batch of retired root buckets, retained as separate exclusive borrows.
///
/// Compaction transfers each borrow only after retiring its destination and
/// copy sources. The Rayon scope joins every job before the input is reused.
struct Job<'records, T> {
    records: usize,
    buckets: Vec<&'records mut [T]>,
}

impl<'records, T> Job<'records, T> {
    fn new() -> Self {
        Self {
            records: 0,
            buckets: Vec::new(),
        }
    }

    fn push(&mut self, bucket: &'records mut [T]) {
        self.records += bucket.len();
        self.buckets.push(bucket);
    }

    fn bytes(&self) -> usize {
        self.records.saturating_mul(core::mem::size_of::<T>())
    }

    fn run<E: Extract<T> + Clone + Send>(
        &mut self,
        mut extractor: E,
        candidate: u128,
        tile_len: usize,
        pool: &Pool<T>,
    ) {
        let mut scratch = pool.take();
        let mut sorter = Sorter::<T, E> {
            scratch: &mut scratch,
            extractor: &mut extractor,
            tile_len,
        };
        for child in &mut self.buckets {
            sorter.sort_child(child, candidate);
        }
        pool.give(scratch);
    }
}

/// Recycles scratch between jobs without holding the lock during user code.
struct Pool<T> {
    free: std::sync::Mutex<Vec<Scratch<T>>>,
}

impl<T> Pool<T> {
    fn new() -> Self {
        Self {
            free: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn take(&self) -> Scratch<T> {
        self.free
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .pop()
            .unwrap_or_else(Scratch::new)
    }

    fn give(&self, scratch: Scratch<T>) {
        self.free
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(scratch);
    }
}

/// Partitions the root and sorts each bucket as soon as compaction releases it.
fn sort_root<'records, T: Send, E: Extract<T> + Clone + Send + Sync>(
    records: &'records mut [T],
    extractor: &mut E,
    scratch: &mut Scratch<T>,
    varying: u128,
    tile_len: usize,
) {
    let node_bytes = records.len().saturating_mul(core::mem::size_of::<T>());
    if records.len() < 4 * tile_len {
        Sorter::<T, E> {
            scratch,
            extractor,
            tile_len,
        }
        .sort_node(records, varying);
        return;
    }
    let shards = rayon::current_num_threads().min((node_bytes / MIN_TASK_BYTES).max(1));
    let digit = match plan_node(records, varying, tile_len, extractor) {
        NodePlan::Digit(digit) => digit,
        NodePlan::Equal(equal) => {
            let equal = equal.to_u128();
            let mut key = extractor.clone();
            // Match the serial equality policy. ByKey caches classification and
            // declines filling: equal ranks do not identify record payloads.
            let rank = ByKey(move |record: &T| {
                let value = key.key(record).to_u128();
                u8::from(value >= equal) + u8::from(value > equal)
            });
            let ends = block::partition_digit_parallel(
                records,
                2,
                0,
                rank,
                &mut scratch.block,
                shards,
                block::BucketFinish {
                    limit: 0,
                    sort: &|_| {},
                    visit: &mut |_| {},
                },
            );
            // Materialization has retired all copy sources. Skip the equal band;
            // the unused fourth bucket leaves the greater band through the end.
            let (less, rest) = records.split_at_mut(ends[0]);
            let (_, greater) = rest.split_at_mut(ends[1] - ends[0]);
            scratch.block.recycle_ends(ends);
            let mut right_extractor = extractor.clone();
            [
                (less, scratch, extractor),
                (greater, &mut Scratch::new(), &mut right_extractor),
            ]
            .into_par_iter()
            .for_each(|(records, scratch, extractor)| {
                Sorter {
                    scratch,
                    extractor,
                    tile_len,
                }
                .sort_child(records, varying);
            });
            return;
        }
        plan => {
            Sorter::<T, E> {
                scratch,
                extractor,
                tile_len,
            }
            .execute_node(records, varying, plan);
            return;
        }
    };

    let candidate = varying & !digit.mask::<E::Value>().to_u128();
    // Finished buckets need no jobs' work; direct keys count them in parallel.
    if candidate == 0 && extractor.try_count_window_parallel(records, digit.shift, digit.width) {
        return;
    }

    let pool = Pool::new();
    let pool_ref = &pool;
    let root_extractor = extractor.clone();
    let child_extractor = extractor.clone();
    rayon::scope(move |scope| {
        let pending = std::sync::Mutex::new(None::<Job<'records, T>>);
        let visit = |bucket: &'records mut [T]| {
            // A covering digit already finished these records; do not retain
            // borrows or allocate jobs whose child finisher would return.
            if candidate == 0 {
                return;
            }
            let job = {
                let mut pending = pending
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let mut job = pending.take().unwrap_or_else(Job::new);
                job.push(bucket);
                if job.bytes() < MIN_TASK_BYTES {
                    *pending = Some(job);
                    return;
                }
                job
            };
            // No batch lock crosses scheduling or a user key callback.
            spawn_job::<T, E>(
                scope,
                job,
                candidate,
                child_extractor.clone(),
                tile_len,
                pool_ref,
            );
        };
        let finish = |child: &mut [T]| {
            if candidate == 0 {
                return;
            }
            let mut key = child_extractor.clone();
            let mut scratch = pool_ref.take();
            Sorter::<T, E> {
                scratch: &mut scratch,
                extractor: &mut key,
                tile_len,
            }
            .sort_staged_child(child, candidate);
            pool_ref.give(scratch);
        };
        let limit = bucket_limit::<T>(records.len(), digit.width);
        let ends = block::partition_digit_parallel(
            records,
            digit.width,
            digit.shift,
            root_extractor,
            &mut scratch.block,
            shards,
            block::BucketFinish {
                limit,
                sort: &finish,
                visit: &visit,
            },
        );
        if let Some(job) = pending
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            spawn_job::<T, E>(
                scope,
                job,
                candidate,
                child_extractor.clone(),
                tile_len,
                pool_ref,
            );
        }
        scratch.block.recycle_ends(ends);
    });
}

/// Bounds staging to square-root space and declines infeasible root layouts.
///
/// The 1M/10M paired screens keep the existing small-input route. The cap
/// admits balanced 10M roots with 256 buckets while bounding suspended
/// staging; it deliberately declines larger or concentrated buckets.
fn bucket_limit<T>(len: usize, width: usize) -> usize {
    let limit = len.isqrt().saturating_mul(16);
    if len < (1 << 21) || core::mem::size_of::<T>() > 16 || limit.saturating_mul(1 << width) < len {
        0
    } else {
        limit
    }
}

/// Queues a group of complete buckets on the current Rayon scope.
fn spawn_job<'scope, T: Send + 'scope, E: Extract<T> + Clone + Send + 'scope>(
    scope: &Scope<'scope>,
    mut job: Job<'scope, T>,
    candidate: u128,
    extractor: E,
    tile_len: usize,
    pool: &'scope Pool<T>,
) {
    scope.spawn(move |_| job.run::<E>(extractor, candidate, tile_len, pool));
}

#[cfg(test)]
mod bucket_borrow_tests;
