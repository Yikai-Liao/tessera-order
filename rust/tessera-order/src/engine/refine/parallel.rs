// SPDX-License-Identifier: BSD-2-Clause

//! Parallel root partition and completion of materialized buckets.

use super::*;

/// The archived experiment did not reliably win at 512 KiB; keep 1 MiB as a
/// starting cutoff until controlled measurements retune it.
const MIN_PARALLEL_BYTES: usize = 1 << 20;
/// Nearby sizes showed no reliable win in controlled 10M random A/B screens.
const MIN_TASK_BYTES: usize = 256 * 1024;

/// Sorts large inputs by handing complete root buckets to Rayon's current pool.
pub(crate) fn sort<T: Send, E: Extract<T> + Clone + Send + Sync>(records: &mut [T], extractor: E) {
    let record_bytes = core::mem::size_of::<T>();
    let bytes = records.len().saturating_mul(record_bytes);
    if records.len() < 2 || record_bytes == 0 {
        return;
    }
    if bytes < MIN_PARALLEL_BYTES || rayon::current_num_threads() == 1 {
        super::sort::<T, E>(records, extractor);
        return;
    }
    dispatch_root(
        records,
        extractor,
        tile_capacity::<T>(TILE_BYTES),
        ParallelRoot,
    );
}

struct ParallelRoot;

impl<T: Send, E: Extract<T> + Clone + Send + Sync> RefineRoot<T, E> for ParallelRoot {
    fn refine(self, sorter: &mut Sorter<'_, T, E>, records: &mut [T], varying: u128) {
        sorter.sort_root(records, varying, Requirement::All);
    }
}

impl<T: Send, E: Extract<T> + Clone + Sync> Sorter<'_, T, E> {
    /// Partitions the root and finishes each bucket in its movement task.
    fn sort_root(&mut self, records: &mut [T], varying: u128, required: Requirement)
    where
        E: Send,
    {
        let node_bytes = records.len().saturating_mul(core::mem::size_of::<T>());
        if records.len() < 4 * self.tile_len {
            self.sort_node(records, varying, required);
            return;
        }
        let shards = rayon::current_num_threads().min((node_bytes / MIN_TASK_BYTES).max(1));
        match plan_node(records, varying, self.tile_len, self.extractor) {
            NodePlan::Equal(equal) => {
                let rank = Rank::new(self.extractor.clone(), equal, varying);
                let split = Split::equality(&rank, varying);
                self.partition_root(records, rank, split, shards, required);
            }
            NodePlan::Digit(digit) => {
                let split = Split::digit::<E::Value>(digit, varying);
                // Finished buckets need no child sorting; direct keys count them in parallel.
                if split.candidate == 0
                    && key::try_count_window::<T, E>(records, digit.shift, digit.width, true)
                {
                    return;
                }
                self.partition_root(
                    records,
                    KeyDigit::new(self.extractor.clone(), digit.shift),
                    split,
                    shards,
                    required,
                );
            }
            plan => self.execute_node(records, varying, plan, required),
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
        required: Requirement,
    ) {
        // Recycle scratch between finishers without holding the lock across user code.
        let pool = std::sync::Mutex::new(Vec::<Workspace<T>>::new());
        let lock = || {
            pool.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        };
        let extractor = &*self.extractor;
        let tile_len = self.tile_len;
        let visit = |bucket: usize, range: core::ops::Range<usize>, child: &mut [T]| {
            if split.candidate == 0 || split.finished == Some(bucket) {
                return;
            }
            let Some(required) = required.project(range) else {
                return;
            };
            // Movement retired every source and released its locks before lending
            // this final band. Finish in that task instead of handing it to a second
            // queue. Tile storage owns whether the whole child fits one tile.
            let mut key = extractor.clone();
            let mut scratch = lock().pop().unwrap_or_else(Workspace::new);
            Sorter::<T, E> {
                scratch: &mut scratch,
                extractor: &mut key,
                tile_len: tile::finishing_capacity::<T>(child.len(), tile_len),
            }
            .sort_child(child, split.candidate, required);
            lock().push(scratch);
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
