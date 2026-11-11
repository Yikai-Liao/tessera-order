// SPDX-License-Identifier: BSD-2-Clause

//! Parallel block permutation followed by the existing squeeze.
//!
//! Long permutation cycles are split into segments. Their boundary blocks are
//! snapshotted before writes, so segments are independent even within one cycle.
//! Short cycles share a worker-local temporary. No initialized references to T
//! are formed: partial blocks contain uninitialized padding.

use super::*;

struct Addresses<T>(Vec<*mut T>);

// SAFETY: the immutable table only stores addresses. The movement plan assigns
// each physical destination to exactly one job; sources in other jobs are
// snapshotted before execution. T is transferred, never concurrently borrowed.
unsafe impl<T: Send> Sync for Addresses<T> {}

impl<T> Addresses<T> {
    fn at(&self, physical: u32) -> *mut T {
        // SAFETY: all IDs were validated while building cycles, before movement.
        unsafe { *self.0.get_unchecked(physical as usize) }
    }
}

struct Piece {
    blocks: Vec<u32>,
    // None means a complete short cycle using the worker-local temporary.
    final_snapshot: Option<usize>,
}

struct Worker<T> {
    pieces: Vec<Piece>,
    temp: Vec<MaybeUninit<T>>,
    load: usize,
    completed: usize,
}

struct Snapshots<T>(Vec<MaybeUninit<T>>);

// SAFETY: during execution snapshots are read exclusively as raw bytes, once
// for each non-overlapping destination. No shared references to T are formed.
unsafe impl<T: Send> Sync for Snapshots<T> {}

impl<T> Snapshots<T> {
    fn at(&self, offset: usize) -> *const T {
        // SAFETY: construction reserves a complete block at each stored offset.
        unsafe { self.0.as_ptr().add(offset).cast() }
    }
}

fn cells<T>(len: usize) -> Vec<MaybeUninit<T>> {
    let mut result = Vec::with_capacity(len);
    result.resize_with(len, MaybeUninit::uninit);
    result
}

pub(super) fn permute_blocks<T: Send>(
    sorter: &mut Partition<'_, '_, T>,
    workers: usize,
    parallel: bool,
) {
    permute_with_fault(sorter, workers, parallel, None);
}

