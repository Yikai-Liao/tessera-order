// SPDX-License-Identifier: BSD-2-Clause

//! Parallel root partition and completion of materialized buckets.

use super::*;
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

/// Keeps parallel preparation's helper call contexts separate from serial callers.
struct RootExtractor<'a, E>(&'a mut E);

impl<T, E: Extract<T>> Extract<T> for RootExtractor<'_, E> {
    type Value = E::Value;

    fn key(&mut self, record: &T) -> Self::Value {
        self.0.key(record)
    }

    fn confirm_ascending(&mut self, records: &[T], previous: Self::Value) -> bool {
        self.0.confirm_ascending(records, previous)
    }

    fn insertion_sort(&mut self, records: &mut [T], budget: usize) -> Option<usize> {
        self.0.insertion_sort(records, budget)
    }
}

/// Prepares a large root while preserving the serial entry's call geometry.
fn prepare<T, E: Extract<T>>(
    records: &mut [T],
    extractor: &mut E,
    tile_bytes: usize,
) -> Option<u128> {
    // A separate instantiation preserves the serial helpers' caller ranges and
    // inlining contexts. Backend methods still delegate to the original extractor.
    let mut root = RootExtractor(extractor);
    let extractor = &mut root;
    let record_bytes = core::mem::size_of::<T>();
    let sampled = probe_order(records, extractor);
    match sampled {
        Order::Ascending => return None,
        Order::Descending { strict } => {
            if strict {
                reverse_strictly(records);
            } else {
                reverse_stably(records, extractor);
            }
            return None;
        }
        Order::Unsorted => {
            // The parallel entry already rejected roots below 1 MiB, so this
            // root cannot fit the serial insertion leaf's one-tile budget.
            let probe_len = (tile_bytes / record_bytes).max(2).min(records.len());
            if insertion_probe(records, extractor, probe_len) {
                return None;
            }
        }
        Order::Flat => {}
    }

    let varying = block::varying_key_bits(records, extractor);
    if varying == 0 {
        return None;
    }
    if sampled == Order::Flat {
        // An equal sampled pair rules out strictly descending order; confirmed
        // descending keys must therefore retain the stable reversal.
        match confirm_order(records, extractor) {
            Order::Ascending => return None,
            Order::Descending { .. } => {
                reverse_stably(records, extractor);
                return None;
            }
            Order::Unsorted | Order::Flat => {}
        }
    }
    Some(varying)
}

#[derive(Clone)]
struct Narrow<E, const SHIFT: usize>(E);

impl<T, E: Extract<T>, const SHIFT: usize> Extract<T> for Narrow<E, SHIFT> {
    type Value = u64;

    #[inline]
    fn key(&mut self, record: &T) -> u64 {
        (self.0.key(record).to_u128() >> SHIFT) as u64
    }

    fn try_count_window(&mut self, records: &mut [T], low: usize, width: usize) -> bool {
        self.0.try_count_window(records, low + SHIFT, width)
    }

    fn try_count_window_parallel(&mut self, records: &mut [T], low: usize, width: usize) -> bool
    where
        T: Send,
    {
        self.0
            .try_count_window_parallel(records, low + SHIFT, width)
    }
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
    let low = varying.trailing_zeros() as usize;
    let top = bit_len(varying);
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
    if top - low <= SPLIT_BITS {
        if extractor.try_count_window_parallel(records, low, top - low) {
            return;
        }
        let limit = bucket_limit::<T>(records.len(), top - low);
        let ends = block::partition_digit_parallel_with_finish(
            records,
            top - low,
            low,
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

    let mut keys = [KeyValue::<T, E>::from_usize(0); MODE_SAMPLE_LEN];
    sample_keys(records, &mut keys, extractor);
    if let Some(equal) = sample_mode(&mut keys) {
        sort_repeated_root::<T, E, CACHED>(
            records,
            extractor,
            scratch,
            equal.to_u128(),
            varying,
            tile_bytes,
            shards,
        );
        return;
    }
    // A node the order already groups is finished sequentially: the run split
    // needs no partition pass, so there is nothing to share.
    let top_bit = 1u128 << (top - 1);
    if two_runs(records, top_bit, extractor).is_some_and(|(_, set)| !set) {
        Sorter::<T, E, CACHED> {
            scratch,
            extractor,
            tile_bytes,
        }
        .sort_node(records, varying);
        return;
    }

    let digit = choose_split_digit(records, top, tile_bytes, extractor);
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

/// Encodes the three bands of `Sorter::partition_equal` as a two-bit digit.
///
/// The rank is not a record key, so counting/filling is never forwarded.
#[derive(Clone)]
struct EqualityRank<E> {
    extractor: E,
    equal: u128,
}

impl<T, E: Extract<T>> Extract<T> for EqualityRank<E> {
    type Value = u8;

    fn key(&mut self, record: &T) -> u8 {
        let key = self.extractor.key(record).to_u128();
        u8::from(key >= self.equal) + u8::from(key > self.equal)
    }
}

/// Materializes a sharded equality partition and sorts only its two outer bands.
fn sort_repeated_root<T: Send, E: Extract<T> + Clone + Send + Sync, const CACHED: bool>(
    records: &mut [T],
    extractor: &mut E,
    scratch: &mut Scratch<T>,
    equal: u128,
    varying: u128,
    tile_bytes: usize,
    shards: usize,
) {
    let ends = block::partition_digit_parallel_with_visit(
        records,
        2,
        0,
        EqualityRank {
            extractor: extractor.clone(),
            equal,
        },
        &mut scratch.block,
        shards,
        &mut |_| {},
    );
    let (less, rest) = records.split_at_mut(ends[0]);
    let (_, greater) = rest.split_at_mut(ends[1] - ends[0]);
    scratch.block.recycle_ends(ends);
    let mut right_extractor = extractor.clone();
    rayon::join(
        || {
            Sorter::<T, E, CACHED> {
                scratch,
                extractor,
                tile_bytes,
            }
            .sort_child(less, varying)
        },
        || {
            Sorter::<T, E, CACHED> {
                scratch: &mut Scratch::new(),
                extractor: &mut right_extractor,
                tile_bytes,
            }
            .sort_child(greater, varying);
        },
    );
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
    fn preparation_preserves_backends() {
        struct Extractor {
            confirmation: usize,
            insertion: usize,
        }
        impl Extract<u64> for Extractor {
            type Value = u64;

            fn key(&mut self, record: &u64) -> u64 {
                *record
            }

            fn confirm_ascending(&mut self, records: &[u64], previous: u64) -> bool {
                self.confirmation += 1;
                records.first().is_none_or(|&first| previous <= first)
                    && records.windows(2).all(|pair| pair[0] <= pair[1])
            }

            fn insertion_sort(&mut self, records: &mut [u64], budget: usize) -> Option<usize> {
                self.insertion += 1;
                Direct.insertion_sort(records, budget)
            }
        }

        let records: Vec<_> = (0..8192).collect();
        let mut extractor = Extractor {
            confirmation: 0,
            insertion: 0,
        };
        assert!(matches!(
            probe_order(&records, &mut RootExtractor(&mut extractor)),
            Order::Ascending
        ));
        let mut inverted = [2, 1];
        assert_eq!(
            RootExtractor(&mut extractor).insertion_sort(&mut inverted, 1),
            Some(1)
        );
        assert_eq!(inverted, [1, 2]);
        assert_eq!(extractor.confirmation, 1);
        assert_eq!(extractor.insertion, 1);
    }
}
