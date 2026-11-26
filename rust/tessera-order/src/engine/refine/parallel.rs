// SPDX-License-Identifier: BSD-2-Clause

//! Parallel root partition and completion of materialized buckets.

use super::*;
use crate::engine::key::{ByKey, RadixKey};
use rayon::prelude::*;

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

/// Packs key prefixes into spare index bits, sorts through the parallel root,
/// then unmasks indices and refines remaining prefix ties in parallel.
pub(crate) fn argsort<T: Sync, K: RadixKey>(
    values: &[T],
    key: impl Fn(&T) -> K + Sync,
) -> Vec<usize> {
    let tile_len = tile_capacity::<usize>(TILE_BYTES);
    let len = values.len();
    assert!(len <= isize::MAX as usize / size_of::<usize>());
    let bytes = len.saturating_mul(size_of::<T>().max(size_of::<usize>()));
    if len < 4 * tile_len || bytes < MIN_PARALLEL_BYTES || rayon::current_num_threads() == 1 {
        return super::argsort(values, key);
    }
    let indexed = ByKey(|i: &usize| key(&values[*i]));
    let mut extract = ByKey(&key);
    let sampled = probe_order(values, &mut extract);
    let varying = match resolve_varying(values, sampled, K::VARYING_MASK, &mut extract) {
        Ok(varying) => varying,
        Err(order) => {
            let mut out: Vec<usize> = (0..len).into_par_iter().collect();
            finish_ordered(&mut out, order, &mut indexed.clone());
            return out;
        }
    };
    let index_bits = bit_len((len - 1) as u128);
    // `len <= isize::MAX / size_of::<usize>()` leaves at least two spare bits.
    let spare_bits = usize::BITS as usize - index_bits;
    let index_mask = usize::MAX >> spare_bits;
    let high = bit_len(varying);
    let prefix_bits = (high - varying.trailing_zeros() as usize).min(spare_bits);
    let prefix_shift = high - prefix_bits;
    let prefix_mask = (1usize << prefix_bits) - 1;
    let remaining = varying & ((1u128 << prefix_shift) - 1);
    let mut words: Vec<usize> = values
        .par_iter()
        .enumerate()
        .map(|(i, r)| {
            (((key(r).encode().to_u128() >> prefix_shift) as usize & prefix_mask) << index_bits) | i
        })
        .collect();
    Sorter {
        scratch: &mut Workspace::new(),
        extractor: &mut ByKey(move |word: &usize| *word >> index_bits),
        tile_len,
    }
    .sort_root(&mut words, varying >> prefix_shift, Requirement::All);
    if remaining == 0 {
        words.par_iter_mut().for_each(|word| *word &= index_mask);
        return words;
    }
    let chunk = len.div_ceil(rayon::current_num_threads()).max(1);
    let (mut bands, mut huge) = (Vec::new(), Vec::new());
    let mut rest = words.as_mut_slice();
    while rest.len() > chunk {
        let prefix = rest[chunk - 1] >> index_bits;
        let end = chunk + rest[chunk..].partition_point(|w| *w >> index_bits == prefix);
        let start = rest[..chunk].partition_point(|w| *w >> index_bits < prefix);
        let (head, tail) = rest.split_at_mut(end);
        let (before, tie) = head.split_at_mut(if end - start >= chunk { start } else { end });
        bands.push(before);
        if !tie.is_empty() {
            huge.push(tie);
        }
        rest = tail;
    }
    bands.push(rest);
    bands.into_par_iter().for_each(|band| {
        let (mut scratch, mut extract) = (Workspace::new(), indexed.clone());
        let mut sorter = Sorter {
            scratch: &mut scratch,
            extractor: &mut extract,
            tile_len,
        };
        for ties in band.chunk_by_mut(|a, b| (*a >> index_bits) == (*b >> index_bits)) {
            ties.iter_mut().for_each(|w| *w &= index_mask);
            if ties.len() > 1 {
                sorter.sort_child(ties, remaining, Requirement::All);
            }
        }
    });
    let (mut scratch, mut extract) = (Workspace::new(), indexed);
    for ties in huge {
        let sub = unmask_and_scan_varying(ties, index_mask, remaining, values, &key);
        if sub != 0 {
            Sorter {
                scratch: &mut scratch,
                extractor: &mut extract,
                tile_len,
            }
            .sort_root(ties, sub, Requirement::All);
        }
    }
    words
}

