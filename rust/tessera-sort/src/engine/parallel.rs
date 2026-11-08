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
pub(crate) fn sort<T: Send, E: Extract<T> + Clone + Send, const CACHED: bool>(
    records: &mut [T],
    extractor: &mut E,
) {
    let record_bytes = core::mem::size_of::<T>();
    let bytes = records.len().saturating_mul(record_bytes);
    if records.len() < 2 || record_bytes == 0 {
        return;
    }
    if bytes < MIN_PARALLEL_BYTES || rayon::current_num_threads() == 1 {
        super::sort::<T, E, CACHED>(records, extractor, None);
        return;
    }

    let tile_bytes = TILE_BYTES
        .max(record_bytes)
        .min(MAX_TILE_RECORDS.saturating_mul(record_bytes));
    let mut workspace = Workspace::new();
    let Some(varying) = prepare(records, extractor, &mut workspace, tile_bytes) else {
        return;
    };

    // Keep the existing narrow kernels when only one half of a 128-bit key varies.
    if E::Value::BITS > 64 {
        if varying >> 64 == 0 {
            let mut key = Narrow::<E, 0>(extractor.clone());
            sort_root::<T, _, CACHED>(records, &mut key, &mut workspace, varying, tile_bytes);
            return;
        }
        if varying & ((1u128 << 64) - 1) == 0 {
            let mut key = Narrow::<E, 64>(extractor.clone());
            sort_root::<T, _, CACHED>(records, &mut key, &mut workspace, varying >> 64, tile_bytes);
            return;
        }
    }
    sort_root::<T, E, CACHED>(records, extractor, &mut workspace, varying, tile_bytes);
}

