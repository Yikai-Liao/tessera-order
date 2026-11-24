// SPDX-License-Identifier: BSD-2-Clause

//! Parallel root partition and completion of materialized buckets.

use super::*;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use rayon::prelude::*;

/// The archived experiment did not reliably win at 512 KiB; keep 1 MiB as a
/// starting cutoff until controlled measurements retune it.
const MIN_PARALLEL_BYTES: usize = 1 << 20;
/// Nearby sizes showed no reliable win in controlled 10M random A/B screens.
const MIN_TASK_BYTES: usize = 256 * 1024;
/// Tuned serial prefix checked before waking Rayon for root confirmation or
/// variation scans, matching one `PARALLEL_SCAN_BLOCK` so early inversions and
/// near-start outliers exit deterministically without waking worker threads.
const PARALLEL_VARYING_PREFIX: usize = 4096;
/// Sub-chunk scanned between shared early-exit checks across Rayon shards.
const PARALLEL_SCAN_BLOCK: usize = 4096;

/// Summary of an ordered sub-slice: `(first, last, asc, desc, strict)`.
///
/// - `asc` holds iff every adjacent pair satisfies `a <= b`.
/// - `desc` holds iff every adjacent pair satisfies `a >= b`.
/// - `strict` holds iff every adjacent pair satisfies `a != b`; only a
///   descending run with `strict == true` may reverse in place without stable
///   tie repair.
///
/// A constant run (`len >= 2`) has `(true, true, false)` so it can extend
/// either direction without authorizing strict reversal; a singleton has
/// `(true, true, true)` because it contains no internal tie, and any equal
/// boundary at a join clears `strict` while preserving `desc`.
type OrderRun<V> = (V, V, bool, bool, bool);

fn merge_order<V: Unsigned>(l: OrderRun<V>, r: OrderRun<V>) -> Option<OrderRun<V>> {
    let next = (
        l.0,
        r.1,
        l.2 && r.2 && l.1 <= r.0,
        l.3 && r.3 && l.1 >= r.0,
        l.4 && r.4 && l.1 != r.0,
    );
    (next.2 || next.3).then_some(next)
}

fn order_summary<T, E: Extract<T>>(chunk: &[T], key: &mut E) -> Option<OrderRun<E::Value>> {
    let (first, last) = (key.key(&chunk[0]), key.key(&chunk[chunk.len() - 1]));
    match confirm_order(chunk, key) {
        Order::Ascending => Some((first, last, true, first == last, chunk.len() < 2)),
        Order::Descending { strict } => Some((first, last, false, true, strict)),
        Order::Flat | Order::Unsorted => None,
    }
}

/// Confirms order across disjoint shards without allocating heap scratch.
fn confirm_order_parallel<T: Send, E: Extract<T> + Clone + Sync>(
    records: &mut [T],
    extractor: &E,
) -> Order {
    let prefix = records.len().min(PARALLEL_VARYING_PREFIX);
    let mut head_key = extractor.clone();
    let Some(head) = order_summary(&records[..prefix], &mut head_key) else {
        return Order::Unsorted;
    };
    let summary = if prefix == records.len() {
        Some(head)
    } else {
        // Reject a direction-breaking drop at the prefix boundary before waking Rayon.
        let next = head_key.key(&records[prefix]);
        if merge_order(head, (next, next, true, true, true)).is_none() {
            return Order::Unsorted;
        }
        let aborted = AtomicBool::new(false);
        // Mutable blocks need only Send; no shared record borrow crosses tasks.
        records[prefix..]
            .par_chunks_mut(PARALLEL_SCAN_BLOCK)
            .map(|sub| {
                if aborted.load(Ordering::Relaxed) {
                    return None;
                }
                // Reject blocks whose direction contradicts the confirmed prefix
                // so peer blocks cancel without waiting for the final join.
                let run = order_summary(sub, &mut extractor.clone())
                    .filter(|r| (head.2 && r.2) || (head.3 && r.3));
                if run.is_none() {
                    aborted.store(true, Ordering::Relaxed);
                }
                run
            })
            .reduce_with(|l, r| {
                let merged = l.zip(r).and_then(|(l, r)| merge_order(l, r));
                if merged.is_none() {
                    aborted.store(true, Ordering::Relaxed);
                }
                merged
            })
            .flatten()
            .and_then(|rest| merge_order(head, rest))
    };
    match summary {
        Some((_, _, true, _, _)) => Order::Ascending,
        Some((_, _, _, true, strict)) => Order::Descending { strict },
        _ => Order::Unsorted,
    }
}

