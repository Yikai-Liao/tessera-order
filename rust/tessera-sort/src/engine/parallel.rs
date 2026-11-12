// SPDX-License-Identifier: BSD-2-Clause

//! Parallel root partition and completion of materialized buckets.

use super::*;
use rayon::prelude::*;

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
    let mut plan = plan_node(records, varying, tile_len, extractor);
    if let NodePlan::Equal(equal) = plan {
        // The same rank digit as serial equality partitions.
        let rank = Rank {
            extractor: extractor.clone(),
            equal,
        };
        let ends = block::partition_digit_parallel(
            records,
            2,
            0,
            rank,
            &mut scratch.block,
            shards,
            block::BucketFinish {
                limit: 0,
                scale_blocks: false,
                sort: &|_| {},
                visit: &mut |_| {},
            },
        );
        match plan_after_equality(records, &ends, varying, tile_len, extractor) {
            Some(progress) => {
                // The node is unchanged; continue with the bit-consuming plan.
                plan = progress;
            }
            None => {
                // Materialization has retired all copy sources. Skip the equal
                // band; the unused fourth bucket leaves the greater band through
                // the end.
                let (less, rest) = records.split_at_mut(ends[0]);
                let (_, greater) = rest.split_at_mut(ends[1] - ends[0]);
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
        }
    }
    let digit = match plan {
        NodePlan::Digit(digit) => digit,
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
    // Finished buckets need no child sorting; direct keys count them in parallel.
    if candidate == 0 && key::try_count_window_parallel::<T, E>(records, digit.shift, digit.width) {
        return;
    }

    let pool = Pool::new();
    let child_extractor = extractor.clone();
    let visit = |child: &mut [T]| {
        if candidate == 0 {
            return;
        }
        // Movement retired every source and released its locks before lending
        // this final band. Finish in that task instead of handing it to a second
        // queue. P1 experiment: it uses the staged allowance.
        let mut key = child_extractor.clone();
        let mut scratch = pool.take();
        Sorter::<T, E> {
            scratch: &mut scratch,
            extractor: &mut key,
            tile_len,
        }
        .sort_staged_child(child, candidate);
        pool.give(scratch);
    };
    let finish = |child: &mut [T]| {
        if candidate == 0 {
            return;
        }
        let mut key = child_extractor.clone();
        let mut scratch = pool.take();
        Sorter::<T, E> {
            scratch: &mut scratch,
            extractor: &mut key,
            tile_len,
        }
        .sort_staged_child(child, candidate);
        pool.give(scratch);
    };
    // P1 experiment: keep the block scale that staging eligibility selects,
    // but stream every final band instead of staging it.
    let scale_blocks = bucket_limit::<T>(records.len(), digit.width) > 0;
    let limit = 0;
    block::partition_digit_parallel(
        records,
        digit.width,
        digit.shift,
        extractor.clone(),
        &mut scratch.block,
        shards,
        block::BucketFinish {
            limit,
            scale_blocks,
            sort: &finish,
            visit: &visit,
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

#[cfg(test)]
mod bucket_borrow_tests;
