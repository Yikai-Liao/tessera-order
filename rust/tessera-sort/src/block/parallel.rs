// SPDX-License-Identifier: BSD-2-Clause

//! Parallel permutation of a completed block partition.
//!
//! The permutation is split at cycle boundaries. Long cycles are split into
//! pieces whose boundary blocks are copied to scratch before any piece runs;
//! each worker reuses one block for short cycles.

use super::*;
use rayon::prelude::*;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

struct BlockAddresses<T>(Vec<*mut T>);

// SAFETY: the table stores immutable addresses. The cycle plan gives each
// destination to one worker and snapshots every source shared between pieces.
unsafe impl<T: Send> Sync for BlockAddresses<T> {}

impl<T> BlockAddresses<T> {
    fn at(&self, physical: u32) -> *mut T {
        // SAFETY: physical IDs are validated while building cycles.
        unsafe { *self.0.get_unchecked(physical as usize) }
    }
}

struct Piece {
    blocks: Vec<u32>,
    final_snapshot: Option<usize>,
}

struct Worker<T> {
    pieces: Vec<Piece>,
    scratch: Vec<MaybeUninit<T>>,
    block_load: usize,
    completed_pieces: usize,
}

struct BlockSnapshots<T>(Vec<MaybeUninit<T>>);

// SAFETY: each snapshot is an immutable source for exactly one destination;
// block copies never form shared references to T.
unsafe impl<T: Send> Sync for BlockSnapshots<T> {}

impl<T> BlockSnapshots<T> {
    fn at(&self, offset: usize) -> *const T {
        // SAFETY: each offset reserves a complete block during planning.
        unsafe { self.0.as_ptr().add(offset).cast() }
    }
}

fn uninitialized<T>(len: usize) -> Vec<MaybeUninit<T>> {
    let mut blocks = Vec::with_capacity(len);
    blocks.resize_with(len, MaybeUninit::uninit);
    blocks
}

fn assign_piece<T>(workers: &mut [Worker<T>], piece: Piece) {
    let worker = workers
        .iter_mut()
        .min_by_key(|worker| worker.block_load)
        .expect("at least one permutation worker");
    worker.block_load += piece.blocks.len();
    worker.pieces.push(piece);
}

/// Reorders the physical blocks so the existing compactor can squeeze them into
/// the input slice.
pub(crate) fn permute_blocks<T: Send>(partition: &mut Partition<'_, '_, T>, workers: usize) {
    #[cfg(test)]
    permute(partition, workers, !cfg!(miri), None);
    #[cfg(not(test))]
    permute(partition, workers, !cfg!(miri));
}