/// Unmasks a large tie band in parallel while scanning its remaining varying
/// bits. Each of at most `P` chunks merges observed 1-bits and 0-bits (masked
/// to `remaining`) into shared monotonic atomics at entry and at bounded batch
/// intervals (64 growing to `MAX_SCAN_BATCH`). Once global `(ones & zeros)`
/// reaches `remaining`—whether within one chunk or across chunks—active
/// workers stop reading keys at the end of their current batch (at most
/// `MAX_SCAN_BATCH` extra key reads per peer worker) and only unmask slots.
fn unmask_and_scan_varying<T: Sync, K: RadixKey>(
    ties: &mut [usize],
    index_mask: usize,
    remaining: u128,
    values: &[T],
    key: &(impl Fn(&T) -> K + Sync),
) -> u128 {
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    const MAX_SCAN_BATCH: usize = 4096;

    let shared_ones = [AtomicU64::new(0), AtomicU64::new(0)];
    let shared_zeros = [AtomicU64::new(0), AtomicU64::new(0)];
    let load_pair = |a: &[AtomicU64; 2]| {
        u128::from(a[0].load(Relaxed)) | (u128::from(a[1].load(Relaxed)) << 64)
    };
    let merge_pair = |a: &[AtomicU64; 2], new_bits: u128| {
        let (lo, hi) = (new_bits as u64, (new_bits >> 64) as u64);
        let rlo = if lo != 0 {
            a[0].fetch_or(lo, Relaxed) | lo
        } else {
            a[0].load(Relaxed)
        };
        let rhi = if hi != 0 {
            a[1].fetch_or(hi, Relaxed) | hi
        } else {
            a[1].load(Relaxed)
        };
        u128::from(rlo) | (u128::from(rhi) << 64)
    };

    let chunk_len = ties.len().div_ceil(rayon::current_num_threads()).max(1);
    ties.par_chunks_mut(chunk_len).for_each(|chunk| {
        if (load_pair(&shared_ones) & load_pair(&shared_zeros)) == remaining {
            for slot in chunk {
                *slot &= index_mask;
            }
            return;
        }
        let (first_slot, rest) = chunk.split_first_mut().unwrap();
        *first_slot &= index_mask;
        let first = key(&values[*first_slot]).encode();
        let (mut union, mut intersection) = (first, first);
        let (mut pub_ones, mut pub_zeros) = (0u128, 0u128);
        let mut publish_and_saturated = |u: u128, inter: u128| {
            let new_ones = (u & remaining) & !pub_ones;
            let new_zeros = (!inter & remaining) & !pub_zeros;
            pub_ones |= new_ones;
            pub_zeros |= new_zeros;
            (merge_pair(&shared_ones, new_ones) & merge_pair(&shared_zeros, new_zeros)) == remaining
        };
        if publish_and_saturated(first.to_u128(), first.to_u128()) {
            for slot in rest {
                *slot &= index_mask;
            }
            return;
        }
        let mut i = 0;
        let mut batch = 64;
        while i < rest.len() {
            let end = i + batch.min(rest.len() - i);
            for slot in &mut rest[i..end] {
                *slot &= index_mask;
                let k = key(&values[*slot]).encode();
                union |= k;
                intersection = intersection & k;
            }
            i = end;
            if publish_and_saturated(union.to_u128(), intersection.to_u128()) {
                for slot in &mut rest[i..] {
                    *slot &= index_mask;
                }
                break;
            }
            batch = batch.saturating_mul(2).min(MAX_SCAN_BATCH);
        }
    });
    load_pair(&shared_ones) & load_pair(&shared_zeros)
}

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