fn permute_with_fault<T: Send>(
    sorter: &mut Partition<'_, '_, T>,
    workers: usize,
    parallel: bool,
    fail_after: Option<usize>,
) {
    let count = sorter.block_order.len();
    let head = sorter.active_blocks.start;
    let block_size = sorter.blocks.block_size;
    let workers = workers.max(1).min(count);
    // Each split cycle has at least cap blocks. There are at most count/cap
    // such cycles and at most 2*count/cap segment boundaries, hence <= 8Q
    // temporary boundary blocks, plus Q worker blocks. Metadata is O(count).
    let cap = count.div_ceil(workers.saturating_mul(4)).max(2);
    let mut visited = alloc::vec![false; count];
    let mut jobs: Vec<_> = (0..workers)
        .map(|_| Worker {
            pieces: Vec::new(),
            temp: cells(block_size),
            load: 0,
            completed: 0,
        })
        .collect();
    let mut boundaries = Vec::new();
    let mut put = |piece: Piece| {
        let worker = jobs.iter_mut().min_by_key(|worker| worker.load).unwrap();
        worker.load += piece.blocks.len();
        worker.pieces.push(piece);
    };
    for start in 0..count {
        if visited[start] {
            continue;
        }
        let mut cycle = Vec::new();
        let mut physical = start;
        while !visited[physical] {
            visited[physical] = true;
            cycle.push(physical as u32);
            // Target physical ID for logical i is (i + head) mod count.
            let logical = if physical >= head {
                physical - head
            } else {
                count - (head - physical)
            };
            physical = sorter.block_order[logical] as usize;
        }
        assert_eq!(physical, start);
        if cycle.len() <= 1 {
            continue;
        }
        if cycle.len() <= cap {
            put(Piece {
                blocks: cycle,
                final_snapshot: None,
            });
        } else {
            let base = boundaries.len();
            let pieces = cycle.len().div_ceil(cap);
            for chunk in cycle.chunks(cap) {
                boundaries.push(chunk[0]);
            }
            for (index, chunk) in cycle.chunks(cap).enumerate() {
                put(Piece {
                    blocks: chunk.to_vec(),
                    final_snapshot: Some((base + (index + 1) % pieces) * block_size),
                });
            }
        }
    }
    let mut snapshots = Snapshots(cells(boundaries.len().checked_mul(block_size).unwrap()));
    // Calling block_ptr repeatedly must not create fresh overlapping T borrows.
    // It returns pointers from the original input ancestor or Vec allocations.
    let addresses = Addresses((0..count).map(|id| sorter.blocks.block_ptr(id)).collect());
    if parallel {
        let _ = rayon::current_num_threads();
    }
    for (index, physical) in boundaries.into_iter().enumerate() {
        // SAFETY: full physical blocks and disjoint snapshot blocks are valid
        // allocations of B cells. Raw copies preserve uninitialized padding.
        unsafe {
            ptr::copy_nonoverlapping(
                addresses.at(physical),
                snapshots.0.as_mut_ptr().add(index * block_size).cast(),
                block_size,
            );
        }
    }
    let completed_pieces = std::sync::atomic::AtomicUsize::new(0);
    let run = |job: &mut Worker<T>| {
        while job.completed < job.pieces.len() {
            // SAFETY: completed is checked above and every planned piece is nonempty.
            let piece = unsafe { job.pieces.get_unchecked(job.completed) };
            // SAFETY: every piece comes from a nonempty cycle chunk.
            let first = unsafe { *piece.blocks.get_unchecked(0) };
            let final_source = if let Some(offset) = piece.final_snapshot {
                snapshots.at(offset)
            } else {
                // SAFETY: the short cycle is wholly owned by this worker and
                // its temporary is disjoint from every physical block.
                unsafe {
                    ptr::copy_nonoverlapping(
                        addresses.at(first),
                        job.temp.as_mut_ptr().cast(),
                        block_size,
                    );
                }
                job.temp.as_ptr().cast()
            };
            for pair in piece.blocks.windows(2) {
                // SAFETY: distinct cycle nodes identify disjoint full blocks;
                // no other worker reads or writes either node except boundary
                // sources, which were copied before this phase.
                unsafe {
                    ptr::copy_nonoverlapping(
                        addresses.at(*pair.get_unchecked(1)),
                        addresses.at(*pair.get_unchecked(0)),
                        block_size,
                    );
                }
            }
            // SAFETY: the final source is either this worker's temporary or a
            // pre-staged boundary. It is disjoint from the final destination.
            unsafe {
                ptr::copy_nonoverlapping(
                    final_source,
                    addresses.at(*piece.blocks.get_unchecked(piece.blocks.len() - 1)),
                    block_size,
                );
            }
            job.completed += 1;
            // Injection is strictly between pieces, after ownership reaches a
            // recoverable boundary. The monotonically increasing count makes
            // it one-shot, including during serial completion after unwind.
            if let Some(fail_after) = fail_after {
                let completed = completed_pieces.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                assert_ne!(
                    completed, fail_after,
                    "injected scheduling panic between pieces"
                );
            }
        }
    };
    // The movement kernel has no allocation, user callbacks, or panicking
    // operations. A scheduler panic can only leave whole pieces pending. Rayon
    // joins existing jobs before returning; finish pending work serially using
    // preallocated state, then restore the input before resuming the panic.
    let result = unwind::catch(|| {
        if parallel {
            jobs.par_iter_mut().for_each(run);
        } else {
            jobs.iter_mut().for_each(&run);
        }
    });
    if result.is_err() {
        jobs.iter_mut().for_each(run);
    }
    for (logical, physical) in sorter.block_order.iter_mut().enumerate() {
        *physical = if logical >= count - head {
            logical - (count - head)
        } else {
            logical + head
        } as u32;
    }
    if let Err(payload) = result {
        sorter.compact();
        unwind::resume(payload);
    }
}

/// One input prefix, split between its original address and a saved suffix.
struct PackingSource {
    physical: u32,
    len: usize,
    saved_from: usize,
    saved_offset: usize,
}

/// Whole buckets with disjoint final destinations and protected source suffixes.
struct PackingSegment<T> {
    sources: Vec<PackingSource>,
    saved: Vec<MaybeUninit<T>>,
    start: usize,
    ends: Vec<usize>,
    completed: bool,
    panic: Option<unwind::Payload>,
}