fn permute<T: Send>(
    partition: &mut Partition<'_, '_, T>,
    workers: usize,
    parallel: bool,
    #[cfg(test)] fail_after: Option<usize>,
) {
    let block_count = partition.block_order.len();
    let head = partition.bucket_count;
    let block_size = partition.blocks.block_size;
    let worker_count = workers.max(1).min(block_count);
    // A long cycle creates at most two boundaries per cap-sized segment.
    // Since cap is about block_count / (4 * worker_count), snapshots need at
    // most 8 * worker_count blocks, plus one reusable block per worker.
    let cap = block_count.div_ceil(worker_count.saturating_mul(4)).max(2);
    let mut visited = alloc::vec![false; block_count];
    let mut jobs: Vec<_> = (0..worker_count)
        .map(|_| Worker {
            pieces: Vec::new(),
            scratch: uninitialized(block_size),
            block_load: 0,
            completed_pieces: 0,
        })
        .collect();
    let mut boundaries = Vec::new();

    for start in 0..block_count {
        if visited[start] {
            continue;
        }
        let mut cycle = Vec::new();
        let mut physical = start;
        while !visited[physical] {
            visited[physical] = true;
            cycle.push(physical as u32);
            // The destination of physical block p is the block named by
            // logical position (p - head) modulo block_count.
            let logical = if physical >= head {
                physical - head
            } else {
                block_count - (head - physical)
            };
            physical = partition.block_order[logical] as usize;
        }
        assert_eq!(physical, start);
        if cycle.len() <= 1 {
            continue;
        }
        if cycle.len() <= cap {
            assign_piece(
                &mut jobs,
                Piece {
                    blocks: cycle,
                    final_snapshot: None,
                },
            );
        } else {
            let base = boundaries.len();
            let piece_count = cycle.len().div_ceil(cap);
            for piece in cycle.chunks(cap) {
                boundaries.push(piece[0]);
            }
            for (index, piece) in cycle.chunks(cap).enumerate() {
                assign_piece(
                    &mut jobs,
                    Piece {
                        blocks: piece.to_vec(),
                        final_snapshot: Some((base + (index + 1) % piece_count) * block_size),
                    },
                );
            }
        }
    }
    debug_assert!(boundaries.len() <= worker_count.saturating_mul(8));

    let mut snapshots = BlockSnapshots(uninitialized(
        boundaries
            .len()
            .checked_mul(block_size)
            .expect("permutation scratch overflow"),
    ));
    let addresses = BlockAddresses(
        (0..block_count)
            .map(|physical| partition.blocks.block_ptr(physical))
            .collect(),
    );
    for (index, physical) in boundaries.into_iter().enumerate() {
        // SAFETY: source and destination are disjoint full blocks. Copying
        // MaybeUninit padding as raw bytes does not read it as T.
        unsafe {
            ptr::copy_nonoverlapping(
                addresses.at(physical),
                snapshots.0.as_mut_ptr().add(index * block_size).cast(),
                block_size,
            );
        }
    }

    #[cfg(test)]
    let completed_pieces = AtomicUsize::new(0);
    let run = |worker: &mut Worker<T>| {
        while worker.completed_pieces < worker.pieces.len() {
            // SAFETY: cycle construction only emits nonempty pieces.
            let piece = unsafe { worker.pieces.get_unchecked(worker.completed_pieces) };
            // SAFETY: the first block belongs to this piece.
            let first = unsafe { *piece.blocks.get_unchecked(0) };
            let last = *piece.blocks.last().expect("permutation piece is nonempty");
            let final_source = if let Some(offset) = piece.final_snapshot {
                snapshots.at(offset)
            } else {
                // SAFETY: a short cycle belongs to this worker; its scratch is
                // disjoint from every physical block.
                unsafe {
                    ptr::copy_nonoverlapping(
                        addresses.at(first),
                        worker.scratch.as_mut_ptr().cast(),
                        block_size,
                    );
                }
                worker.scratch.as_ptr().cast()
            };
            for pair in piece.blocks.windows(2) {
                // SAFETY: cycle nodes are distinct. Sources shared with another
                // piece were snapshotted before movement started.
                unsafe {
                    ptr::copy_nonoverlapping(
                        addresses.at(pair[1]),
                        addresses.at(pair[0]),
                        block_size,
                    );
                }
            }
            // SAFETY: final_source is a staged snapshot or this worker's
            // scratch, and is disjoint from the final destination.
            unsafe {
                ptr::copy_nonoverlapping(final_source, addresses.at(last), block_size);
            }
            worker.completed_pieces += 1;
            #[cfg(test)]
            if let Some(fail_after) = fail_after {
                let completed = completed_pieces.fetch_add(1, Ordering::Relaxed);
                assert_ne!(completed, fail_after, "injected panic between pieces");
            }
        }
    };

    // No user code or allocation runs during movement. If scheduling unwinds
    // between pieces, finish the plan before asking the existing compactor to
    // restore every live record to the input slice.
    let result = unwind::catch(|| {
        if parallel {
            jobs.par_iter_mut().for_each(run);
        } else {
            jobs.iter_mut().for_each(run);
        }
    });
    if result.is_err() {
        jobs.iter_mut().for_each(run);
    }

    for (logical, physical) in partition.block_order.iter_mut().enumerate() {
        *physical = if logical >= block_count - head {
            logical - (block_count - head)
        } else {
            logical + head
        } as u32;
    }
    if let Err(payload) = result {
        partition.compact();
        unwind::resume(payload);
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
    fn partial_cycle_is_stable() {
        let mut records: Vec<_> = (0..29).collect();
        let mut expected = Vec::new();
        for block in [1, 2, 3, 4, 5, 6, 0, 8, 7] {
            expected.extend(block * 3..block * 3 + 3);
        }
        expected.extend(27..29);
        {
            let mut buffers = Buffers::new();
            let mut partition = Partition::new(&mut records, 2, 3, &mut buffers);
            partition.block_order[2..9].rotate_left(1);
            partition.block_order.swap(9, 10);
            permute(&mut partition, 4, !cfg!(miri), None);
            partition.compact();
        }
        assert_eq!(records, expected);
    }

    #[test]
    fn panic_restores_owned_and_borrowed() {
        let len = if cfg!(miri) { 23 } else { 113 };
        for fail_after in [None, Some(0), Some(1)] {
            let drops = Arc::new((0..len).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
            let mut records: Vec<_> = (0..len)
                .map(|id| Owned {
                    key: id * 37 % 4,
                    id,
                    text: id.to_string(),
                    drops: drops.clone(),
                })
                .collect();
            let result = unwind::catch(|| {
                let mut buffers = Buffers::new();
                let mut partition = Partition::new(&mut records, 4, 3, &mut buffers);
                assert!(
                    partition
                        .step_with_ends(|record| record.key, false)
                        .0
                        .is_none()
                );
                permute(&mut partition, 4, !cfg!(miri), fail_after);
                partition.compact();
            });
            assert_eq!(result.is_err(), fail_after.is_some());
            let mut expected: Vec<_> = (0..len).collect();
            expected.sort_by_key(|id| id * 37 % 4);
            assert_eq!(
                records.iter().map(|record| record.id).collect::<Vec<_>>(),
                expected
            );
            assert!(drops.iter().all(|count| count.load(Ordering::Relaxed) == 0));
            drop(records);
            assert!(drops.iter().all(|count| count.load(Ordering::Relaxed) == 1));

            let mut backing: Vec<_> = (0..len).collect();
            let mut records: Vec<_> = backing.iter_mut().collect();
            let result = unwind::catch(|| {
                let mut buffers = Buffers::new();
                let mut partition = Partition::new(&mut records, 4, 3, &mut buffers);
                assert!(
                    partition
                        .step_with_ends(|record| **record * 37 % 4, false)
                        .0
                        .is_none()
                );
                permute(&mut partition, 4, !cfg!(miri), fail_after);
                partition.compact();
            });
            assert_eq!(result.is_err(), fail_after.is_some());
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
}
