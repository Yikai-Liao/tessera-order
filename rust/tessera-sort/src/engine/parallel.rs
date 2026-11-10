// SPDX-License-Identifier: BSD-2-Clause

//! Parallel root partition and completion of materialized buckets.

use super::*;
use crate::key::impl_key_half;
use rayon::Scope;

/// The archived experiment did not reliably win at 512 KiB; keep 1 MiB as a
/// starting cutoff until controlled measurements retune it.
const MIN_PARALLEL_BYTES: usize = 1 << 20;
/// Nearby sizes showed no reliable win in controlled 10M random A/B screens.
const MIN_TASK_BYTES: usize = 256 * 1024;

/// Sorts large inputs by handing complete root buckets to Rayon's current pool.
pub(crate) fn sort<T: Send, E: Extract<T> + Clone + Send + Sync, const CACHED: bool>(
    records: &mut [T],
    extractor: &mut E,
) {
    let record_bytes = core::mem::size_of::<T>();
    let bytes = records.len().saturating_mul(record_bytes);
    if records.len() < 2 || record_bytes == 0 {
        return;
    }
    if bytes < MIN_PARALLEL_BYTES || rayon::current_num_threads() == 1 {
        super::sort::<T, E, CACHED>(records, extractor);
        return;
    }

    let tile_bytes = TILE_BYTES
        .max(record_bytes)
        .min(MAX_TILE_RECORDS.saturating_mul(record_bytes));
    let mut scratch = Scratch::new();
    let Some(varying) = prepare(records, extractor, tile_bytes) else {
        return;
    };

    // Keep the existing narrow kernels when only one half of a 128-bit key varies.
    if E::Value::BITS > 64 {
        if varying >> 64 == 0 {
            let mut key = Narrow::<E, 0>(extractor.clone());
            sort_root::<T, _, CACHED>(records, &mut key, &mut scratch, varying, tile_bytes);
            return;
        }
        if varying & ((1u128 << 64) - 1) == 0 {
            let mut key = Narrow::<E, 64>(extractor.clone());
            sort_root::<T, _, CACHED>(records, &mut key, &mut scratch, varying >> 64, tile_bytes);
            return;
        }
    }
    sort_root::<T, E, CACHED>(records, extractor, &mut scratch, varying, tile_bytes);
}

/// Owns a half-key view so parallel jobs can clone the original extractor.
#[derive(Clone)]
struct Narrow<E, const SHIFT: usize>(E);