/// Finishes confirmed order on a parallel root, reversing strictly descending
/// inputs across shards without allocating.
fn finish_ordered_parallel<T: Send, E: Extract<T>>(
    records: &mut [T],
    order: Order,
    extractor: &mut E,
) -> bool {
    let Order::Descending { strict: true } = order else {
        return finish_ordered(records, order, extractor);
    };
    let (half, skip) = (records.len() / 2, records.len() & 1);
    let (left, right) = records.split_at_mut(half);
    let chunk = half.div_ceil(rayon::current_num_threads());
    left.par_chunks_mut(chunk)
        .zip(right[skip..].par_rchunks_mut(chunk))
        .for_each(|(l, r)| {
            l.iter_mut()
                .zip(r.iter_mut().rev())
                .for_each(|(a, b)| core::mem::swap(a, b));
        });
    true
}

/// Scans a short serial prefix for full-width exit, then reduces remaining
/// varying bits across Rayon shards without heap allocation.
fn scan_varying_bits_parallel<T: Send, E: Extract<T> + Clone + Sync>(
    records: &mut [T],
    candidate: u128,
    extractor: &mut E,
) -> u128 {
    let prefix = records.len().min(PARALLEL_VARYING_PREFIX);
    let seen = scan_varying_bits(&records[..prefix], candidate, extractor);
    if seen == candidate || prefix == records.len() {
        return seen;
    }
    let first = extractor.key(&records[0]).to_u128();
    // Monotonic shared bit-set used only to skip already-witnessed bits.
    // The two Relaxed loads need not observe the same instant: missing a newly
    // published bit only causes extra work, and every bit published into
    // `lo`/`hi` is also returned by its publishing block into the `.reduce` OR
    // that owns the final variation mask, so a skipped block returning 0 never
    // drops a varying bit.
    let (lo, hi) = (
        AtomicU64::new(seen as u64),
        AtomicU64::new((seen >> 64) as u64),
    );
    let rest = records[prefix..]
        .par_chunks_mut(PARALLEL_SCAN_BLOCK)
        .map(|sub| {
            let known =
                lo.load(Ordering::Relaxed) as u128 | ((hi.load(Ordering::Relaxed) as u128) << 64);
            if known & candidate == candidate {
                return 0;
            }
            let mut key = extractor.clone();
            let head = key.key(&sub[0]).to_u128() ^ first;
            let found = head | scan_varying_bits(sub, candidate & !(known | head), &mut key);
            if found & !known != 0 {
                lo.fetch_or(found as u64, Ordering::Relaxed);
                hi.fetch_or((found >> 64) as u64, Ordering::Relaxed);
            }
            found
        })
        .reduce(|| 0, |a, b| a | b);
    (seen | rest) & candidate
}

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
    let mut scratch = Workspace::new();
    let required = Requirement::All;
    let Some(varying) = prepare_with(
        records,
        &mut extractor,
        tile_len,
        |r, e| sample_order(r, e).unwrap_or_else(|| confirm_order_parallel(r, e)),
        |r, e| confirm_order_parallel(r, e),
        finish_ordered_parallel,
        scan_varying_bits_parallel,
    ) else {
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
            .sort_root(records, varying, required);
            return;
        }
        if varying & ((1u128 << 64) - 1) == 0 {
            Sorter::<T, _> {
                scratch,
                extractor: &mut KeyHalf::<_, 64>(extractor),
                tile_len,
            }
            .sort_root(records, varying >> 64, required);
            return;
        }
    }
    Sorter::<T, E> {
        scratch,
        extractor: &mut extractor,
        tile_len,
    }
    .sort_root(records, varying, required);
}

