// SPDX-License-Identifier: BSD-2-Clause

//! Intrinsic unsigned counting with independently owned output-rank intervals.

use super::Unsigned;
use alloc::vec;
use core::mem::size_of;
use rayon::prelude::*;

// Account for both parallel iterator trees, their closures and scalar frames.
// There are at most P leaves and P-1 internal nodes per tree, and the passes do
// not overlap. No per-task arrays, records, or user callbacks live on the stack.
// Existing Rayon worker stacks and pool bookkeeping are not auxiliary storage
// allocated by this algorithm. This conservative allowance is 4 KiB per leaf.
const TASK_BYTES: usize = 4096;
const RECORD_FACTOR: usize = 8;
const MIN_CHUNK: usize = 4096;

#[derive(Debug)]
struct Plan {
    buckets: usize,
    chunk: usize,
    workers: usize,
    cells: usize,
}

fn plan(n: usize, workers: usize, width: usize, record_bytes: usize) -> Option<Plan> {
    if workers < 2 || width == 0 || width > 8 || record_bytes == 0 || workers > n / MIN_CHUNK {
        return None;
    }
    let buckets = 1usize.checked_shl(width as u32)?;
    let cells = workers.checked_mul(buckets)?;
    let table_bytes = cells.checked_mul(size_of::<usize>())?;
    let prefix_bytes = buckets.checked_add(1)?.checked_mul(size_of::<usize>())?;
    let scratch = table_bytes
        .checked_add(prefix_bytes)?
        .checked_add(workers.checked_mul(TASK_BYTES)?)?;
    let root = n.isqrt();
    let ceiling_root = root.checked_add(usize::from(root * root != n))?;
    let budget = RECORD_FACTOR
        .checked_mul(workers)?
        .checked_mul(ceiling_root)?
        .checked_mul(record_bytes)?;
    if scratch > budget {
        return None;
    }
    Some(Plan {
        buckets,
        chunk: n.div_ceil(workers),
        workers,
        cells,
    })
}

fn count<K: Unsigned>(records: &[K], counts: &mut [usize], low: usize) {
    let mask = K::from_usize(counts.len() - 1);
    let shift = K::from_usize(low);
    for &record in records {
        counts[((record >> shift) & mask).to_usize()] += 1;
    }
}

fn prefix(counts: &[usize], buckets: usize, ends: &mut [usize]) {
    for row in counts.chunks_exact(buckets) {
        for (end, &count) in ends[1..].iter_mut().zip(row) {
            *end += count;
        }
    }
    for bucket in 0..buckets {
        ends[bucket + 1] += ends[bucket];
    }
}

fn fill<K: Unsigned>(records: &mut [K], start: usize, ends: &[usize], base: K, low: usize) {
    let mut rank = start;
    let mut bucket = ends[1..].partition_point(|&end| end <= rank);
    let mut remaining = records;
    while !remaining.is_empty() {
        let amount = (ends[bucket + 1] - rank).min(remaining.len());
        let (run, rest) = remaining.split_at_mut(amount);
        let mut value = base;
        value |= K::from_usize(bucket) << K::from_usize(low);
        run.fill(value);
        rank += amount;
        remaining = rest;
        bucket += 1;
    }
}

// Only sealed u32/u64 implementations call this function. Every other bit is
// known constant from prepare_sort, so replacing a run preserves exact values.
pub(super) fn sort<K: Unsigned + Send + Sync>(records: &mut [K], low: usize, width: usize) -> bool {
    if low.checked_add(width).is_none_or(|top| top > K::BITS) {
        return false;
    }
    let Some(plan) = plan(
        records.len(),
        rayon::current_num_threads(),
        width,
        size_of::<K>(),
    ) else {
        return false;
    };
    let mut counts = vec![0usize; plan.cells];
    let mut ends = vec![0usize; plan.buckets + 1];
    // vec![0; len] requests exactly len elements; neither vector grows. The
    // table contains P rows even when ceil(n/P) yields fewer nonempty chunks.
    debug_assert_eq!(counts.capacity(), plan.cells);
    debug_assert_eq!(ends.capacity(), plan.buckets + 1);
    debug_assert!(records.len().div_ceil(plan.chunk) <= plan.workers);
    #[cfg(test)]
    let hooks = tests::witness::current();
    #[cfg(test)]
    if let Some(hooks) = &hooks {
        *hooks.count_layout.lock().unwrap() =
            Some((counts.as_ptr().addr() % 64, plan.workers, plan.buckets));
    }
    let count_chunk = |(row, chunk): (&mut [usize], &mut [K])| {
        count(chunk, row, low);
        #[cfg(test)]
        record_work(&hooks, false, chunk.len());
    };
    counts
        .par_chunks_mut(plan.buckets)
        .zip(records.par_chunks_mut(plan.chunk))
        .for_each(count_chunk);
    prefix(&counts, plan.buckets, &mut ends);
    debug_assert_eq!(ends[plan.buckets], records.len());
    let mask = K::from_usize(plan.buckets - 1) << K::from_usize(low);
    let base = records[0] & !mask;
    let fill_chunk = |(index, chunk): (usize, &mut [K])| {
        fill(chunk, index * plan.chunk, &ends, base, low);
        #[cfg(test)]
        record_work(&hooks, true, chunk.len());
    };
    records
        .par_chunks_mut(plan.chunk)
        .enumerate()
        .for_each(fill_chunk);
    #[cfg(test)]
    if let Some(hooks) = tests::witness::current() {
        hooks
            .parallel_count
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    true
}

#[cfg(test)]
fn record_work(hooks: &Option<std::sync::Arc<tests::witness::Hooks>>, fill: bool, len: usize) {
    if let Some(hooks) = hooks {
        hooks
            .count_work
            .lock()
            .unwrap()
            .push((fill, rayon::current_thread_index().unwrap(), len));
    }
}

#[cfg(test)]
mod tests;
