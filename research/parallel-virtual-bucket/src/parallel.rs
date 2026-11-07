// SPDX-License-Identifier: BSD-2-Clause

//! Bounded worker scheduling and whole-job slot ownership transfers.

use std::mem::size_of;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use rayon::prelude::*;

use crate::local::{LiveCounters, LocalArena, SlotAddress, WorkerMemory};
use crate::{Config, Directory, Record, Scheduler, Stats, VirtualSorter};

struct SharedState {
    pending: Vec<(usize, Directory)>,
    completed: Vec<Option<Directory>>,
    free: Vec<usize>,
    minimum_free: usize,
}

#[derive(Default)]
struct WorkerHooks {
    #[cfg(test)]
    probe: Option<std::sync::Arc<LeaseProbe>>,
}

#[cfg(test)]
struct LeaseProbe {
    entry: std::sync::Barrier,
    exit: std::sync::Barrier,
    worker_ids: Mutex<Vec<std::thread::ThreadId>>,
}

struct WorkerResult<T: Record> {
    memory: WorkerMemory<T>,
    stats: Stats,
    writes: usize,
}

// Both Rayon and scoped-thread tests call this exact function. It performs no
// nested scheduling or user callback while a slot lease is outstanding.
fn run_worker<T: Record>(
    mut memory: WorkerMemory<T>,
    table: &[SlotAddress<T>],
    shared: &Mutex<SharedState>,
    counters: &LiveCounters,
    config: Config,
    _hooks: &WorkerHooks,
) -> WorkerResult<T> {
    let reserve = 3 * (1usize << config.digit_bits) + 8;
    let mut aggregate = Stats::default();
    let mut writes = 0;
    #[cfg(test)]
    let mut first_rendezvous_done = false;
    loop {
        let (index, source, leased) = {
            let mut state = shared.lock().expect("poisoned job queue");
            let Some((index, source)) = state.pending.pop() else {
                break;
            };
            assert!(
                state.free.len() >= reserve,
                "global batch reserve exhausted"
            );
            let from = state.free.len() - reserve;
            let leased = state.free.split_off(from);
            state.minimum_free = state.minimum_free.min(state.free.len());
            (index, source, leased)
        };
        let active = counters.active.fetch_add(1, Ordering::Relaxed) + 1;
        assert!(active <= config.child_workers);
        counters.maximum_active.fetch_max(active, Ordering::Relaxed);
        #[cfg(test)]
        let rendezvous = !first_rendezvous_done && _hooks.probe.is_some();
        #[cfg(test)]
        if rendezvous {
            let probe = _hooks.probe.as_ref().unwrap();
            first_rendezvous_done = true;
            probe
                .worker_ids
                .lock()
                .unwrap()
                .push(std::thread::current().id());
            probe.entry.wait();
        }
        let (mut arena, source) =
            LocalArena::new(table, source, leased, &mut memory, counters, config);
        let output = arena.sort_child(source);
        #[cfg(test)]
        if rendezvous {
            // All first jobs retain their leases until every participating
            // record kernel completes. No production build contains this wait.
            _hooks.probe.as_ref().unwrap().exit.wait();
        }
        let (output, returned, stats, local_writes) = arena.finish(output);
        // Every source slot has full capacity. A packed output retains no more
        // slots than the original source directory, so the loan is replenished.
        assert!(returned.len() >= reserve);
        writes += local_writes;
        aggregate.virtual_partitions += stats.virtual_partitions;
        aggregate.tile_leaves += stats.tile_leaves;
        aggregate.lsd_leaves += stats.lsd_leaves;
        aggregate.observed_metadata_bytes = aggregate
            .observed_metadata_bytes
            .max(stats.observed_metadata_bytes);
        {
            let mut state = shared.lock().expect("poisoned completed queue");
            assert!(state.completed[index].is_none());
            state.completed[index] = Some(output);
            state.free.extend(returned);
            counters.active.fetch_sub(1, Ordering::Relaxed);
        }
    }
    WorkerResult {
        memory,
        stats: aggregate,
        writes,
    }
}

pub(crate) fn sort_children<T: Record>(
    sorter: &mut VirtualSorter<'_, T>,
    roots: Vec<Directory>,
) -> Directory {
    sort_children_with_hooks(sorter, roots, &WorkerHooks::default())
}