/// Recycles scratch between bucket finishers without locking during user code.
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
            .unwrap_or_else(Workspace::new)
    }

    fn give(&self, scratch: Workspace<T>) {
        self.free
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(scratch);
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
        let pool = Pool::new();
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
            let mut scratch = pool.take();
            Sorter::<T, E> {
                scratch: &mut scratch,
                extractor: &mut key,
                tile_len: tile::finishing_capacity::<T>(child.len(), tile_len),
            }
            .sort_child(child, split.candidate, required);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::key::ByKey;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    #[cfg_attr(
        miri,
        ignore = "parallel root scan fixture uses multi-thread Rayon pool"
    )]
    fn saturated_suffix_anchor_skips_interior_scan() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();
        let mut records: alloc::vec::Vec<(usize, u64)> = (0..131_073)
            .map(|i| {
                (
                    i,
                    if i < PARALLEL_VARYING_PREFIX {
                        0
                    } else {
                        u64::MAX
                    },
                )
            })
            .collect();
        let interior_calls = AtomicUsize::new(0);
        let varying = pool.install(|| {
            scan_varying_bits_parallel(
                &mut records,
                u64::MAX as u128,
                &mut ByKey(|&(idx, v): &(usize, u64)| {
                    if idx >= PARALLEL_VARYING_PREFIX
                        && !(idx - PARALLEL_VARYING_PREFIX).is_multiple_of(PARALLEL_SCAN_BLOCK)
                    {
                        interior_calls.fetch_add(1, Ordering::Relaxed);
                    }
                    v
                }),
            )
        });
        assert_eq!(varying, u64::MAX as u128);
        assert_eq!(
            interior_calls.load(Ordering::Relaxed),
            0,
            "saturated suffix block anchor must not read any block interior key"
        );

        // When the serial prefix covers the whole ordered root, confirm_order_parallel
        // returns immediately without re-evaluating the slice.
        let mut short_asc: alloc::vec::Vec<u64> = (0..PARALLEL_VARYING_PREFIX as u64).collect();
        let short_calls = AtomicUsize::new(0);
        let order = pool.install(|| {
            confirm_order_parallel(
                &mut short_asc,
                &ByKey(|v: &u64| {
                    short_calls.fetch_add(1, Ordering::Relaxed);
                    *v
                }),
            )
        });
        assert!(order == Order::Ascending);
        assert!(
            short_calls.load(Ordering::Relaxed) <= PARALLEL_VARYING_PREFIX + 2,
            "prefix-covered ordered root should not be scanned twice"
        );

        // A direction-breaking drop right at the prefix boundary rejects
        // confirmation before scanning any suffix block.
        let mut boundary_drop: alloc::vec::Vec<u64> = (0..262_144u64).collect();
        boundary_drop[PARALLEL_VARYING_PREFIX] = (PARALLEL_VARYING_PREFIX - 2) as u64;
        let drop_calls = AtomicUsize::new(0);
        let drop_order = pool.install(|| {
            confirm_order_parallel(
                &mut boundary_drop,
                &ByKey(|v: &u64| {
                    drop_calls.fetch_add(1, Ordering::Relaxed);
                    *v
                }),
            )
        });
        assert!(drop_order == Order::Unsorted);
        assert!(
            drop_calls.load(Ordering::Relaxed) <= PARALLEL_VARYING_PREFIX + 4,
            "prefix-boundary drop should reject before scanning the suffix"
        );

        // An ascending prefix followed by a drop inside the first suffix block
        // (0..=4096 then 4095 repeated) contradicts the prefix direction and
        // cancels remaining constant blocks.
        let mut suffix_drop: alloc::vec::Vec<u64> = (0..262_144)
            .map(|i| {
                if i <= PARALLEL_VARYING_PREFIX {
                    i as u64
                } else {
                    (PARALLEL_VARYING_PREFIX - 1) as u64
                }
            })
            .collect();
        let suffix_drop_calls = AtomicUsize::new(0);
        let suffix_drop_order = pool.install(|| {
            confirm_order_parallel(
                &mut suffix_drop,
                &ByKey(|v: &u64| {
                    suffix_drop_calls.fetch_add(1, Ordering::Relaxed);
                    *v
                }),
            )
        });
        assert!(suffix_drop_order == Order::Unsorted);
        assert!(
            suffix_drop_calls.load(Ordering::Relaxed) < suffix_drop.len() / 2,
            "suffix block contradicting prefix direction should cancel peer blocks"
        );

        // A tie right across the prefix boundary prevents strict reversal and
        // preserves equal-key payload order.
        let mut boundary_tie: alloc::vec::Vec<(usize, u64)> = (0..131_073)
            .map(|i| {
                let key = if i == PARALLEL_VARYING_PREFIX {
                    (200_000 - (PARALLEL_VARYING_PREFIX - 1)) as u64
                } else {
                    (200_000 - i) as u64
                };
                (i, key)
            })
            .collect();
        let tie_order = pool
            .install(|| confirm_order_parallel(&mut boundary_tie, &ByKey(|r: &(usize, u64)| r.1)));
        assert!(tie_order == Order::Descending { strict: false });
        pool.install(|| crate::par_sort_by_key(&mut boundary_tie, |r| r.1));
        let tied: alloc::vec::Vec<usize> = boundary_tie
            .iter()
            .filter(|r| r.1 == (200_000 - (PARALLEL_VARYING_PREFIX - 1)) as u64)
            .map(|r| r.0)
            .collect();
        assert_eq!(
            tied,
            alloc::vec![PARALLEL_VARYING_PREFIX - 1, PARALLEL_VARYING_PREFIX]
        );
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "parallel root scan fixture uses multi-thread Rayon pool"
    )]
    fn early_inversion_and_sparse_outlier_stop_peer_shards() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();
        let len = 262_144usize;

        // Swapped prefix must reject order confirmation before touching other shards.
        let mut swapped: alloc::vec::Vec<u64> = (0..len as u64).collect();
        swapped.swap(0, 1);
        let confirm_calls = AtomicUsize::new(0);
        let order = pool.install(|| {
            confirm_order_parallel(
                &mut swapped,
                &ByKey(|v: &u64| {
                    confirm_calls.fetch_add(1, Ordering::Relaxed);
                    *v
                }),
            )
        });
        assert!(order == Order::Unsorted);
        assert!(
            confirm_calls.load(Ordering::Relaxed) < PARALLEL_VARYING_PREFIX,
            "early inversion should reject before scanning peer shards"
        );

        // Near-start outlier at index 257 is caught deterministically in the
        // serial prefix without waking peer shards.
        let mut near_outlier = alloc::vec![0u64; len];
        near_outlier[257] = u64::MAX;
        let near_calls = AtomicUsize::new(0);
        let near_varying = pool.install(|| {
            scan_varying_bits_parallel(
                &mut near_outlier,
                u64::MAX as u128,
                &mut ByKey(|v: &u64| {
                    near_calls.fetch_add(1, Ordering::Relaxed);
                    *v
                }),
            )
        });
        assert_eq!(near_varying, u64::MAX as u128);
        assert!(
            near_calls.load(Ordering::Relaxed) < PARALLEL_VARYING_PREFIX,
            "near-start full-width outlier should exit inside the serial prefix"
        );

        // Full-width outlier just beyond the serial prefix stops constant peer blocks.
        let mut sparse = alloc::vec![0u64; len];
        sparse[PARALLEL_VARYING_PREFIX + 1] = u64::MAX;
        let scan_calls = AtomicUsize::new(0);
        let varying = pool.install(|| {
            scan_varying_bits_parallel(
                &mut sparse,
                u64::MAX as u128,
                &mut ByKey(|v: &u64| {
                    scan_calls.fetch_add(1, Ordering::Relaxed);
                    *v
                }),
            )
        });
        assert_eq!(varying, u64::MAX as u128);
        assert!(
            scan_calls.load(Ordering::Relaxed) < len / 4,
            "sparse full-width outlier should stop constant peer shards early"
        );

        // High-half bit in the prefix must not suppress a low-half bit in a
        // suffix block when loading the shifted `hi` atomic half.
        let mut wide = alloc::vec![0u128; len];
        wide[1] = 1u128 << 64;
        wide[PARALLEL_VARYING_PREFIX + 2] = 1u128;
        assert_eq!(
            pool.install(|| scan_varying_bits_parallel(&mut wide, u128::MAX, &mut Direct)),
            (1u128 << 64) | 1u128
        );

        // Constant input still returns 0, and a late varying bit still propagates.
        let mut constant = alloc::vec![7u64; len];
        assert_eq!(
            pool.install(|| scan_varying_bits_parallel(
                &mut constant,
                u64::MAX as u128,
                &mut Direct
            )),
            0
        );
        constant[len - 1] = 7 ^ (1u64 << 63);
        assert_eq!(
            pool.install(|| scan_varying_bits_parallel(
                &mut constant,
                u64::MAX as u128,
                &mut Direct
            )),
            1u128 << 63
        );
    }
}
