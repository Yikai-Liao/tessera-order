// SPDX-License-Identifier: BSD-2-Clause

//! Parallel root completion.
//!
//! A large root runs the serial preparation and split policy, then distributes
//! the work of one split: the block scatter of a classifier spreads over
//! disjoint shards, an in-place window fill counts in parallel, and the buckets
//! the split produces are sorted on the caller's Rayon pool. Each bucket is a
//! contiguous slice of the input once compaction finishes, so the same serial
//! recursion sorts it. Small roots and one-thread pools never leave the serial
//! engine.

use super::*;
use crate::key::Narrowed;
use rayon::prelude::*;

/// Smallest root worth distributing: below sixteen tiles the serial engine wins.
const MIN_PARALLEL_BYTES: usize = 16 * TILE_BYTES;

/// Bytes one worker receives before a wider shard count pays off.
const MIN_SHARD_BYTES: usize = 4 * TILE_BYTES;

/// Sorts a large root by distributing the buckets of one split.
pub(crate) fn sort<T, E>(records: &mut [T], extractor: &mut E)
where
    T: Send,
    E: Extract<T> + Clone + Send,
{
    let record_bytes = core::mem::size_of::<T>();
    if records.len() < 2 || record_bytes == 0 {
        return;
    }
    let bytes = records.len().saturating_mul(record_bytes);
    if bytes < MIN_PARALLEL_BYTES || rayon::current_num_threads() == 1 {
        return super::sort(records, extractor);
    }
    let tile_bytes = TILE_BYTES
        .max(record_bytes)
        .min(MAX_TILE_RECORDS.saturating_mul(record_bytes));
    let Some(varying) = prepare(records, u128::MAX, extractor) else {
        return;
    };
    if adaptive(records, tile_bytes, extractor) {
        return;
    }
    if E::Value::BITS > 64 {
        if varying >> 64 == 0 {
            return sort_narrowed::<T, E, 0>(records, extractor, varying, tile_bytes);
        }
        if varying & u64::MAX as u128 == 0 {
            return sort_narrowed::<T, E, 64>(records, extractor, varying >> 64, tile_bytes);
        }
    }
    sort_varying(records, extractor, varying, tile_bytes);
}

/// Continues a large root on the 64-bit half of its key that varies.
fn sort_narrowed<T, E, const SHIFT: usize>(
    records: &mut [T],
    extractor: &E,
    varying: u128,
    tile_bytes: usize,
) where
    T: Send,
    E: Extract<T> + Clone + Send,
{
    let mut narrow = Narrowed::<E, SHIFT>(extractor.clone());
    sort_varying(records, &mut narrow, varying, tile_bytes);
}

/// Distributes the buckets of one root split on the current pool.
fn sort_varying<T, E>(records: &mut [T], extractor: &mut E, varying: u128, tile_bytes: usize)
where
    T: Send,
    E: Extract<T> + Clone + Send,
{
    let bytes = records.len().saturating_mul(core::mem::size_of::<T>());
    let low = varying.trailing_zeros() as usize;
    let span = bit_len(varying) - low;
    // One window over the whole span finishes a narrow root: counts come from
    // disjoint shards and the runs are then filled in parallel.
    if span <= FILL_BITS {
        let digit = Digit {
            shift: low,
            width: span,
        };
        if extractor.fill_window_parallel(records, digit) {
            return;
        }
    }
    let shards = rayon::current_num_threads().min((bytes / MIN_SHARD_BYTES).max(1));
    let mut scratch = Scratch::new();
    let mut ends = Vec::new();
    let choice = choose_node(records, varying, &mut scratch.planner, extractor);
    let (ranges, candidate) = match choice.route {
        // A slice that already holds its two runs needs no pass at all: one
        // rotation and two scans finish it, so there is nothing to distribute.
        cost::Route::Rotate => {
            let top_bit = 1u128 << (bit_len(varying) - 1);
            let (boundary, front_set) =
                two_runs(records, top_bit, extractor).expect("rotation was priced from this slice");
            Sorter {
                scratch: &mut scratch,
                extractor: &mut *extractor,
                tile_bytes,
            }
            .rotate(records, varying, boundary, front_set);
            return;
        }
        cost::Route::Pivot => {
            let pivot = sample_pivot(records, extractor).expect("pivot was priced from this slice");
            let classify = move |extractor: &mut E, record: &T| {
                let key = extractor.key(record);
                usize::from(key >= pivot) + usize::from(key > pivot)
            };
            block::partition_parallel(
                records,
                4,
                classify,
                extractor.clone(),
                &mut scratch.block,
                shards,
                &mut ends,
            );
            let (below, equal_end) = (ends[0], ends[1]);
            if below == records.len() || equal_end == below {
                // A changing key left the whole slice in one band; a digit
                // partition removes candidate bits instead of repeating it.
                let digit = choose_node(records, varying, &mut scratch.planner, extractor)
                    .route
                    .digit()
                    .expect("a node route falls back to a split");
                partition_root(records, digit, extractor, &mut scratch, shards, &mut ends);
                (
                    ranges_from(&ends),
                    varying & !digit.mask::<E::Value>().to_u128(),
                )
            } else {
                // The equal band is finished; both outer bands keep every bit.
                let mut ranges = Vec::with_capacity(2);
                for (start, end) in [(0, below), (equal_end, records.len())] {
                    if end > start + 1 {
                        ranges.push((start, end));
                    }
                }
                (ranges, varying)
            }
        }
        cost::Route::Split { digit, .. } => {
            partition_root(records, digit, extractor, &mut scratch, shards, &mut ends);
            (
                ranges_from(&ends),
                varying & !digit.mask::<E::Value>().to_u128(),
            )
        }
        cost::Route::Flat { .. } => unreachable!("a root range never finishes flat"),
    };
    sort_ranges(records, &ranges, candidate, extractor, tile_bytes);
}

