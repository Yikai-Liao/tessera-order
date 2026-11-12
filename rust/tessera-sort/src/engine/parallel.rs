// SPDX-License-Identifier: BSD-2-Clause

//! Parallel root partition and completion of materialized buckets.

use super::*;

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

/// Recycles scratch between bucket finishers without locking during user code.
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

/// Partitions the root and finishes each bucket in its movement task.
fn sort_root<T: Send, E: Extract<T> + Clone + Send + Sync>(
    records: &mut [T],
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
    match plan_node(records, varying, tile_len, extractor) {
        NodePlan::Equal(equal) => {
            let rank = Rank::new(extractor.clone(), equal, varying);
            let split = Split::equality(&rank, varying);
            partition_root(records, rank, split, extractor, scratch, shards, tile_len);
        }
        NodePlan::Digit(digit) => {
            let candidate = varying & !digit.mask::<E::Value>().to_u128();
            // Finished buckets need no child sorting; direct keys count them in parallel.
            if candidate == 0
                && key::try_count_window_parallel::<T, E>(records, digit.shift, digit.width)
            {
                return;
            }
            let split = Split {
                radix: digit.width,
                candidate,
                finished: None,
            };
            partition_root(
                records,
                KeyDigit::new(extractor.clone(), digit.shift),
                split,
                extractor,
                scratch,
                shards,
                tile_len,
            );
        }
        plan => Sorter::<T, E> {
            scratch,
            extractor,
            tile_len,
        }
        .execute_node(records, varying, plan),
    }
}

/// Partitions the root by `classifier` across `shards` and finishes each
/// unfinished bucket with `extractor` in the task that completed its movement.
fn partition_root<T: Send, C: Classify<T> + Clone + Send, E: Extract<T> + Clone + Sync>(
    records: &mut [T],
    classifier: C,
    split: Split,
    extractor: &E,
    scratch: &mut Scratch<T>,
    shards: usize,
    tile_len: usize,
) {
    let pool = Pool::new();
    let visit = |bucket: usize, child: &mut [T]| {
        if split.candidate == 0 || split.finished == Some(bucket) {
            return;
        }
        // Movement retired every source and released its locks before lending
        // this final band. Finish in that task instead of handing it to a second
        // queue. Tile storage owns whether the whole child fits one tile.
        let mut key = extractor.clone();
        let mut scratch = pool.take();
        Sorter::<T, E> {
            scratch: &mut scratch,
            extractor: &mut key,
            tile_len: tile::finishing_capacity::<T>(child.len(), tile_len),
        }
        .sort_child(child, split.candidate);
        pool.give(scratch);
    };
    block::partition_parallel(
        records,
        split.radix,
        classifier,
        &mut scratch.block,
        shards,
        &visit,
    );
}

#[cfg(test)]
mod bucket_borrow_tests;