fn sort_children_with_hooks<T: Record>(
    sorter: &mut VirtualSorter<'_, T>,
    mut roots: Vec<Directory>,
    hooks: &WorkerHooks,
) -> Directory {
    let started = Instant::now();
    let before_writes = sorter.writes;
    let config = sorter.config;
    // The staged API is safe and Directory is public: validate ownership before
    // any raw addresses cross threads, rather than trusting caller descriptors.
    sorter.verify_directories(&roots);

    // An original short fragment cannot be loaned as a full block. Normalize it
    // once before creating pointer ancestors or starting any child worker.
    if let Some(tail) = sorter.tail_slot {
        for root in &mut roots {
            for span in &mut root.blocks {
                if span.slot == tail {
                    let replacement = sorter.allocate();
                    for at in 0..span.len {
                        let value = sorter.read(tail, at);
                        sorter.write(replacement, at, value);
                    }
                    sorter.release(tail);
                    span.slot = replacement;
                }
            }
        }
        assert_eq!(sorter.live_len[tail], 0);
    }

    // Derive these two raw ancestors exactly once. Until all workers join, there
    // is no further input/scratch borrow, no allocation resize and no global
    // record read/write. Metadata and copied pointer values are all jobs receive.
    let original = sorter.input.as_mut_ptr();
    let scratch = sorter.scratch.as_mut_ptr();
    let full = sorter.full;
    let slots = full
        .checked_add(sorter.scratch_count)
        .expect("address table overflow");
    let b = config.block_records;
    let mut addresses = Vec::with_capacity(slots);
    for slot in 0..slots {
        let pointer = if slot < full {
            let offset = slot.checked_mul(b).expect("input offset overflow");
            // SAFETY: slot < full, and the constructor checked the original full
            // block range. This raw pointer derives from the single ancestor.
            unsafe { original.add(offset) }
        } else {
            let offset = (slot - full)
                .checked_mul(b)
                .expect("scratch offset overflow");
            // SAFETY: slot-full < scratch_count, whose complete block allocation
            // and layout were checked before mutation. It stays pinned until join.
            unsafe { scratch.add(offset).cast() }
        };
        addresses.push(SlotAddress(pointer));
    }

    let mut completed: Vec<Option<Directory>> = (0..roots.len()).map(|_| None).collect();
    let mut pending = Vec::new();
    for (index, root) in roots.into_iter().enumerate() {
        if root.len == 0 {
            completed[index] = Some(root);
        } else {
            pending.push((index, root));
        }
    }
    pending.reverse();
    let free = std::mem::take(&mut sorter.free);
    let minimum_free = free.len();
    let shared = Mutex::new(SharedState {
        pending,
        completed,
        free,
        minimum_free,
    });
    let counters = LiveCounters {
        live: AtomicUsize::new(sorter.live),
        maximum: AtomicUsize::new(sorter.stats.maximum_live_blocks),
        minimum_local_free: AtomicUsize::new(3 * (1usize << config.digit_bits) + 8),
        active: AtomicUsize::new(0),
        maximum_active: AtomicUsize::new(0),
    };
    let memories = std::mem::take(&mut sorter.worker_memory);
    assert_eq!(memories.len(), config.child_workers);
    let ambient_threads;
    let results: Vec<WorkerResult<T>> = match config.scheduler {
        Scheduler::Rayon => {
            ambient_threads = rayon::current_num_threads();
            memories
                .into_par_iter()
                .map(|memory| run_worker(memory, &addresses, &shared, &counters, config, hooks))
                .collect()
        }
        Scheduler::Scoped => {
            ambient_threads = 0;
            std::thread::scope(|scope| {
                let handles: Vec<_> = memories
                    .into_iter()
                    .map(|memory| {
                        let addresses = &addresses;
                        let shared = &shared;
                        let counters = &counters;
                        scope.spawn(move || {
                            run_worker(memory, addresses, shared, counters, config, hooks)
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().expect("scoped child panicked"))
                    .collect()
            })
        }
    };
    assert_eq!(counters.active.load(Ordering::Relaxed), 0);
    // Both scheduling backends have joined. Only now may the owner rebuild
    // global metadata and subsequently reborrow the original backing regions.
    let state = shared.into_inner().expect("poisoned final queue");
    assert!(state.pending.is_empty());
    sorter.free = state.free;
    sorter.live_len.fill(0);
    sorter.free_flags.fill(false);
    let mut seen = vec![false; sorter.live_len.len()];
    for &slot in &sorter.free {
        assert!(!seen[slot]);
        assert_ne!(Some(slot), sorter.tail_slot);
        seen[slot] = true;
        sorter.free_flags[slot] = true;
    }
    let mut directories = Vec::with_capacity(state.completed.len());
    for output in state.completed {
        let output = output.expect("missing root output");
        for span in &output.blocks {
            assert!(!seen[span.slot]);
            assert_ne!(Some(span.slot), sorter.tail_slot);
            seen[span.slot] = true;
            sorter.live_len[span.slot] = span.len;
        }
        directories.push(output);
    }
    assert!(seen[..slots].iter().all(|&owned| owned));
    sorter.live = counters.live.load(Ordering::Relaxed);
    assert_eq!(
        sorter.live,
        sorter.live_len.iter().filter(|&&len| len > 0).count()
    );
    sorter.stats.maximum_live_blocks = counters.maximum.load(Ordering::Relaxed);
    sorter.stats.minimum_free_blocks = sorter.stats.minimum_free_blocks.min(state.minimum_free);
    sorter.stats.minimum_worker_free_blocks = counters.minimum_local_free.load(Ordering::Relaxed);
    sorter.stats.maximum_active_children = counters.maximum_active.load(Ordering::Relaxed);
    sorter.stats.child_workers = config.child_workers;
    sorter.stats.ambient_threads = ambient_threads;
    let mut concurrent_local_metadata_bound = 0;
    for result in results {
        sorter.worker_memory.push(result.memory);
        sorter.writes += result.writes;
        sorter.stats.virtual_partitions += result.stats.virtual_partitions;
        sorter.stats.tile_leaves += result.stats.tile_leaves;
        sorter.stats.lsd_leaves += result.stats.lsd_leaves;
        concurrent_local_metadata_bound += result.stats.observed_metadata_bytes;
    }
    sorter.observe_metadata(&directories);
    // This is still an observed accounting figure, not an allocator peak. Sum
    // each worker's observed local high-water figure, not a single worker only.
    sorter.stats.observed_metadata_bytes +=
        addresses.capacity() * size_of::<SlotAddress<T>>() + concurrent_local_metadata_bound;
    let result = VirtualSorter::<T>::concat(directories);
    sorter.stats.child_time = started.elapsed();
    sorter.stats.child_write_bytes = (sorter.writes - before_writes) * size_of::<T>();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Row16;
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier};

    #[test]
    fn miri_forced_parallel_leases() {
        let workers = 3;
        let mut input: Vec<_> = (0..97)
            .map(|i| Row16 {
                key: ((i * 37 % 13) as u64) << 60 | (i * 17 % 31) as u64,
                ordinal: i as u64,
            })
            .collect();
        let mut expected = input.clone();
        expected.sort_by_key(|row| row.key);
        let config = Config {
            block_records: 3,
            digit_bits: 2,
            tile_records: 5,
            msd_levels: 2,
            root_shards: 3,
            child_workers: workers,
            scheduler: Scheduler::Scoped,
        };
        let probe = Arc::new(LeaseProbe {
            entry: Barrier::new(workers),
            exit: Barrier::new(workers),
            worker_ids: Mutex::new(Vec::new()),
        });
        let hooks = WorkerHooks {
            probe: Some(probe.clone()),
        };
        let mut sorter = VirtualSorter::new(&mut input, config);
        let roots = sorter.scatter_root();
        // The only rendezvous entry point requires one ready job per scoped
        // thread. It is never used with an ambient pool smaller than this count.
        assert!(roots.iter().filter(|root| root.len > 0).count() >= workers);
        let sorted = sort_children_with_hooks(&mut sorter, roots, &hooks);
        sorter.verify_directories(std::slice::from_ref(&sorted));
        let packed = sorter.repack_global(sorted);
        sorter.verify_directories(std::slice::from_ref(&packed));
        let stats = sorter.restore(packed);
        assert_eq!(input, expected);
        let ids = probe.worker_ids.lock().unwrap();
        assert_eq!(ids.len(), workers);
        assert_eq!(ids.iter().copied().collect::<HashSet<_>>().len(), workers);
        assert_eq!(stats.maximum_active_children, workers);
        eprintln!("FORCED_PARALLEL workers={workers} ids={ids:?} lease_highwater={} min_local_free={} min_global_free={} aux_record_bytes={}",
            stats.maximum_active_children, stats.minimum_worker_free_blocks,
            stats.minimum_free_blocks, stats.auxiliary_record_bytes);
    }
}
