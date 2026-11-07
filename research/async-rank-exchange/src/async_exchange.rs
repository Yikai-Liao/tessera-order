// SPDX-License-Identifier: BSD-2-Clause
//! Ordered endpoint locks over an injective stable-rank directory.
//!
//! No child borrows an interval until every covering block has retired. A stale
//! source can name retired metadata, but must fail validation before payload use.
use super::*;
use std::{
    any::Any,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

const DEAD: usize = usize::MAX;
type Panic = Box<dyn Any + Send + 'static>;

/// Child policy for the same asynchronous exact-target moving kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// Restore and join, then sort the original consecutive fixed bucket groups.
    Barrier,
    /// Let the final contributing owner sort each nonempty bucket immediately.
    RetireInline,
}

/// Per-call movement counts and a capacity ledger, excluding allocator overhead.
///
/// Child allocation peaks must additionally be measured by the benchmark's
/// allocator instrumentation. Retained bytes are not a transient peak claim.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub identity_blocks: usize,
    pub one_copy_blocks: usize,
    pub three_copy_blocks: usize,
    pub copy_records: usize,
    pub retries: usize,
    pub root_retained_bytes: usize,
    pub exchange_retained_bytes: usize,
    pub child_retained_bytes: usize,
    pub atomic_bool_bytes: usize,
    pub atomic_usize_bytes: usize,
    pub worker_bytes: usize,
    pub recovered_worker_panics: usize,
}

/// Compact lock whose release contains no payload access or user operation.
struct Held<'a>(&'a AtomicBool);
impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
fn lock(flag: &AtomicBool) -> Held<'_> {
    while flag
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        std::hint::spin_loop();
    }
    Held(flag)
}

/// Metadata and pointers shared without creating references to payload records.
struct Shared<'a, T> {
    memory: Memory<T>,
    positions: Vec<AtomicUsize>,
    inverse: Vec<AtomicUsize>,
    locks: Vec<AtomicBool>,
    active: Vec<AtomicUsize>,
    ends: &'a [usize],
    tail_source: usize,
    tail_len: usize,
    inline: bool,
    failed: AtomicBool,
}

/// A fixed resource-owning job, with its progress committed before callbacks.
struct Worker<T> {
    id: usize,
    next: usize,
    end: usize,
    owns_tail: bool,
    tail_done: bool,
    bucket_cursor: usize,
    remaining: Vec<usize>,
    temp: Vec<MaybeUninit<T>>,
    workspace: tessera::Workspace<T>,
    panic: Option<Panic>,
    worker_panic: Option<Panic>,
    identity: usize,
    one: usize,
    three: usize,
    retries: usize,
    #[cfg(test)]
    panic_after: Option<usize>,
}

/// Preallocated exchange state, kept alive until all jobs and recovery have joined.
pub(super) struct State<'a, T> {
    shared: Shared<'a, T>,
    workers: Vec<Worker<T>>,
    scheduler_panic: Option<Panic>,
}

impl<'a, T: Record> State<'a, T> {
    /// Allocates all root state before scatter; child capacities grow only in children.
    pub(super) fn new(
        memory: Memory<T>,
        total: usize,
        n: usize,
        workers: usize,
        ends: &'a [usize],
        policy: Option<Policy>,
    ) -> Self {
        let full = memory.full;
        let block = memory.block;
        assert!(block > 0 && workers > 0 && total >= full);
        assert_eq!(full, n / block);
        assert_eq!(ends.last(), Some(&n));
        assert!(ends.windows(2).all(|w| w[0] <= w[1]));
        // Bound all pointer offsets and the 0/1/3-copy report before movement.
        assert!(n.checked_mul(size_of::<T>()).unwrap() <= isize::MAX as usize);
        let scratch = (total - full).checked_mul(block).unwrap();
        assert!(scratch.checked_mul(size_of::<T>()).unwrap() <= isize::MAX as usize);
        full.checked_mul(block)
            .unwrap()
            .checked_mul(3)
            .unwrap()
            .checked_add(n % block)
            .unwrap();
        workers.checked_mul(ends.len()).unwrap();
        let mut active: Vec<_> = (0..ends.len()).map(|_| AtomicUsize::new(0)).collect();
        let mut jobs = Vec::with_capacity(workers);
        let mut first = 0;
        for id in 0..workers {
            let end = first + full / workers + usize::from(id < full % workers);
            let owns_tail = id + 1 == workers && n % block != 0;
            let mut remaining = Vec::with_capacity(ends.len());
            let mut begin = 0;
            for (bucket, &stop) in ends.iter().enumerate() {
                let count = if begin == stop {
                    0
                } else {
                    let lower = (begin / block).max(first);
                    let upper = stop.div_ceil(block).min(full).min(end);
                    upper.saturating_sub(lower) + usize::from(owns_tail && stop > full * block)
                };
                remaining.push(count);
                if count != 0 {
                    *active[bucket].get_mut() += 1;
                }
                begin = stop;
            }
            jobs.push(Worker {
                id,
                next: first,
                end,
                owns_tail,
                tail_done: false,
                bucket_cursor: ends.partition_point(|&stop| stop <= first * block),
                remaining,
                temp: cells(block),
                workspace: tessera::Workspace::new(),
                panic: None,
                worker_panic: None,
                identity: 0,
                one: 0,
                three: 0,
                retries: 0,
                #[cfg(test)]
                panic_after: None,
            });
            first = end;
        }
        Self {
            shared: Shared {
                memory,
                positions: (0..full).map(|_| AtomicUsize::new(DEAD)).collect(),
                inverse: (0..total).map(|_| AtomicUsize::new(DEAD)).collect(),
                locks: (0..total).map(|_| AtomicBool::new(false)).collect(),
                active,
                ends,
                tail_source: DEAD,
                tail_len: n % block,
                inline: policy == Some(Policy::RetireInline),
                failed: AtomicBool::new(false),
            },
            workers: jobs,
            scheduler_panic: None,
        }
    }

