// SPDX-License-Identifier: BSD-2-Clause

//! Parallel completion of fully materialized, disjoint radix buckets.

use super::*;

// A task needs several tiles to amortize scheduling and independent scratch.
const MIN_TASK_BYTES: usize = 256 * 1024;
// At 512 KiB, the first measured schedule did not consistently beat the
// sequential call; keep at least sixteen tiles before paying for task setup.
const MIN_PARALLEL_BYTES: usize = 1024 * 1024;

/// Selects the serial entry point before adapting a user key to a shared borrow.
#[inline]
pub(crate) fn use_serial<T>(len: usize) -> bool {
    len < 2
        || core::mem::size_of::<T>() == 0
        || len.saturating_mul(core::mem::size_of::<T>()) < MIN_PARALLEL_BYTES
        || rayon::current_num_threads() == 1
}

pub(crate) fn sort<T: Send, E: Extract<T> + Clone + Send, const CACHED: bool>(
    arr: &mut [T],
    mut key: E,
) {
    let record_bytes = core::mem::size_of::<T>();
    let bytes = arr.len().saturating_mul(record_bytes);
    if arr.len() < 2
        || record_bytes == 0
        || bytes < MIN_PARALLEL_BYTES
        || rayon::current_num_threads() == 1
    {
        super::sort::<T, E, CACHED>(arr, &mut key, None);
        return;
    }
    let tile_bytes = TILE_BYTES
        .max(record_bytes)
        .min(MAX_TILE_RECORDS.saturating_mul(record_bytes));
    let Some(varying) = prepare_sort(arr, &mut key, tile_bytes) else {
        return;
    };
    // Keep 128-bit keys on the existing 64-bit kernels when only one half varies.
    if E::Value::BITS > 64 {
        if varying >> 64 == 0 {
            sort_prepared::<T, _, CACHED>(arr, Narrow::<E, 0>(key), varying, tile_bytes);
            return;
        }
        if varying & ((1u128 << 64) - 1) == 0 {
            sort_prepared::<T, _, CACHED>(arr, Narrow::<E, 64>(key), varying >> 64, tile_bytes);
            return;
        }
    }
    sort_prepared::<T, E, CACHED>(arr, key, varying, tile_bytes);
}

#[derive(Clone)]
struct Narrow<E, const SHIFT: usize>(E);
impl<T, E: Extract<T>, const SHIFT: usize> Extract<T> for Narrow<E, SHIFT> {
    type Value = u64;
    #[inline]
    fn key(&mut self, record: &T) -> u64 {
        (self.0.key(record).to_u128() >> SHIFT) as u64
    }
}

/// A run of adjacent, fully materialized buckets handed to one task.
///
/// The compaction materializes buckets in increasing address order, so
/// consecutive buckets of a run are contiguous and a run is one slice. The run
/// owns that slice from the moment the compaction drops it: the copy never
/// writes a materialized bucket again and never reads one as a copy source.
struct Job<T> {
    start: *mut T,
    records: usize,
    /// Ends of the buckets inside the run, relative to `start` and increasing.
    bounds: Vec<usize>,
}

// SAFETY: a job owns its run exclusively. Runs come from disjoint bucket
// ranges, and the compaction hands a bucket over only once.
unsafe impl<T: Send> Send for Job<T> {}

impl<T: Send> Job<T> {
    fn new(start: *mut T) -> Self {
        Job {
            start,
            records: 0,
            bounds: Vec::new(),
        }
    }

    /// Extends the run by one materialized bucket, if it is adjacent.
    fn push(&mut self, bucket: *mut T, records: usize, record_bytes: usize) -> bool {
        // SAFETY: pointer arithmetic only; the comparison is between addresses.
        let adjacent = unsafe { std::ptr::eq(self.start.add(self.records), bucket) };
        if !adjacent {
            return false;
        }
        self.records += records;
        self.bounds.push(self.records);
        if core::mem::size_of::<T>() != 0 {
            debug_assert_eq!(record_bytes, core::mem::size_of::<T>());
        }
        true
    }

    fn bytes(&self) -> usize {
        self.records.saturating_mul(core::mem::size_of::<T>())
    }

    fn run<E: Extract<T> + Clone + Send, const CACHED: bool>(
        &mut self,
        mut key: E,
        candidate: u128,
        tile_bytes: usize,
        pool: &Pool<T>,
    ) {
        let mut workspace = pool.take();
        let mut start = 0;
        for &end in &self.bounds {
            // SAFETY: the bounds are monotonic and no larger than the run, and
            // the run is exclusively owned by this job. The slices are
            // disjoint, so no other task aliases these records.
            let child =
                unsafe { core::slice::from_raw_parts_mut(self.start.add(start), end - start) };
            if child.len() > 1 {
                let varying = scan_varying_bits(child, candidate, &mut key);
                Sorter::<T, E, CACHED> {
                    workspace: &mut workspace,
                    key: &mut key,
                    tile_bytes,
                }
                .sort_node(child, varying);
            }
            start = end;
        }
        pool.give(workspace);
    }
}

/// Recycled per-task workspaces.
///
/// A task needs its own tile and block scratch. Allocating one per run would
/// cost a 64 KiB allocation per 32K records; the pool keeps one per worker.
struct Pool<T> {
    free: std::sync::Mutex<Vec<Workspace<T>>>,
}