/// Keeps the serial engine's adaptive root checks without changing its call path.
fn prepare<T, E: Extract<T>>(
    records: &mut [T],
    extractor: &mut E,
    workspace: &mut Workspace<T>,
    tile_bytes: usize,
) -> Option<u128> {
    let record_bytes = core::mem::size_of::<T>();
    let sampled = probe_order(records, extractor);
    match sampled {
        Order::Ascending => return None,
        Order::Descending => {
            reverse_stably(records, extractor);
            return None;
        }
        Order::Unsorted => {
            let probe_len = (tile_bytes / record_bytes).max(2).min(records.len());
            if insertion_probe(records, extractor, probe_len, Some(&mut workspace.probe)) {
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
        match confirm_order(records, extractor) {
            Order::Ascending => return None,
            Order::Descending => {
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
}

/// One bucket pointer with the provenance of its own materialized slice.
struct Bucket<T> {
    start: *mut T,
    len: usize,
}

/// A batch of disjoint root buckets, each kept as its own borrowed region.
///
/// The pointers live only inside the Rayon scope that borrows the input. A
/// bucket is handed off after compaction has materialized it and retired it as
/// a copy source; the partition never accesses that region again.
struct Job<T> {
    records: usize,
    buckets: Vec<Bucket<T>>,
}

// SAFETY: each pointer names one complete, exclusively owned bucket from the
// input. The block partition transfers it once, no job overlaps another, and
// the enclosing Rayon scope joins every job before the input borrow ends.
unsafe impl<T: Send> Send for Job<T> {}

impl<T> Job<T> {
    fn new() -> Self {
        Self {
            records: 0,
            buckets: Vec::new(),
        }
    }

    fn push(&mut self, bucket: &mut [T]) {
        self.records += bucket.len();
        self.buckets.push(Bucket {
            start: bucket.as_mut_ptr(),
            len: bucket.len(),
        });
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
        let mut workspace = pool.take();
        for bucket in &self.buckets {
            // SAFETY: this exact pointer and length came from one exclusive
            // materialized bucket. It is disjoint from the other stored slices,
            // and compaction no longer reads or writes any of them.
            let child = unsafe { core::slice::from_raw_parts_mut(bucket.start, bucket.len) };
            if child.len() > 1 {
                let varying = scan_varying_bits(child, candidate, &mut extractor);
                Sorter::<T, E, CACHED> {
                    workspace: &mut workspace,
                    extractor: &mut extractor,
                    tile_bytes,
                }
                .sort_node(child, varying);
            }
        }
        pool.give(workspace);
    }
}

/// Recycles workspaces between jobs without holding the lock during user code.
struct Pool<T> {
    free: std::sync::Mutex<Vec<Workspace<T>>>,
}

impl<T> Pool<T> {
    fn new() -> Self {
        Self {
            free: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn take(&self) -> Workspace<T> {
        self.free
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .pop()
            .unwrap_or_default()
    }

    fn give(&self, workspace: Workspace<T>) {
        self.free
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(workspace);
    }
}

/// Partitions the root and sorts each bucket as soon as compaction releases it.
fn sort_root<T: Send, E: Extract<T> + Clone + Send, const CACHED: bool>(
    records: &mut [T],
    extractor: &mut E,
    workspace: &mut Workspace<T>,
    varying: u128,
    tile_bytes: usize,
) {
    let node_bytes = records.len().saturating_mul(core::mem::size_of::<T>());
    let low = varying.trailing_zeros() as usize;
    let top = bit_len(varying);
    if node_bytes <= tile_bytes || node_bytes < 4 * tile_bytes {
        Sorter::<T, E, CACHED> {
            workspace,
            extractor,
            tile_bytes,
        }
        .sort_node(records, varying);
        return;
    }
    let shards = rayon::current_num_threads().min((node_bytes / MIN_TASK_BYTES).max(1));
    if top - low <= SPLIT_BITS {
        if extractor.try_count_window(records, low, top - low) {
            return;
        }
        let ends = block::partition_digit_parallel_with_visit(
            records,
            top - low,
            low,
            extractor.clone(),
            workspace.block_buffers(),
            shards,
            &mut |_| {},
        );
        workspace
            .block
            .as_mut()
            .expect("partition buffers restored")
            .recycle_ends(ends);
        return;
    }

    let mut keys = [KeyValue::<T, E>::from_usize(0); MODE_SAMPLE_LEN];
    sample_keys(records, &mut keys, extractor);
    let bit = KeyValue::<T, E>::from_usize(1) << KeyValue::<T, E>::from_usize(top - 1);
    if sample_mode(&mut keys).is_some() || outlier_run(records, &keys, bit, extractor).is_some() {
        Sorter::<T, E, CACHED> {
            workspace,
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
            workspace.block_buffers(),
            shards,
            &mut |_| {},
        );
        workspace
            .block
            .as_mut()
            .expect("partition buffers restored")
            .recycle_ends(ends);
        return;
    }

    let pool = Pool::new();
    let pool_ref = &pool;
    let root_extractor = extractor.clone();
    let child_extractor = extractor.clone();
    rayon::scope(move |scope| {
        let mut pending: Option<Job<T>> = None;
        let mut visit = |bucket: &mut [T]| {
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
        let ends = block::partition_digit_parallel_with_visit(
            records,
            digit.width,
            digit.shift,
            root_extractor,
            workspace.block_buffers(),
            shards,
            &mut visit,
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
        workspace
            .block
            .as_mut()
            .expect("partition buffers restored")
            .recycle_ends(ends);
    });
}

/// Queues a group of complete buckets on the current Rayon scope.
fn spawn_job<
    'scope,
    T: Send + 'scope,
    E: Extract<T> + Clone + Send + 'scope,
    const CACHED: bool,
>(
    scope: &Scope<'scope>,
    mut job: Job<T>,
    candidate: u128,
    extractor: E,
    tile_bytes: usize,
    pool: &'scope Pool<T>,
) {
    scope.spawn(move |_| job.run::<E, CACHED>(extractor, candidate, tile_bytes, pool));
}

#[cfg(test)]
mod bucket_borrow_tests;