/// Packs already-permuted blocks in independent, bucket-aligned segments.
///
/// Later segments write at or above this segment's final end. Save every live
/// source suffix they can overwrite before any writes start; earlier segments
/// end before this segment's first source. Packing then only moves left within
/// each segment. Complete pending segments without callbacks after a scheduler
/// panic, before retiring the directory guard or propagating a key panic.
pub(super) fn pack_buckets<T: Send>(
    sorter: &mut Partition<'_, '_, T>,
    ends: &[usize],
    workers: usize,
    parallel: bool,
    finish: Option<BucketSort<'_, T>>,
) {
    pack_with_fault(sorter, ends, workers, parallel, finish, None);
}

fn pack_with_fault<T: Send>(
    sorter: &mut Partition<'_, '_, T>,
    ends: &[usize],
    workers: usize,
    parallel: bool,
    finish: Option<BucketSort<'_, T>>,
    fail_after: Option<usize>,
) {
    let block_size = sorter.blocks.block_size;
    let count = sorter.block_order.len();
    let input_id = count as u32;
    let mut addresses = Addresses((0..count).map(|id| sorter.blocks.block_ptr(id)).collect());
    addresses.0.push(sorter.blocks.input);
    let workers = workers.max(1).min(ends.len().max(1));
    let target = sorter.blocks.input_len.div_ceil(workers);
    let mut segments = Vec::with_capacity(workers);
    let mut first_bucket = 0;
    let mut start = 0;
    let mut logical = sorter.active_blocks.start;
    let mut partial = 0;
    for (bucket, &end) in ends.iter().enumerate() {
        // Keep trailing empty buckets in the last populated segment.
        if (end - start < target || end == sorter.blocks.input_len) && bucket + 1 != ends.len() {
            continue;
        }
        let mut segment = PackingSegment {
            sources: Vec::new(),
            saved: Vec::new(),
            start,
            ends: ends[first_bucket..=bucket].to_vec(),
            completed: false,
            panic: None,
        };
        while logical < sorter.buffers.counts[bucket] {
            let len = sorter.next_block_len(logical, &mut partial);
            let physical = sorter.block_order[logical];
            let saved_from = if physical as usize >= sorter.blocks.scratch_blocks {
                let position = (physical as usize - sorter.blocks.scratch_blocks) * block_size;
                end.saturating_sub(position).min(len)
            } else {
                len
            };
            let saved_offset = segment.saved.len();
            segment
                .saved
                .resize_with(saved_offset + len - saved_from, MaybeUninit::uninit);
            // SAFETY: this live source prefix and the reserved suffix buffer
            // are disjoint. These snapshots stay stale until packing begins;
            // no input or directory ownership has changed if allocation fails.
            unsafe {
                ptr::copy_nonoverlapping(
                    addresses.at(physical).add(saved_from),
                    segment.saved.as_mut_ptr().add(saved_offset).cast(),
                    len - saved_from,
                );
            }
            segment.sources.push(PackingSource {
                physical,
                len,
                saved_from,
                saved_offset,
            });
            logical += 1;
        }
        segments.push(segment);
        first_bucket = bucket + 1;
        start = end;
    }
    assert_eq!(logical, sorter.active_blocks.end);
    let completed = std::sync::atomic::AtomicUsize::new(0);
    let run = |segment: &mut PackingSegment<T>| {
        pack_segment(segment, &addresses, input_id, finish);
        if let Some(fail_after) = fail_after {
            let position = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            assert_ne!(
                position, fail_after,
                "injected panic between packing segments"
            );
        }
    };
    let result = unwind::catch(|| {
        if parallel {
            segments.par_iter_mut().for_each(run);
        } else {
            segments.iter_mut().for_each(run);
        }
    });
    if result.is_err() {
        for segment in &mut segments {
            pack_segment(segment, &addresses, input_id, None);
        }
    }
    sorter.input_materialized = true;
    if let Err(payload) = result {
        unwind::resume(payload);
    }
    if let Some(payload) = segments.iter_mut().find_map(|segment| segment.panic.take()) {
        unwind::resume(payload);
    }
}