/// Partitions a root range by one digit window through the sharded block engine.
fn partition_root<T, E>(
    records: &mut [T],
    digit: Digit,
    extractor: &E,
    scratch: &mut Scratch<T>,
    shards: usize,
    ends: &mut Vec<usize>,
) where
    T: Send,
    E: Extract<T> + Clone + Send,
{
    let classify = |extractor: &mut E, record: &T| digit.classify(extractor, record);
    block::partition_parallel(
        records,
        1 << digit.width,
        classify,
        extractor.clone(),
        &mut scratch.block,
        shards,
        ends,
    );
}
/// The ranges worth sorting out of a set of bucket ends.
fn ranges_from(ends: &[usize]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::with_capacity(ends.len());
    let mut start = 0;
    for &end in ends {
        if end > start + 1 {
            ranges.push((start, end));
        }
        start = end;
    }
    ranges
}

/// Sorts disjoint ranges of `records` on the current pool.
fn sort_ranges<T, E>(
    records: &mut [T],
    ranges: &[(usize, usize)],
    candidate: u128,
    extractor: &E,
    tile_bytes: usize,
) where
    T: Send,
    E: Extract<T> + Clone + Send,
{
    if ranges.is_empty() {
        return;
    }
    let pool = Pool::new();
    let mut jobs = Vec::with_capacity(ranges.len());
    let mut rest = records;
    let mut position = 0;
    for &(start, end) in ranges {
        let (_, tail) = rest.split_at_mut(start - position);
        let (child, tail) = tail.split_at_mut(end - start);
        rest = tail;
        position = end;
        jobs.push((child, extractor.clone()));
    }
    // Miri interprets the same handoff without Rayon's scheduler.
    if cfg!(miri) {
        for (child, mut extractor) in jobs {
            let mut scratch = pool.take();
            Sorter {
                scratch: &mut scratch,
                extractor: &mut extractor,
                tile_bytes,
            }
            .sort_slice(child, candidate);
            pool.give(scratch);
        }
        return;
    }
    let pool = &pool;
    jobs.into_par_iter().for_each(|(child, mut extractor)| {
        let mut scratch = pool.take();
        Sorter {
            scratch: &mut scratch,
            extractor: &mut extractor,
            tile_bytes,
        }
        .sort_slice(child, candidate);
        pool.give(scratch);
    });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Direct;

    #[test]
    fn bucket_handoff_sorts_disjoint_ranges() {
        // Sequential ownership fixture for the pool handoff: every range gets
        // its own scratch and extractor, and no range reaches its neighbours.
        let mut records: Vec<u64> = (0..if cfg!(miri) { 40 } else { 400 })
            .map(|id| (id * 37) % 64)
            .collect();
        let ranges = [
            (0, records.len() / 2),
            (records.len() / 2 + 3, records.len()),
        ];
        // Every range sorts on its own: the records between ranges are not part
        // of any job and keep their place.
        let mut expected = records.clone();
        for &(start, end) in &ranges {
            expected[start..end].sort_unstable();
        }
        sort_ranges(&mut records, &ranges, u128::from(u64::MAX), &Direct, 64);
        assert_eq!(records, expected);
    }
}