impl_key_half!(Narrow<E, SHIFT>);

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

    fn run<E: Extract<T> + Clone + Send, const CACHED: bool>(
        &mut self,
        mut extractor: E,
        candidate: u128,
        tile_bytes: usize,
        pool: &Pool<T>,
    ) {
        let mut scratch = pool.take();
        for child in &mut self.buckets {
            if child.len() > 1 {
                let varying = scan_varying_bits(child, candidate, &mut extractor);
                Sorter::<T, E, CACHED> {
                    scratch: &mut scratch,
                    extractor: &mut extractor,
                    tile_bytes,
                }
                .sort_node(child, varying);
            }
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
fn sort_root<'records, T: Send, E: Extract<T> + Clone + Send + Sync, const CACHED: bool>(
    records: &'records mut [T],
    extractor: &mut E,
    scratch: &mut Scratch<T>,
    varying: u128,
    tile_bytes: usize,
) {
    let node_bytes = records.len().saturating_mul(core::mem::size_of::<T>());
    if node_bytes < 4 * tile_bytes {
        Sorter::<T, E, CACHED> {
            scratch,
            extractor,
            tile_bytes,
        }
        .sort_node(records, varying);
        return;
    }
    let shards = rayon::current_num_threads().min((node_bytes / MIN_TASK_BYTES).max(1));
    let digit = match plan_node(records, varying, tile_bytes, extractor) {
        NodePlan::Window(digit) => {
            if extractor.try_count_window_parallel(records, digit.shift, digit.width) {
                return;
            }
            let limit = bucket_limit::<T>(records.len(), digit.width);
            let ends = block::partition_digit_parallel_with_finish(
                records,
                digit.width,
                digit.shift,
                extractor.clone(),
                &mut scratch.block,
                shards,
                block::BucketFinish {
                    limit,
                    sort: &|_| {},
                    visit: &mut |_| {},
                },
            );
            scratch.block.recycle_ends(ends);
            return;
        }
        NodePlan::Digit(digit) => digit,
        plan => {
            Sorter::<T, E, CACHED> {
                scratch,
                extractor,
                tile_bytes,
            }
            .execute_node(records, varying, plan);
            return;
        }
    };

    let candidate = varying & !digit.mask::<E::Value>().to_u128();
    if candidate == 0 {
        let ends = block::partition_digit_parallel_with_visit(
            records,
            digit.width,
            digit.shift,
            extractor.clone(),
            &mut scratch.block,
            shards,
            &mut |_| {},
        );
        scratch.block.recycle_ends(ends);
        return;
    }

    let pool = Pool::new();
    let pool_ref = &pool;
    let root_extractor = extractor.clone();
    let child_extractor = extractor.clone();
    rayon::scope(move |scope| {
        let mut pending: Option<Job<'records, T>> = None;
        let mut visit = |bucket: &'records mut [T]| {
            let mut job = pending.take().unwrap_or_else(Job::new);
            job.push(bucket);
            if job.bytes() >= MIN_TASK_BYTES {
                spawn_job::<T, E, CACHED>(
                    scope,
                    job,
                    candidate,
                    child_extractor.clone(),
                    tile_bytes,
                    pool_ref,
                );
            } else {
                pending = Some(job);
            }
        };
        let finish = |child: &mut [T]| {
            let mut key = child_extractor.clone();
            let mut scratch = pool_ref.take();
            let varying = scan_varying_bits(child, candidate, &mut key);
            let tile_bytes = tile_bytes
                .max(child.len().saturating_mul(core::mem::size_of::<T>()))
                .min(MAX_TILE_RECORDS.saturating_mul(core::mem::size_of::<T>()));
            Sorter::<T, E, CACHED> {
                scratch: &mut scratch,
                extractor: &mut key,
                tile_bytes,
            }
            .sort_node(child, varying);
            pool_ref.give(scratch);
        };
        let limit = bucket_limit::<T>(records.len(), digit.width);
        let ends = block::partition_digit_parallel_with_finish(
            records,
            digit.width,
            digit.shift,
            root_extractor,
            &mut scratch.block,
            shards,
            block::BucketFinish {
                limit,
                sort: &finish,
                visit: &mut visit,
            },
        );
        drop(visit);
        if let Some(job) = pending {
            spawn_job::<T, E, CACHED>(
                scope,
                job,
                candidate,
                child_extractor.clone(),
                tile_bytes,
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
fn spawn_job<
    'scope,
    T: Send + 'scope,
    E: Extract<T> + Clone + Send + 'scope,
    const CACHED: bool,
>(
    scope: &Scope<'scope>,
    mut job: Job<'scope, T>,
    candidate: u128,
    extractor: E,
    tile_bytes: usize,
    pool: &'scope Pool<T>,
) {
    scope.spawn(move |_| job.run::<E, CACHED>(extractor, candidate, tile_bytes, pool));
}

#[cfg(test)]
mod bucket_borrow_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_half_keys_keep_encoded_bits() {
        let key = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210u128;
        assert_eq!(Narrow::<_, 0>(Direct).key(&key), 0xfedc_ba98_7654_3210);
        assert_eq!(Narrow::<_, 64>(Direct).key(&key), 0x0123_4567_89ab_cdef);
    }

    #[test]
    fn owned_half_windows_keep_offsets_and_payloads() {
        crate::key::tests::check_half_windows::<0>(
            &mut Narrow::<_, 0>(Direct),
            &mut Narrow::<_, 0>(crate::key::ByKey(|record: &(u128, usize)| record.0)),
        );
        crate::key::tests::check_half_windows::<64>(
            &mut Narrow::<_, 64>(Direct),
            &mut Narrow::<_, 64>(crate::key::ByKey(|record: &(u128, usize)| record.0)),
        );
    }
}