    /// Validates the completed scatter map without accessing payload records.
    pub(super) fn seed(&mut self, directory: &[usize]) {
        let full = self.shared.memory.full;
        assert_eq!(
            directory.len(),
            full + usize::from(self.shared.tail_len != 0)
        );
        for (target, &source) in directory.iter().take(full).enumerate() {
            assert!(source < self.shared.inverse.len());
            assert_eq!(*self.shared.inverse[source].get_mut(), DEAD);
            *self.shared.inverse[source].get_mut() = target;
            *self.shared.positions[target].get_mut() = source;
        }
        if self.shared.tail_len != 0 {
            let source = directory[full];
            assert!(source >= full && source < self.shared.inverse.len());
            assert_eq!(*self.shared.inverse[source].get_mut(), DEAD);
            self.shared.tail_source = source;
        }
    }

    /// Runs fixed jobs, completes every outstanding root target, then propagates panic.
    pub(super) fn execute(
        &mut self,
        parallel: bool,
        policy: Option<Policy>,
        root_retained_bytes: usize,
    ) -> Report {
        self.execute_with(parallel, policy, root_retained_bytes, &T::sort_child)
    }

    fn execute_with(
        &mut self,
        parallel: bool,
        policy: Option<Policy>,
        root_retained_bytes: usize,
        child: &(impl Fn(&mut tessera::Workspace<T>, &mut [T]) + Sync),
    ) -> Report {
        let mut guard = Finish {
            state: self,
            armed: true,
        };
        let shared = &guard.state.shared;
        let work = &mut guard.state.workers;
        let result = catch_unwind(AssertUnwindSafe(|| {
            execute_jobs(work, parallel, |job| {
                // No worker panic reaches Rayon while another job owns root data.
                let result = catch_unwind(AssertUnwindSafe(|| shared.run(job, child, false)));
                if let Err(payload) = result {
                    shared.failed.store(true, Ordering::Release);
                    job.worker_panic = Some(payload);
                }
            });
        }));
        if let Err(payload) = result {
            guard.state.shared.failed.store(true, Ordering::Release);
            guard.state.scheduler_panic = Some(payload);
        }
        // execute_jobs joins all started jobs even on unwind. Recovery uses the
        // same ownership/progress ledger, never the stale original directory.
        guard.state.finish_pending();
        guard.armed = false;
        if policy == Some(Policy::Barrier) && !guard.state.shared.failed.load(Ordering::Acquire) {
            let shared = &guard.state.shared;
            let chunk = shared.ends.len().div_ceil(guard.state.workers.len()).max(1);
            let result = catch_unwind(AssertUnwindSafe(|| {
                execute_jobs(&mut guard.state.workers, parallel, |job| {
                    let first = job.id.saturating_mul(chunk).min(shared.ends.len());
                    let end = first.saturating_add(chunk).min(shared.ends.len());
                    for bucket in first..end {
                        shared.child(job, bucket, child);
                    }
                });
            }));
            if let Err(payload) = result {
                guard.state.scheduler_panic = Some(payload);
            }
        }
        let report = guard.state.report(root_retained_bytes);
        let mut panic = guard.state.scheduler_panic.take();
        for job in &mut guard.state.workers {
            for slot in [&mut job.panic, &mut job.worker_panic] {
                if let Some(payload) = slot.take() {
                    if panic.is_none() {
                        panic = Some(payload);
                    } else {
                        // Root restoration and every worker have completed. A
                        // secondary destructor cannot see unfinished input now.
                        if let Err(nested) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
                            // Do not recursively drop an arbitrary new panic
                            // payload or replace the selected primary panic.
                            std::mem::forget(nested);
                        }
                    }
                }
            }
        }
        if let Some(payload) = panic {
            resume_unwind(payload);
        }
        report
    }

    /// Finishes only unfinished targets; no callback, key, allocation or scheduling.
    fn finish_pending(&mut self) {
        for job in &mut self.workers {
            self.shared.run(job, &T::sort_child, true);
        }
    }

    fn report(&self, root_retained_bytes: usize) -> Report {
        let mut report = Report {
            root_retained_bytes,
            atomic_bool_bytes: size_of::<AtomicBool>(),
            atomic_usize_bytes: size_of::<AtomicUsize>(),
            worker_bytes: size_of::<Worker<T>>(),
            exchange_retained_bytes: size_of::<Self>()
                + self.workers.capacity() * size_of::<Worker<T>>()
                + self.shared.positions.capacity() * size_of::<AtomicUsize>()
                + self.shared.inverse.capacity() * size_of::<AtomicUsize>()
                + self.shared.locks.capacity() * size_of::<AtomicBool>()
                + self.shared.active.capacity() * size_of::<AtomicUsize>(),
            ..Report::default()
        };
        for job in &self.workers {
            report.identity_blocks += job.identity;
            report.one_copy_blocks += job.one;
            report.three_copy_blocks += job.three;
            report.retries += job.retries;
            report.exchange_retained_bytes += job.remaining.capacity() * size_of::<usize>()
                + job.temp.capacity() * size_of::<T>();
            report.child_retained_bytes += job.workspace.retained_bytes();
            report.recovered_worker_panics += usize::from(job.worker_panic.is_some());
        }
        report.copy_records = (report.one_copy_blocks + 3 * report.three_copy_blocks)
            * self.shared.memory.block
            + self.shared.tail_len;
        report
    }
}

