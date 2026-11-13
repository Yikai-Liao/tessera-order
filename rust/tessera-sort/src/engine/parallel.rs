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

    let scratch = &mut scratch;
    // Keep the existing narrow kernels when only one half of a 128-bit key varies.
    if E::Value::BITS > 64 {
        if varying >> 64 == 0 {
            Sorter::<T, _> {
                scratch,
                extractor: &mut KeyHalf::<_, 0>(extractor),
                tile_len,
            }
            .sort_root(records, varying);
            return;
        }
        if varying & ((1u128 << 64) - 1) == 0 {
            Sorter::<T, _> {
                scratch,
                extractor: &mut KeyHalf::<_, 64>(extractor),
                tile_len,
            }
            .sort_root(records, varying >> 64);
            return;
        }
    }
    Sorter::<T, E> {
        scratch,
        extractor: &mut extractor,
        tile_len,
    }
    .sort_root(records, varying);
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

impl<T: Send, E: Extract<T> + Clone + Sync> Sorter<'_, T, E> {
    /// Partitions the root and finishes each bucket in its movement task.
    fn sort_root(&mut self, records: &mut [T], varying: u128)
    where
        E: Send,
    {
        let node_bytes = records.len().saturating_mul(core::mem::size_of::<T>());
        if records.len() < 4 * self.tile_len {
            self.sort_node(records, varying);
            return;
        }
        let shards = rayon::current_num_threads().min((node_bytes / MIN_TASK_BYTES).max(1));
        match plan_node(records, varying, self.tile_len, self.extractor) {
            NodePlan::Equal(equal) => {
                let rank = Rank::new(self.extractor.clone(), equal, varying);
                let split = Split::equality(&rank, varying);
                self.partition_root(records, rank, split, shards);
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
                self.partition_root(
                    records,
                    KeyDigit::new(self.extractor.clone(), digit.shift),
                    split,
                    shards,
                );
            }
            plan => self.execute_node(records, varying, plan),
        }
    }

    /// Partitions the root by `classifier` across `shards` and finishes each
    /// unfinished bucket in the task that completed its movement.
    fn partition_root<C: Classify<T> + Clone + Send>(
        &mut self,
        records: &mut [T],
        classifier: C,
        split: Split,
        shards: usize,
    ) {
        let pool = Pool::new();
        let extractor = &*self.extractor;
        let tile_len = self.tile_len;
        let visit = |bucket: usize, _range: core::ops::Range<usize>, child: &mut [T]| {
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
            &mut self.scratch.block,
            shards,
            &visit,
        );
    }
}

#[cfg(test)]
mod bucket_borrow_tests;