impl<T> Pool<T> {
    fn new() -> Self {
        Pool {
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

fn sort_prepared<T: Send, E: Extract<T> + Clone + Send, const CACHED: bool>(
    arr: &mut [T],
    mut key: E,
    varying: u128,
    tile_bytes: usize,
) {
    let mut workspace = Workspace::new();
    let low = varying.trailing_zeros() as usize;
    let top = bit_len(varying);
    // A single digit has no unfinished children. Preserve the adaptive serial
    // equality and outlier routes until there is useful independent work.
    let mut keys = [KeyValue::<T, E>::from_usize(0); MODE_SAMPLE_LEN];
    sample_keys(arr, &mut keys, &mut key);
    let equal = sample_mode(&mut keys);
    let bit = KeyValue::<T, E>::from_usize(1) << KeyValue::<T, E>::from_usize(top - 1);
    if top - low <= SPLIT_BITS
        || equal.is_some()
        || outlier_run(arr, &keys, bit, &mut key).is_some()
    {
        Sorter::<T, E, CACHED> {
            workspace: &mut workspace,
            key: &mut key,
            tile_bytes,
        }
        .sort_node(arr, varying);
        return;
    }
    let digit = choose_split_digit(arr, top, tile_bytes, &mut key);
    let candidate = varying & !digit.mask::<E::Value>().to_u128();
    if candidate == 0 {
        return;
    }
    // The compaction is the longest serial phase of a parallel call, and the
    // children are the only parallel work that does not depend on all of it:
    // it hands out each bucket as soon as it is materialized. Sorting a run of
    // materialized buckets inside that callback overlaps the two, which is
    // worth about a third of the root on large inputs. Runs are cut at
    // MIN_TASK_BYTES so a task still amortises its scratch and scheduling.
    let pool = Pool::new();
    let record_bytes = core::mem::size_of::<T>();
    rayon::scope(|scope| {
        let mut pending: Option<Job<T>> = None;
        {
            let mut visit = |bucket: &mut [T], visit_key: &mut E| {
                let start = bucket.as_mut_ptr();
                let records = bucket.len();
                let mut job = pending.take().unwrap_or_else(|| Job::new(start));
                if !job.push(start, records, record_bytes) {
                    // The compaction materializes buckets in increasing
                    // addresses, so a gap means the previous run is complete.
                    spawn_job::<T, E, CACHED>(
                        scope,
                        job,
                        candidate,
                        visit_key.clone(),
                        tile_bytes,
                        &pool,
                    );
                    job = Job::new(start);
                    assert!(job.push(start, records, record_bytes));
                }
                if job.bytes() >= MIN_TASK_BYTES {
                    spawn_job::<T, E, CACHED>(
                        scope,
                        job,
                        candidate,
                        visit_key.clone(),
                        tile_bytes,
                        &pool,
                    );
                } else {
                    pending = Some(job);
                }
            };
            block::partition_digit_with_ends(
                arr,
                digit.width,
                digit.shift,
                &mut key,
                workspace.block_buffers(),
                &mut visit,
            );
        }
        if let Some(job) = pending {
            spawn_job::<T, E, CACHED>(scope, job, candidate, key.clone(), tile_bytes, &pool);
        }
    });
}

/// Queues one materialized run of buckets on the parallel workers.
fn spawn_job<
    'scope,
    T: Send + 'scope,
    E: Extract<T> + Clone + Send + 'scope,
    const CACHED: bool,
>(
    scope: &rayon::Scope<'scope>,
    mut job: Job<T>,
    candidate: u128,
    key: E,
    tile_bytes: usize,
    pool: &'scope Pool<T>,
) {
    scope.spawn(move |_| job.run::<E, CACHED>(key, candidate, tile_bytes, pool));
}

/// Keeps the serial adaptive checks without changing the sequential entry point.
fn prepare_sort<T, E: Extract<T>>(arr: &mut [T], key: &mut E, tile_bytes: usize) -> Option<u128> {
    let record_bytes = core::mem::size_of::<T>();
    let sampled = probe_order(arr, key);
    match sampled {
        Order::Ascending => return None,
        Order::Descending => {
            reverse_stably(arr, key);
            return None;
        }
        Order::Flat | Order::Unsorted => {}
    }
    if sampled == Order::Unsorted {
        let probe = (tile_bytes / record_bytes).max(2).min(arr.len());
        let swaps = if probe == arr.len() {
            key.insertion_sort(arr, probe)
        } else {
            // No retained workspace is exposed by the parallel API, so these
            // Copy keys can use a typed temporary without raw scratch casts.
            let mut keys: Vec<_> = arr[..probe].iter().map(|record| key.key(record)).collect();
            match Direct.insertion_sort(&mut keys, probe) {
                Some(0) => None,
                Some(swaps) => key.insertion_sort(arr, swaps * arr.len() / probe),
                None => None,
            }
        };
        if swaps.is_some() {
            return None;
        }
    }
    let varying = block::varying_key_bits(arr, key);
    if varying == 0 {
        return None;
    }
    if sampled == Order::Flat {
        match confirm_order(arr, key) {
            Order::Ascending => return None,
            Order::Descending => {
                reverse_stably(arr, key);
                return None;
            }
            _ => {}
        }
    }
    Some(varying)
}