/// A last-resort completion owner for unwinding around the joined dispatcher.
struct Finish<'s, 'a, T: Record> {
    state: &'s mut State<'a, T>,
    armed: bool,
}
impl<T: Record> Drop for Finish<'_, '_, T> {
    fn drop(&mut self) {
        if self.armed {
            self.state.shared.failed.store(true, Ordering::Release);
            self.state.finish_pending();
        }
    }
}

impl<T: Record> Shared<'_, T> {
    /// Returns a certified slot pointer without a panic-capable checked helper.
    ///
    /// # Safety
    /// `slot` is a validated physical ID and the selected block is in its own
    /// allocation. Numeric adjacency never bridges input and scratch provenance.
    unsafe fn pointer(&self, slot: usize) -> *mut MaybeUninit<T> {
        // SAFETY: construction bounded both allocations and every slot offset.
        unsafe {
            if slot < self.memory.full {
                self.memory.input.add(slot * self.memory.block).cast()
            } else {
                self.memory
                    .scratch
                    .add((slot - self.memory.full) * self.memory.block)
            }
        }
    }

    /// Tries a speculative source, returning None without payload access if stale.
    fn exchange(&self, target: usize, source: usize, temp: *mut MaybeUninit<T>) -> Option<usize> {
        // SAFETY: target comes from a certified owner range; source is either an
        // initial checked ID or an ID copied from another valid position entry.
        // All unchecked metadata indices below are consequently in range.
        unsafe {
            let low = target.min(source);
            let high = target.max(source);
            let _low = lock(self.locks.get_unchecked(low));
            let _high = (low != high).then(|| lock(self.locks.get_unchecked(high)));
            if self.positions.get_unchecked(target).load(Ordering::Acquire) != source
                || self.inverse.get_unchecked(source).load(Ordering::Relaxed) != target
            {
                return None;
            }
            if target == source {
                return Some(0);
            }
            let displaced = self.inverse.get_unchecked(target).load(Ordering::Relaxed);
            let destination_ptr = self.pointer(target);
            let source_ptr = self.pointer(source);
            let block = self.memory.block;
            // Live endpoints are fully initialized. A dead endpoint is never
            // read. temp belongs to this job and has exactly B available cells.
            if displaced != DEAD {
                std::ptr::copy_nonoverlapping(destination_ptr, temp, block);
            }
            std::ptr::copy_nonoverlapping(source_ptr, destination_ptr, block);
            if displaced != DEAD {
                std::ptr::copy_nonoverlapping(temp, source_ptr, block);
                self.positions
                    .get_unchecked(displaced)
                    .store(source, Ordering::Release);
                self.inverse
                    .get_unchecked(source)
                    .store(displaced, Ordering::Relaxed);
            } else {
                self.inverse
                    .get_unchecked(source)
                    .store(DEAD, Ordering::Relaxed);
            }
            self.inverse
                .get_unchecked(target)
                .store(target, Ordering::Relaxed);
            self.positions
                .get_unchecked(target)
                .store(target, Ordering::Release);
            // Both ledger endpoints are committed before either Held drops.
            Some(if displaced == DEAD { 1 } else { 3 })
        }
    }

    fn run(
        &self,
        job: &mut Worker<T>,
        child: &(impl Fn(&mut tessera::Workspace<T>, &mut [T]) + Sync),
        recovering: bool,
    ) {
        while job.next < job.end {
            let target = job.next;
            let copies = loop {
                let source = self.positions[target].load(Ordering::Acquire);
                if let Some(copies) = self.exchange(target, source, job.temp.as_mut_ptr()) {
                    break copies;
                }
                #[cfg(feature = "async-exchange-diagnostics")]
                {
                    job.retries += 1;
                }
            };
            match copies {
                0 => job.identity += 1,
                1 => job.one += 1,
                _ => job.three += 1,
            }
            // Commit root progress before callbacks. Even a later internal
            // unwind cannot make recovery touch an already published payload.
            job.next += 1;
            self.retire(
                job,
                target * self.memory.block,
                self.memory.block,
                child,
                recovering,
            );
            #[cfg(test)]
            if !recovering && job.panic_after == Some(job.next) {
                job.panic_after = None;
                panic!("injected worker boundary panic");
            }
        }
        if job.owns_tail && !job.tail_done {
            // SAFETY: reserved tail scratch is excluded from every full-block
            // source and target, and only this owner accesses its initialized
            // prefix. Original input tail is not any full-block destination.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.pointer(self.tail_source).cast::<T>(),
                    self.memory.input.add(self.memory.full * self.memory.block),
                    self.tail_len,
                );
            }
            job.tail_done = true;
            self.retire(
                job,
                self.memory.full * self.memory.block,
                self.tail_len,
                child,
                recovering,
            );
        }
    }

    /// Credits exactly the nonempty buckets intersecting one retired interval.
    fn retire(
        &self,
        job: &mut Worker<T>,
        first: usize,
        len: usize,
        child: &(impl Fn(&mut tessera::Workspace<T>, &mut [T]) + Sync),
        recovering: bool,
    ) {
        while job.bucket_cursor < self.ends.len() && self.ends[job.bucket_cursor] <= first {
            job.bucket_cursor += 1;
        }
        let mut bucket = job.bucket_cursor;
        let stop = first + len;
        while bucket < self.ends.len() {
            let begin = if bucket == 0 {
                0
            } else {
                self.ends[bucket - 1]
            };
            if begin >= stop {
                break;
            }
            if self.ends[bucket] > begin {
                // Every positive counter was geometrically checked before
                // scatter, and its unique owner credits each block exactly once.
                job.remaining[bucket] -= 1;
                if job.remaining[bucket] == 0
                    && self.active[bucket].fetch_sub(1, Ordering::AcqRel) == 1
                    && self.inline
                    && !recovering
                {
                    self.child(job, bucket, child);
                }
            }
            bucket += 1;
        }
    }

    /// Invokes only an exact disjoint bucket borrow, never under physical locks.
    fn child(
        &self,
        job: &mut Worker<T>,
        bucket: usize,
        child: &(impl Fn(&mut tessera::Workspace<T>, &mut [T]) + Sync),
    ) {
        if self.failed.load(Ordering::Acquire) {
            return;
        }
        let begin = if bucket == 0 {
            0
        } else {
            self.ends[bucket - 1]
        };
        let len = self.ends[bucket] - begin;
        let result = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: inline publication requires the final AcqRel contribution
            // of ALL covering blocks, including shared boundaries and tail.
            // Barrier children run only after full restoration join. Buckets
            // are disjoint and called once, with no widened mutable reborrow.
            let values =
                unsafe { std::slice::from_raw_parts_mut(self.memory.input.add(begin), len) };
            child(&mut job.workspace, values);
        }));
        if let Err(payload) = result {
            self.failed.store(true, Ordering::Release);
            // This job cannot invoke a second child after recording its panic.
            job.panic = Some(payload);
        }
    }
}

#[cfg(test)]
mod tests;
