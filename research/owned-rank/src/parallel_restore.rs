// SPDX-License-Identifier: BSD-2-Clause
//! Bounded-snapshot segmentation of disjoint restoration paths and cycles.
use super::{Addresses, NONE};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Piece {
    start: usize,
    len: usize,
    last_source: usize,
    save_cycle: bool,
    next: usize,
}
struct Worker {
    head: usize,
    tail: usize,
    load: usize,
    temp: usize,
}
pub(super) struct Plan {
    nodes: Vec<usize>,
    pieces: Vec<Piece>,
    workers: Vec<Worker>,
    snapshots: Vec<usize>,
    visited: Vec<bool>,
    grain: usize,
    snapshot_base: usize,
}
impl Plan {
    pub(super) fn new(full: usize, p: usize, snapshot_base: usize) -> Self {
        let snapshot_limit = p.checked_mul(8).expect("snapshot block bound overflow");
        let temp_base = snapshot_base
            .checked_add(snapshot_limit)
            .expect("restore temp base overflow");
        let divisor = p.checked_mul(4).expect("restore segment divisor overflow");
        let workers = (0..p)
            .map(|i| Worker {
                head: NONE,
                tail: NONE,
                load: 0,
                temp: temp_base.checked_add(i).expect("restore temp id overflow"),
            })
            .collect();
        Self {
            nodes: Vec::with_capacity(full),
            pieces: Vec::with_capacity(full),
            workers,
            snapshots: Vec::with_capacity(snapshot_limit),
            visited: vec![false; full],
            grain: full.div_ceil(divisor).max(1),
            snapshot_base,
        }
    }
    pub(super) fn metadata_slots(&self) -> usize {
        self.nodes.capacity()
            + self.snapshots.capacity()
            + self.visited.capacity()
            + self.pieces.capacity() * std::mem::size_of::<Piece>() / std::mem::size_of::<usize>()
            + self.workers.capacity() * std::mem::size_of::<Worker>() / std::mem::size_of::<usize>()
    }
    fn add_component(&mut self, start: usize, len: usize, cycle: bool, output: &[usize]) {
        let split = len > self.grain;
        let mut offset = 0;
        while offset < len {
            let take = if split {
                self.grain.min(len - offset)
            } else {
                len
            };
            let end_node = self.nodes[start + offset + take - 1];
            let endpoint = output[end_node];
            let worker_id = self
                .workers
                .iter()
                .enumerate()
                .min_by_key(|(_, w)| w.load)
                .unwrap()
                .0;
            let save_cycle = cycle && !split;
            let last_source = if save_cycle {
                self.workers[worker_id].temp
            } else if split && endpoint < output.len() {
                // Each long component contributes at most ceil(L/grain)
                // snapshots. Since L>grain, this is <2L/grain. Summed over
                // disjoint components the count is <2full/grain <=8P.
                assert!(self.snapshots.len() < self.snapshots.capacity());
                let id = self.snapshot_base + self.snapshots.len();
                self.snapshots.push(endpoint);
                id
            } else {
                endpoint
            };
            assert!(self.pieces.len() < self.pieces.capacity());
            let id = self.pieces.len();
            self.pieces.push(Piece {
                start: start + offset,
                len: take,
                last_source,
                save_cycle,
                next: NONE,
            });
            let worker = &mut self.workers[worker_id];
            if worker.head == NONE {
                worker.head = id;
            } else {
                self.pieces[worker.tail].next = id;
            }
            worker.tail = id;
            worker.load += take;
            offset += take;
        }
    }
    fn collect_component(&mut self, start: usize, cycle: bool, output: &[usize]) {
        let base = self.nodes.len();
        let mut at = start;
        loop {
            assert!(at < output.len() && !self.visited[at]);
            self.visited[at] = true;
            assert!(self.nodes.len() < self.nodes.capacity());
            self.nodes.push(at);
            let next = output[at];
            if cycle && next == start {
                break;
            }
            if next >= output.len() {
                assert!(!cycle);
                break;
            }
            at = next;
        }
        let len = self.nodes.len() - base;
        // Identity blocks need neither a piece nor a temporary snapshot.
        if cycle && len == 1 {
            return;
        }
        self.add_component(base, len, cycle, output);
    }
    /// Builds the plan and captures all cross-worker endpoint snapshots.
    ///
    /// # Safety
    /// Requires the same initialized/disjoint blocks and reserved storage as
    /// execute. No worker may start until this method has returned.
    unsafe fn prepare<T: Send>(
        &mut self,
        addresses: &Addresses<T>,
        output: &[usize],
        location: &[usize],
        b: usize,
    ) {
        assert_eq!(self.visited.len(), output.len());
        assert!(self.nodes.is_empty() && self.pieces.is_empty());
        for start in 0..output.len() {
            if location[start] == NONE {
                self.collect_component(start, false, output);
            }
        }
        for start in 0..output.len() {
            if !self.visited[start] {
                self.collect_component(start, true, output);
            }
        }
        // Global barrier: every cross-segment source is snapshotted before any
        // destination is overwritten. Short cycles use one reusable temp per
        // worker instead of allocating one temp for every short component.
        for (index, &source) in self.snapshots.iter().enumerate() {
            // SAFETY: source is a live full output block; reserved snapshot is
            // allocated and excluded from all original sources and targets.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    addresses.at(source),
                    addresses.at(self.snapshot_base + index),
                    b,
                );
            }
        }
    }
    /// Plans and executes independent pull segments using preallocated storage.
    ///
    /// # Safety
    /// Every accessed address names a disjoint allocated full-capacity b-record
    /// block. Unaccessed table entries may include the shorter original tail.
    /// output is an injective directory of initialized full blocks. Targets are
    /// 0..output.len(), location is its inverse, and no concurrent record access
    /// exists. Reserved snapshot/temp IDs are absent from every source/target.
    pub(super) unsafe fn execute<T: Send>(
        &mut self,
        addresses: &Addresses<T>,
        output: &[usize],
        location: &[usize],
        b: usize,
        parallel: bool,
    ) -> u64 {
        // SAFETY: execute's contract supplies the same block ownership and
        // initialized source guarantees required by preparation.
        unsafe {
            self.prepare(addresses, output, location, b);
        }
        let mask = AtomicU64::new(0);
        let nodes = &self.nodes;
        let pieces = &self.pieces;
        let run = |worker: &Worker| {
            if worker.head == NONE {
                return;
            }
            let thread = rayon::current_thread_index().unwrap_or(63).min(63);
            mask.fetch_or(1u64 << thread, Ordering::Relaxed);
            // SAFETY: preparation snapshotted every cross-worker source. Each
            // worker is scheduled once and owns disjoint targets and temp.
            unsafe {
                run_worker(addresses, nodes, pieces, worker, b);
            }
        };
        if parallel {
            self.workers.par_iter().for_each(run);
        } else {
            self.workers.iter().for_each(run);
        }
        mask.load(Ordering::Relaxed)
    }
}