fn pack_segment<T>(
    segment: &mut PackingSegment<T>,
    addresses: &Addresses<T>,
    input_id: u32,
    finish: Option<BucketSort<'_, T>>,
) {
    if segment.completed {
        return;
    }
    let input = addresses.at(input_id);
    let mut written = segment.start;
    for source in &segment.sources {
        // SAFETY: the unsaved prefix ends before every later segment's
        // destination. Earlier segments cannot reach this source. Forward
        // packing never overtakes unread sources in this segment. The saved
        // suffix is private and the final destination bands are disjoint.
        unsafe {
            ptr::copy(
                addresses.at(source.physical),
                input.add(written),
                source.saved_from,
            );
            ptr::copy_nonoverlapping(
                segment.saved.as_ptr().add(source.saved_offset).cast::<T>(),
                input.add(written + source.saved_from),
                source.len - source.saved_from,
            );
        }
        written += source.len;
    }
    segment.completed = true;
    if let Some(finish) = finish {
        let mut start = segment.start;
        for &end in &segment.ends {
            if end > start {
                // SAFETY: the complete bucket owns its final band. Other
                // segments use only disjoint destinations or saved sources;
                // neither packing nor recovery reads this band again.
                let records =
                    unsafe { core::slice::from_raw_parts_mut(input.add(start), end - start) };
                if let Err(payload) = unwind::catch(|| finish(records)) {
                    segment.panic = Some(payload);
                    return;
                }
            }
            start = end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct Owned {
        key: usize,
        id: usize,
        text: String,
        drops: Arc<Vec<AtomicUsize>>,
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            assert_eq!(self.text, self.id.to_string());
            self.drops[self.id].fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn packing_callbacks_reenter_with_exclusive_records() {
        use crate::key::ByKey;
        use core::cell::Cell;
        for len in [17usize, 97] {
            for block_size in [1, 3, 8] {
                let mut values: Vec<_> = (0..len as u32).map(Cell::new).collect();
                let mut records: Vec<_> = values
                    .iter_mut()
                    .map(|value| (value.get().wrapping_mul(7919) % 256, value))
                    .collect();
                let mut expected: Vec<_> = records
                    .iter()
                    .map(|record| (record.0, record.1.get()))
                    .collect();
                expected.sort_by_key(|record| record.0);
                let callbacks = AtomicUsize::new(0);
                partition_impl_with_visit(
                    &mut records,
                    3,
                    5,
                    ByKey(|record: &(u32, &mut Cell<u32>)| record.0),
                    &mut Buffers::new(),
                    Config {
                        shards: 3,
                        block_size: Some(block_size),
                        parallel: !cfg!(miri),
                        finish: Some(BoundedFinish {
                            limit: 0,
                            sort: &|records| {
                                callbacks.fetch_add(1, Ordering::Relaxed);
                                if !cfg!(miri) {
                                    let (left, right) = rayon::join(|| 13 * 7, || 13 * 7);
                                    assert_eq!(left, right);
                                }
                                records.sort_by_key(|record| record.0);
                            },
                        }),
                    },
                    Some(&mut |_| panic!("packing callback unexpectedly bypassed")),
                );
                assert!(callbacks.load(Ordering::Relaxed) > 0);
                assert_eq!(
                    records
                        .iter()
                        .map(|record| (record.0, record.1.get()))
                        .collect::<Vec<_>>(),
                    expected
                );
                for (_, value) in records {
                    value.set(value.get() + 1);
                }
                assert_eq!(
                    values.iter().map(Cell::get).collect::<Vec<_>>(),
                    (1..=len as u32).collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn packing_scheduler_panic_restores_owned_and_borrowed_records() {
        fn pack<T: Send>(
            records: &mut [T],
            classify: impl FnMut(&T) -> usize,
            fail: Option<usize>,
        ) {
            let mut buffers = Buffers::new();
            let mut partition = Partition::new(records, 4, 3, &mut buffers);
            assert!(partition.step(classify).is_none());
            let ends = partition.record_ends();
            permute_blocks(&mut partition, 3, !cfg!(miri));
            pack_with_fault(&mut partition, &ends, 3, !cfg!(miri), None, fail);
        }
        let len = if cfg!(miri) { 23 } else { 113 };
        let mut expected: Vec<_> = (0..len).collect();
        expected.sort_by_key(|id| id * 37 % 4);
        for fail in [None, Some(0), Some(1)] {
            let drops = Arc::new((0..len).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
            let mut records: Vec<_> = (0..len)
                .map(|id| Owned {
                    key: id * 37 % 4,
                    id,
                    text: id.to_string(),
                    drops: drops.clone(),
                })
                .collect();
            let result = unwind::catch(|| pack(&mut records, |record| record.key, fail));
            assert_eq!(result.is_err(), fail.is_some());
            assert_eq!(
                records.iter().map(|record| record.id).collect::<Vec<_>>(),
                expected
            );
            assert!(drops.iter().all(|count| count.load(Ordering::Relaxed) == 0));
            drop(records);
            assert!(drops.iter().all(|count| count.load(Ordering::Relaxed) == 1));

            let mut backing: Vec<_> = (0..len).collect();
            let mut records: Vec<_> = backing.iter_mut().collect();
            let result = unwind::catch(|| pack(&mut records, |record| **record * 37 % 4, fail));
            assert_eq!(result.is_err(), fail.is_some());
            assert_eq!(
                records.iter().map(|record| **record).collect::<Vec<_>>(),
                expected
            );
            for record in records {
                *record += 1;
            }
            assert_eq!(backing, (1..=len).collect::<Vec<_>>());
        }
    }

    #[test]
    fn short_cycles_keep_tail() {
        // F=9, H=2 gives 13 physical blocks and cap=2 for Q=4.
        // A seven-node cycle splits 2+2+2+1; the next two nodes
        // form a short cycle. The final live block has padding.
        let mut rows: Vec<_> = (0..29).collect();
        let mut expected = Vec::new();
        for block in [1, 2, 3, 4, 5, 6, 0, 8, 7] {
            expected.extend(block * 3..block * 3 + 3);
        }
        expected.extend(27..29);
        {
            let mut buffers = Buffers::new();
            let mut sorter = Partition::new(&mut rows, 2, 3, &mut buffers);
            sorter.block_order[2..9].rotate_left(1);
            sorter.block_order.swap(9, 10);
            permute_blocks(&mut sorter, 4, !cfg!(miri));
            sorter.compact();
        }
        assert_eq!(rows, expected);
    }

    #[test]
    fn permutation_panic_restores_records() {
        let n = if cfg!(miri) { 23 } else { 113 };
        for fail in [None, Some(0), Some(1)] {
            let drops = Arc::new((0..n).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
            let mut rows: Vec<_> = (0..n)
                .map(|id| Owned {
                    key: id * 37 % 4,
                    id,
                    text: id.to_string(),
                    drops: drops.clone(),
                })
                .collect();
            let result = unwind::catch(|| {
                let mut buffers = Buffers::new();
                let mut sorter = Partition::new(&mut rows, 4, 3, &mut buffers);
                assert!(sorter.step(|row| row.key).is_none());
                permute_with_fault(&mut sorter, 4, !cfg!(miri), fail);
                sorter.compact();
            });
            assert_eq!(result.is_err(), fail.is_some());
            let mut expected: Vec<_> = (0..n).collect();
            expected.sort_by_key(|id| id * 37 % 4);
            assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), expected);
            assert!(drops.iter().all(|d| d.load(Ordering::Relaxed) == 0));
            drop(rows);
            assert!(drops.iter().all(|d| d.load(Ordering::Relaxed) == 1));

            let mut backing: Vec<_> = (0..n).collect();
            let mut refs: Vec<_> = backing.iter_mut().collect();
            let result = unwind::catch(|| {
                let mut buffers = Buffers::new();
                let mut sorter = Partition::new(&mut refs, 4, 3, &mut buffers);
                assert!(sorter.step(|row| **row * 37 % 4).is_none());
                permute_with_fault(&mut sorter, 4, !cfg!(miri), fail);
                sorter.compact();
            });
            assert_eq!(result.is_err(), fail.is_some());
            assert_eq!(refs.iter().map(|r| **r).collect::<Vec<_>>(), expected);
            for record in refs {
                *record += 1;
            }
            assert_eq!(backing, (1..=n).collect::<Vec<_>>());
        }
    }
}