/// Executes exactly one preplanned restoration worker's list.
///
/// # Safety
/// prepare must have completed for this immutable plan/table. Every worker is
/// invoked at most once, concurrently only with distinct workers from the same
/// plan, and there is no other record access until all workers have joined.
unsafe fn run_worker<T: Send>(
    addresses: &Addresses<T>,
    nodes: &[usize],
    pieces: &[Piece],
    worker: &Worker,
    b: usize,
) {
    let mut piece_id = worker.head;
    while piece_id != NONE {
        let piece = &pieces[piece_id];
        if piece.save_cycle {
            // SAFETY: this complete cycle belongs only to this worker.
            // Its temporary is exclusive and reused only after finish.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    addresses.at(nodes[piece.start]),
                    addresses.at(worker.temp),
                    b,
                );
            }
        }
        for offset in 0..piece.len {
            let target = nodes[piece.start + offset];
            let source = if offset + 1 == piece.len {
                piece.last_source
            } else {
                nodes[piece.start + offset + 1]
            };
            // SAFETY: interior sources are read before this same worker
            // overwrites them. Endpoint sources are immutable snapshots,
            // exclusive cycle temps, or external blocks never targeted.
            // Every target belongs to exactly one piece, source!=target.
            unsafe {
                std::ptr::copy_nonoverlapping(addresses.at(source), addresses.at(target), b);
            }
        }
        piece_id = piece.next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scoped_owned_fixture<T: Send>(values: Vec<T>) -> Vec<T> {
        assert_eq!(values.len(), 40);
        let mut layout: Vec<usize> = (0..40).collect();
        for (i, slot) in layout.iter_mut().enumerate().take(23) {
            *slot = i + 1;
        }
        layout[23] = 0;
        layout[24] = 25;
        layout[25] = 24;
        layout[26] = 27;
        layout[27] = 40;
        layout[28] = 41;
        let sources = 42;
        let total = sources + 18;
        let mut storage = crate::cells::<T>(total);
        let base = storage.as_mut_ptr().cast::<T>();
        let addresses = Addresses(
            (0..total)
                .map(|i| {
                    // SAFETY: every slot is allocated and aligned, including scratch.
                    unsafe { base.add(i) }
                })
                .collect(),
        );
        for (&source, value) in layout.iter().zip(values) {
            // SAFETY: each distinct source acquires exactly one owned input T.
            unsafe {
                addresses.at(source).write(value);
            }
        }
        let mut location = vec![NONE; total];
        for (i, &source) in layout.iter().enumerate() {
            location[source] = i;
        }
        let mut plan = Plan::new(40, 2, sources);
        // SAFETY: every directory source was initialized exactly once, target
        // blocks have capacity one, and reserved scratch is not a source.
        unsafe {
            plan.prepare(&addresses, &layout, &location, 1);
        }
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            for worker in &plan.workers {
                let addresses = &addresses;
                let nodes = &plan.nodes;
                let pieces = &plan.pieces;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    // SAFETY: exact production kernel, snapshots complete;
                    // two distinct worker lists own disjoint targets and temps.
                    unsafe {
                        run_worker(addresses, nodes, pieces, worker, 1);
                    }
                });
            }
        });
        (0..40)
            .map(|i| {
                // SAFETY: joined restoration now owns exactly one T at each target.
                // Transfer to returned Vec; MaybeUninit storage drops no stale bytes.
                unsafe { addresses.at(i).read() }
            })
            .collect()
    }
    #[test]
    fn owned_miri_scoped_restore() {
        use std::cell::Cell;
        use std::sync::atomic::AtomicUsize;
        use std::sync::Arc;
        struct Owned {
            id: usize,
            text: String,
            cell: Cell<usize>,
            drops: Arc<Vec<AtomicUsize>>,
        }
        impl Drop for Owned {
            fn drop(&mut self) {
                self.drops[self.id].fetch_add(1, Ordering::SeqCst);
            }
        }
        let drops = Arc::new((0..40).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
        let values = (0..40)
            .map(|id| Owned {
                id,
                text: format!("row-{id}"),
                cell: Cell::new(id),
                drops: drops.clone(),
            })
            .collect();
        let output = scoped_owned_fixture(values);
        for (i, row) in output.iter().enumerate() {
            assert_eq!(row.id, i);
            assert_eq!(row.text, format!("row-{i}"));
            assert_eq!(row.cell.get(), i);
        }
        assert!(drops.iter().all(|n| n.load(Ordering::SeqCst) == 0));
        drop(output);
        assert!(drops.iter().all(|n| n.load(Ordering::SeqCst) == 1));
        let mut backing: Vec<usize> = (0..40).collect();
        let refs = scoped_owned_fixture(backing.iter_mut().collect::<Vec<_>>());
        for (i, row) in refs.into_iter().enumerate() {
            assert_eq!(*row, i);
            *row += 100;
        }
        assert_eq!(backing, (100..140).collect::<Vec<_>>());
    }
    #[test]
    fn miri_scoped_restore_kernel() {
        // Full=40 gives grain=5 for P=2. Each fixture combines split long
        // components, an unsplit short cycle, a short external path and identity.
        for long_path in [false, true] {
            for b in [1, 3] {
                let mut layout: Vec<usize> = (0..40).collect();
                for (i, slot) in layout.iter_mut().enumerate().take(23) {
                    *slot = i + 1;
                }
                layout[23] = if long_path { 42 } else { 0 };
                layout[24] = 25;
                layout[25] = 24;
                layout[26] = 27;
                layout[27] = 40;
                layout[28] = 41;
                let sources = layout.iter().copied().max().unwrap() + 1;
                let total = sources + 18;
                let mut values: Vec<u64> = (0..total * b).map(|i| i as u64).collect();
                let expected: Vec<_> = layout
                    .iter()
                    .flat_map(|&source| values[source * b..(source + 1) * b].iter().copied())
                    .collect();
                let base = values.as_mut_ptr();
                let addresses = Addresses(
                    (0..total)
                        .map(|i| {
                            // SAFETY: every block, including reserved scratch, is allocated.
                            unsafe { base.add(i * b) }
                        })
                        .collect(),
                );
                let mut location = vec![NONE; total];
                for (i, &source) in layout.iter().enumerate() {
                    assert_eq!(location[source], NONE);
                    location[source] = i;
                }
                let mut plan = Plan::new(layout.len(), 2, sources);
                // SAFETY: injective initialized fixture with disjoint full blocks
                // and 9P reserved slots, no concurrent access during snapshots.
                unsafe {
                    plan.prepare(&addresses, &layout, &location, b);
                }
                assert!(!plan.snapshots.is_empty());
                assert!(plan.pieces.iter().any(|piece| piece.save_cycle));
                assert!(plan.pieces.iter().any(|piece| !piece.save_cycle
                    && piece.last_source >= layout.len()
                    && piece.last_source < sources));
                assert!(plan.workers.iter().all(|worker| worker.head != NONE));
                let barrier = std::sync::Barrier::new(2);
                std::thread::scope(|scope| {
                    for worker in &plan.workers {
                        let addresses = &addresses;
                        let nodes = &plan.nodes;
                        let pieces = &plan.pieces;
                        let barrier = &barrier;
                        scope.spawn(move || {
                            barrier.wait();
                            // SAFETY: exact production worker kernel, each worker
                            // once, all snapshots complete, disjoint target/temp
                            // ownership. Scoped join precedes any validation reads.
                            unsafe {
                                run_worker(addresses, nodes, pieces, worker, b);
                            }
                        });
                    }
                });
                assert_eq!(&values[..expected.len()], expected.as_slice());
            }
        }
    }
    #[test]
    fn parallel_restore_shapes() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap();
        pool.install(|| {
            let layouts = [
                vec![],
                vec![0],
                vec![0, 1, 2, 3],
                vec![1, 0],
                vec![1, 2, 3, 0],
                vec![4, 0, 5, 2],
                vec![1, 0, 5, 2, 4],
                vec![5, 6, 7, 8, 9],
                (1..41).chain(std::iter::once(0)).collect(),
                (0..40).map(|i| i ^ 1).collect(),
                (1..41).chain(std::iter::once(50)).collect(),
            ];
            for layout in &layouts {
                for p in [1, 2, 4] {
                    for b in [1, 3] {
                        let sources = layout
                            .iter()
                            .copied()
                            .max()
                            .map_or(0, |x| x + 1)
                            .max(layout.len());
                        let total = sources + 9 * p;
                        let mut values: Vec<u64> = (0..total * b).map(|i| i as u64).collect();
                        let expected: Vec<_> = layout
                            .iter()
                            .flat_map(|&s| values[s * b..(s + 1) * b].iter().copied())
                            .collect();
                        let base = values.as_mut_ptr();
                        let addresses = Addresses(
                            (0..total)
                                .map(|i| {
                                    // SAFETY: every block is allocated, including all reserved slots.
                                    unsafe { base.add(i * b) }
                                })
                                .collect(),
                        );
                        let mut location = vec![NONE; total];
                        for (i, &source) in layout.iter().enumerate() {
                            location[source] = i;
                        }
                        let mut plan = Plan::new(layout.len(), p, sources);
                        // SAFETY: fixtures supply disjoint initialized blocks and an injective
                        // directory, with 9P reserved blocks excluded from source/target IDs.
                        unsafe {
                            plan.execute(&addresses, layout, &location, b, true);
                        }
                        assert_eq!(&values[..expected.len()], expected.as_slice());
                    }
                }
            }
        });
    }
}
