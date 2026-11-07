// SPDX-License-Identifier: BSD-2-Clause
//! Experimental owned-record rank merge, limited to u64 keys. Unvalidated source.
use rayon::prelude::*;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "parallel-restore")]
mod parallel_restore;
use std::time::{Duration, Instant};
#[cfg(feature = "padded")]
mod aligned;
#[cfg(feature = "padded")]
use aligned::HotVec;
#[cfg(not(feature = "padded"))]
type HotVec<T> = Vec<T>;

use std::any::Any;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::AtomicUsize;
#[cfg(not(all(
    feature = "padded",
    feature = "cursor-cache",
    feature = "inline-step",
    feature = "pull-restore",
    feature = "parallel-restore",
    feature = "loser-tree"
)))]
compile_error!("This isolated owned prototype requires all six default kernel features");

// Unexpected internal unwinding while values have logical holes cannot expose
// those holes to the caller. User key panics are caught before reaching this
// guard, drained, and resumed only after restoration. Internal invariant bugs
// abort rather than unwinding through invalid input ownership.
struct MovementGuard {
    armed: bool,
}
impl Drop for MovementGuard {
    fn drop(&mut self) {
        if self.armed {
            std::process::abort();
        }
    }
}
fn phase_mark(phase: Option<&AtomicUsize>, value: usize) {
    if let Some(phase) = phase {
        phase.store(value, Ordering::SeqCst);
    }
}

/// Structural counters, not measured allocation or performance results.
#[derive(Debug, Default)]
pub struct Stats {
    pub snapshots: usize,
    pub scratch_records: usize,
    pub metadata_slots: usize,
    pub comparisons: usize,
    pub minimum_free: usize,
    pub local_sort: Duration,
    pub planning: Duration,
    pub merge: Duration,
    pub restore: Duration,
    pub merge_worker_mask: u64,
    pub restore_worker_mask: u64,
}
fn cells<T>(n: usize) -> Vec<MaybeUninit<T>> {
    std::alloc::Layout::array::<MaybeUninit<T>>(n).expect("record scratch layout overflow");
    let mut result = Vec::with_capacity(n);
    result.resize_with(n, MaybeUninit::uninit);
    result
}
#[cfg_attr(feature = "padded", repr(align(64)))]
struct Job {
    at: HotVec<usize>,
    end: HotVec<usize>,
    free: Vec<usize>,
    out: Vec<usize>,
    tree: HotVec<usize>,
    heads: HotVec<u64>,
    #[cfg(feature = "loser-tree")]
    losers: HotVec<Candidate>,
    #[cfg(feature = "loser-tree")]
    champion: Candidate,
    #[cfg(feature = "cursor-cache")]
    cursors: HotVec<Cursor>,
    #[cfg(feature = "cursor-cache")]
    output_cursor: Cursor,
    current: Option<usize>,
    fill: usize,
    remaining: usize,
    min_free: usize,
    comparisons: usize,
    panic: Option<Box<dyn Any + Send>>,
}
#[cfg(feature = "loser-tree")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Candidate {
    key: u64,
    run: usize,
}
#[cfg(feature = "loser-tree")]
impl Candidate {
    fn sentinel(p: usize) -> Self {
        Self {
            key: u64::MAX,
            run: p,
        }
    }
}
#[cfg(feature = "loser-tree")]
impl Job {
    #[inline(always)]
    fn replay_losers(&mut self, run: usize, p: usize, base: usize) {
        // The previous champion's run is the only changed leaf. Each node on
        // that leaf's path retains the winner of the opposing subtree. Carry
        // the smaller pair upward and retain the larger pair as the new loser.
        // Sentinel (MAX,p) sorts after every real (including MAX) key/run pair.
        let mut challenger = if self.at[run] == self.end[run] {
            Candidate::sentinel(p)
        } else {
            Candidate {
                key: self.heads[run],
                run,
            }
        };
        let mut node = (base + run) / 2;
        while node > 0 {
            #[cfg(feature = "comparison-counting")]
            {
                self.comparisons += 1;
            }
            let other = self.losers[node];
            if challenger > other {
                self.losers[node] = challenger;
                challenger = other;
            }
            node /= 2;
        }
        self.champion = challenger;
    }
}

const NONE: usize = usize::MAX;
#[cfg(not(feature = "loser-tree"))]
fn winner(a: usize, b: usize, heads: &[u64], _comparisons: &mut usize) -> usize {
    if a == NONE {
        return b;
    }
    if b == NONE {
        return a;
    }
    #[cfg(feature = "comparison-counting")]
    {
        *_comparisons += 1;
    }
    if (heads[a], a) <= (heads[b], b) {
        a
    } else {
        b
    }
}

/// Experimental sequential stable sort of Send records by a u64 key.
/// Key panics after movement are caught, drained without more keys, and resumed
/// after restoration. Unexpected internal unwinding during movement aborts.
/// This prototype is not a production generic API and supports only u64 keys.
pub fn sort<T: Send, F: Fn(&T) -> u64 + Sync>(
    values: &mut [T],
    p: usize,
    b: usize,
    key: F,
) -> Stats {
    sort_impl(values, p, b, 0, false, false, &key, None)
}
/// Experimental Rayon sort with the same u64-key and recovery restrictions.
pub fn sort_parallel<T: Send, F: Fn(&T) -> u64 + Sync>(
    values: &mut [T],
    p: usize,
    b: usize,
    key: F,
) -> Stats {
    sort_impl(values, p, b, 0, true, false, &key, None)
}
fn sort_impl<T: Send, F: Fn(&T) -> u64 + Sync>(
    values: &mut [T],
    p: usize,
    b: usize,
    interleave_seed: u64,
    parallel: bool,
    scoped: bool,
    key: &F,
    test_phase: Option<&AtomicUsize>,
) -> Stats {
    let phase = Instant::now();
    phase_mark(test_phase, 1);
    assert!(p > 0 && b > 0);
    let n = values.len();
    if std::mem::size_of::<T>() == 0 {
        tessera_sort::stable_sort_by_key(values, |value| key(value));
        phase_mark(test_phase, 5);
        return Stats::default();
    }
    if n == 0 {
        return Stats::default();
    }
    let blocks = n.div_ceil(b);
    let p = p.min(blocks);
    let mut bounds = Vec::with_capacity(p.checked_add(1).expect("run count overflow"));
    bounds.push(0);
    for j in 1..=p {
        bounds.push(
            blocks
                .checked_mul(j)
                .expect("rank multiplication overflow")
                .checked_div(p)
                .unwrap()
                .checked_mul(b)
                .expect("boundary multiplication overflow")
                .min(n),
        );
    }
    if parallel {
        // Each local run is formed by splitting exclusive slices, not by sharing
        // a full input reference with workers. Local scratch dies before merge.
        let mut rest = &mut *values;
        let mut runs = Vec::with_capacity(p);
        for range in bounds.windows(2) {
            let (run, tail) = rest.split_at_mut(range[1] - range[0]);
            runs.push(run);
            rest = tail;
        }
        runs.into_par_iter()
            .for_each(|run| tessera_sort::stable_sort_by_key(run, |value| key(value)));
    } else {
        for range in bounds.windows(2) {
            tessera_sort::stable_sort_by_key(&mut values[range[0]..range[1]], |value| key(value));
        }
    }
    let local_sort = phase.elapsed();
    phase_mark(test_phase, 2);
    let phase = Instant::now();
    let mut cuts = vec![vec![0usize; p]; p + 1];
    let mut comparisons = 0;
    for j in 0..p {
        let (before, after) = cuts.split_at_mut(j + 1);
        after[0].copy_from_slice(&before[j]);
        let at = &mut after[0];
        let mut remaining = bounds[j + 1] - bounds[j];
        while remaining > 0 {
            let active = (0..p)
                .filter(|&r| at[r] < bounds[r + 1] - bounds[r])
                .count();
            let delta = remaining.div_ceil(active);
            let mut best: Option<(u64, usize, usize)> = None;
            for r in 0..p {
                let take = delta.min(bounds[r + 1] - bounds[r] - at[r]);
                if take == 0 {
                    continue;
                }
                let candidate = (key(&values[bounds[r] + at[r] + take - 1]), r, take);
                if let Some(previous) = best {
                    comparisons += 1;
                    if candidate < previous {
                        best = Some(candidate);
                    }
                } else {
                    best = Some(candidate);
                }
            }
            let (_, r, take) = best.unwrap();
            at[r] += take;
            remaining -= take;
        }
    }
    // A block can be recycled only if one job owns every source cell in it.
    let mut owner = vec![NONE; blocks];
    let mut shared = vec![false; blocks];
    for j in 0..p {
        for r in 0..p {
            let lo = bounds[r] + cuts[j][r];
            let hi = bounds[r] + cuts[j + 1][r];
            if lo == hi {
                continue;
            }
            for block in lo / b..=(hi - 1) / b {
                if owner[block] == NONE {
                    owner[block] = j;
                } else if owner[block] != j {
                    shared[block] = true;
                }
            }
        }
    }
    if n % b != 0 {
        shared[blocks - 1] = true;
    }
    let snapshots = shared.iter().filter(|&&x| x).count();
    let seeds = p
        .checked_mul(3)
        .and_then(|v| v.checked_add(2))
        .expect("seed count overflow");
    // Two temporary blocks: one permutation buffer and the final output tail.
    let extra = seeds
        .checked_mul(p)
        .and_then(|v| v.checked_add(snapshots))
        .and_then(|v| v.checked_add(2))
        .expect("scratch block count overflow");
    #[cfg(feature = "parallel-restore")]
    let extra = extra
        .checked_add(p.checked_mul(9).expect("restore scratch bound overflow"))
        .expect("total scratch block overflow");
    let scratch_records = extra.checked_mul(b).expect("scratch record count overflow");
    let address_count = blocks.checked_add(extra).expect("address count overflow");
    let mut scratch = cells::<T>(scratch_records);
    let mut addresses = Vec::with_capacity(address_count);
    let input_ptr = values.as_mut_ptr();
    let scratch_ptr = scratch.as_mut_ptr();
    for i in 0..blocks {
        // SAFETY: i*b lies within the exclusively borrowed input allocation.
        addresses.push(unsafe { input_ptr.add(i * b) });
    }
    for i in 0..extra {
        // SAFETY: every scratch block is allocated and the allocation is fixed.
        addresses.push(unsafe { scratch_ptr.add(i * b).cast::<T>() });
    }
    let mut source: Vec<usize> = (0..blocks).collect();
    let mut next = blocks;
    for block in 0..blocks {
        if shared[block] {
            source[block] = next;
            next += 1;
        }
    }
    let tree_base = p
        .checked_next_power_of_two()
        .expect("tournament count overflow");
    let tree_slots = tree_base
        .checked_mul(2)
        .expect("tournament layout overflow");
    let mut jobs = Vec::with_capacity(p);
    for j in 0..p {
        let count = bounds[j + 1] - bounds[j];
        let owned = owner
            .iter()
            .zip(&shared)
            .filter(|&(&o, &s)| o == j && !s)
            .count();
        let mut free =
            Vec::with_capacity(owned.checked_add(seeds).expect("free list size overflow"));
        free.extend(next..next + seeds);
        next += seeds;
        jobs.push(Job {
            at: cuts[j].clone().into(),
            end: cuts[j + 1].clone().into(),
            free,
            out: Vec::with_capacity(count.div_ceil(b)),
            tree: vec![NONE; tree_slots].into(),
            heads: vec![0; p].into(),
            #[cfg(feature = "loser-tree")]
            losers: vec![Candidate::sentinel(p); tree_base].into(),
            #[cfg(feature = "loser-tree")]
            champion: Candidate::sentinel(p),
            #[cfg(feature = "cursor-cache")]
            cursors: vec![Cursor::empty(); p].into(),
            #[cfg(feature = "cursor-cache")]
            output_cursor: Cursor::empty(),
            current: None,
            fill: 0,
            remaining: count,
            min_free: seeds,
            comparisons: 0,
            panic: None,
        });
    }
    #[cfg(not(feature = "parallel-restore"))]
    let temp = next;
    let tail = next + 1;
    let mut output = Vec::with_capacity(blocks);
    let mut location = vec![NONE; addresses.len()];
    #[cfg(all(feature = "pull-restore", not(feature = "parallel-restore")))]
    let mut visited = vec![false; n / b];
    #[cfg(feature = "parallel-restore")]
    let mut restore_plan = parallel_restore::Plan::new(
        n / b,
        p,
        tail.checked_add(1).expect("restore snapshot base overflow"),
    );
    phase_mark(test_phase, 3);
    for job in &mut jobs {
        for r in 0..p {
            if job.at[r] < job.end[r] {
                let pos = bounds[r] + job.at[r];
                // SAFETY: this cursor selects an initialized unconsumed source cell.
                job.heads[r] = unsafe { key(&*addresses[pos / b].add(pos % b)) };
                job.tree[tree_base + r] = r;
                #[cfg(feature = "cursor-cache")]
                {
                    // SAFETY: source mapping is fixed and this cursor is nonempty.
                    let next = unsafe { addresses[source[pos / b]].add(pos % b).cast() };
                    job.cursors[r] = Cursor {
                        next,
                        block: pos / b,
                        left: (b - pos % b).min(job.end[r] - job.at[r]),
                    };
                }
            }
        }
        for node in (1..tree_base).rev() {
            #[cfg(not(feature = "loser-tree"))]
            {
                job.tree[node] = winner(
                    job.tree[node * 2],
                    job.tree[node * 2 + 1],
                    &job.heads,
                    &mut job.comparisons,
                );
            }
            #[cfg(feature = "loser-tree")]
            {
                let make = |run: usize| {
                    if run == NONE {
                        Candidate::sentinel(p)
                    } else {
                        Candidate {
                            key: job.heads[run],
                            run,
                        }
                    }
                };
                let left = make(job.tree[node * 2]);
                let right = make(job.tree[node * 2 + 1]);
                #[cfg(feature = "comparison-counting")]
                {
                    job.comparisons += 1;
                }
                let (win, lose) = if left <= right {
                    (left, right)
                } else {
                    (right, left)
                };
                job.tree[node] = if win.run == p { NONE } else { win.run };
                job.losers[node] = lose;
            }
        }
        #[cfg(feature = "loser-tree")]
        {
            let run = job.tree[1];
            job.champion = if run == NONE {
                Candidate::sentinel(p)
            } else {
                Candidate {
                    key: job.heads[run],
                    run,
                }
            };
        }
    }
    // All metadata and initial keys are complete while original input still
    // owns every record. Snapshot copies below are logical moves: only snapshot
    // slots own the values afterward; original bytes are stale, never dropped.
    let mut guard = MovementGuard { armed: true };
    phase_mark(test_phase, 4);
    for block in 0..blocks {
        if shared[block] {
            let count = b.min(n - block * b);
            // SAFETY: source input contains count initialized records; destination
            // is disjoint allocated snapshot storage, never recycled during merge.
            unsafe {
                std::ptr::copy_nonoverlapping(addresses[block], addresses[source[block]], count);
            }
        }
    }
    let planning = phase.elapsed();
    let phase = Instant::now();
    let addresses = Addresses(addresses);
    let context = MergeContext {
        addresses: &addresses,
        source: &source,
        #[cfg(not(feature = "cursor-cache"))]
        bounds: &bounds,
        shared: &shared,
        owner: &owner,
        b,
        p,
        tree_base,
        key,
    };
    let workers = AtomicU64::new(0);
    if scoped {
        std::thread::scope(|scope| {
            for (j, job) in jobs.iter_mut().enumerate() {
                let context = &context;
                scope.spawn(move || {
                    while job.remaining > 0 {
                        context.step(j, job);
                    }
                });
            }
        });
    } else if parallel {
        jobs.par_iter_mut().enumerate().for_each(|(j, job)| {
            let worker = rayon::current_thread_index().unwrap_or(63).min(63);
            workers.fetch_or(1u64 << worker, Ordering::Relaxed);
            while job.remaining > 0 {
                context.step(j, job);
            }
        });
    } else {
        let mut state = interleave_seed;
        let mut sequential = 0;
        let mut remaining = n;
        while remaining > 0 {
            let j = if interleave_seed == 0 {
                while jobs[sequential].remaining == 0 {
                    sequential += 1;
                }
                sequential
            } else {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let mut j = state as usize % p;
                while jobs[j].remaining == 0 {
                    j = (j + 1) % p;
                }
                j
            };
            context.step(j, &mut jobs[j]);
            remaining -= 1;
        }
    }
    let merge_time = phase.elapsed();
    let phase = Instant::now();
    let addresses = addresses.0;
    for job in &jobs {
        output.extend_from_slice(&job.out);
    }
    let full = n / b;
    if n % b != 0 {
        // SAFETY: final output block contains exactly n%b initialized cells;
        // save them before a full block restoration can overwrite that block.
        unsafe {
            std::ptr::copy_nonoverlapping(addresses[output[full]], addresses[tail], n % b);
        }
    }
    for logical in 0..full {
        assert_eq!(location[output[logical]], NONE);
        location[output[logical]] = logical;
    }
    #[cfg(all(feature = "pull-restore", not(feature = "parallel-restore")))]
    {
        // SAFETY: all full output sources are unique initialized blocks, and
        // input destinations are full-capacity disjoint blocks. The private
        // permutation temporary is absent from every output source and target.
        unsafe {
            restore_pull(
                &addresses,
                &output[..full],
                &location,
                &mut visited,
                b,
                temp,
            );
        }
    }
    #[cfg(feature = "parallel-restore")]
    let (addresses, restore_worker_mask) = {
        let table = Addresses(addresses);
        // SAFETY: output is injective and initialized; targets are full input
        // blocks. The reserved 9P blocks follow temp/tail and were preallocated
        // before merge, and no merge output can name these reserved blocks.
        let mask = unsafe { restore_plan.execute(&table, &output[..full], &location, b, parallel) };
        (table.0, mask)
    };
    #[cfg(not(feature = "parallel-restore"))]
    let restore_worker_mask = 0;
    #[cfg(not(feature = "pull-restore"))]
    {
        // Swap the desired block into place, maintaining the inverse directory.
        // Empty destinations start paths; occupied destinations splice cycles.
        for logical in 0..full {
            let src = output[logical];
            if src == logical {
                continue;
            }
            let displaced = location[logical];
            // SAFETY: src is a full initialized output block. The temporary and
            // target are allocated full blocks, pairwise disjoint here. Only read
            // target when the inverse directory proves it holds live full output.
            unsafe {
                std::ptr::copy_nonoverlapping(addresses[src], addresses[temp], b);
                if displaced != NONE {
                    std::ptr::copy_nonoverlapping(addresses[logical], addresses[src], b);
                }
                std::ptr::copy_nonoverlapping(addresses[temp], addresses[logical], b);
            }
            location[src] = displaced;
            if displaced != NONE {
                output[displaced] = src;
            }
            location[logical] = logical;
            output[logical] = logical;
        }
    }
    if n % b != 0 {
        // SAFETY: saved tail is initialized for the exact remaining slice length.
        unsafe {
            std::ptr::copy_nonoverlapping(addresses[tail], addresses[full], n % b);
        }
    }
    // Every record is back in the original slice before any panic is resumed
    // or any captured panic payload can be dropped. Pick first by job index;
    // concurrent callback timing does not define a deterministic first panic.
    guard.armed = false;
    phase_mark(test_phase, 5);
    if let Some(payload) = jobs.iter_mut().find_map(|job| job.panic.take()) {
        resume_unwind(payload);
    }
    Stats {
        local_sort,
        planning,
        merge: merge_time,
        restore: phase.elapsed(),
        snapshots,
        scratch_records,
        metadata_slots: addresses.capacity()
            + source.capacity()
            + owner.capacity()
            + shared.capacity()
            + output.capacity()
            + location.capacity()
            + {
                #[cfg(all(feature = "pull-restore", not(feature = "parallel-restore")))]
                {
                    visited.capacity()
                }
                #[cfg(any(not(feature = "pull-restore"), feature = "parallel-restore"))]
                {
                    0
                }
            }
            + bounds.capacity()
            + {
                #[cfg(feature = "parallel-restore")]
                {
                    restore_plan.metadata_slots()
                }
                #[cfg(not(feature = "parallel-restore"))]
                {
                    0
                }
            }
            + cuts.iter().map(Vec::capacity).sum::<usize>()
            + jobs
                .iter()
                .map(|j| {
                    j.at.capacity()
                        + j.end.capacity()
                        + j.free.capacity()
                        + j.out.capacity()
                        + j.tree.capacity()
                        + j.heads.capacity()
                        + {
                            #[cfg(feature = "loser-tree")]
                            {
                                j.losers.capacity() * 2
                            }
                            #[cfg(not(feature = "loser-tree"))]
                            {
                                0
                            }
                        }
                        + {
                            #[cfg(feature = "cursor-cache")]
                            {
                                j.cursors.capacity() * 3
                            }
                            #[cfg(not(feature = "cursor-cache"))]
                            {
                                0
                            }
                        }
                })
                .sum::<usize>(),
        comparisons: comparisons + jobs.iter().map(|j| j.comparisons).sum::<usize>(),
        merge_worker_mask: workers.load(Ordering::Relaxed),
        restore_worker_mask,
        minimum_free: jobs.iter().map(|j| j.min_free).min().unwrap(),
    }
}

struct Addresses<T>(Vec<*mut T>);
// SAFETY: pointers are immutable. During merge, snapshots are read-only;
// snapshot record intervals belong to distinct jobs (no T is shared); every
// other source block has exactly one job owner, and only that owner may
// recycle it after consuming its last cell. Seed ranges are pairwise disjoint.
// No reference to a record is retained across an operation or shared with jobs.
unsafe impl<T: Send> Sync for Addresses<T> {}
impl<T> Addresses<T> {
    fn at(&self, id: usize) -> *mut T {
        self.0[id]
    }
}
struct MergeContext<'a, T, F> {
    key: &'a F,
    addresses: &'a Addresses<T>,
    source: &'a [usize],
    #[cfg(not(feature = "cursor-cache"))]
    bounds: &'a [usize],
    shared: &'a [bool],
    owner: &'a [usize],
    b: usize,
    p: usize,
    tree_base: usize,
}
#[cfg(feature = "cursor-cache")]
#[derive(Clone, Copy)]
struct Cursor {
    next: *mut u8,
    block: usize,
    left: usize,
}
#[cfg(feature = "cursor-cache")]
impl Cursor {
    fn empty() -> Self {
        Self {
            next: std::ptr::null_mut(),
            block: 0,
            left: 0,
        }
    }
}
// SAFETY: a cursor is moved only with its uniquely owned job. Input pointers
// reference this job's exclusive blocks or disjoint snapshot record intervals; output pointers
// reference its exclusive seed/recycled blocks. No pointer escapes the joined job.
#[cfg(feature = "cursor-cache")]
unsafe impl Send for Cursor {}
#[cfg(feature = "cursor-cache")]
impl<T: Send, F: Fn(&T) -> u64 + Sync> MergeContext<'_, T, F> {
    #[inline(always)]
    fn step(&self, j: usize, job: &mut Job) {
        let r = if job.panic.is_some() {
            (0..self.p)
                .find(|&r| job.at[r] < job.end[r])
                .expect("recovery cursor missing")
        } else {
            job.champion.run
        };
        assert!(r < self.p && job.at[r] < job.end[r]);
        let cursor = &mut job.cursors[r];
        // SAFETY: this unique cursor owns the next initialized source value.
        // No shared T reference survives its earlier head-key call.
        let value = unsafe { cursor.next.cast::<T>().read() };
        // SAFETY: advance reaches at most one-past allocated source storage.
        cursor.next = unsafe { cursor.next.add(std::mem::size_of::<T>()) };
        cursor.left -= 1;
        job.at[r] += 1;
        if cursor.left == 0 {
            // A partial rank endpoint is necessarily shared; only a complete
            // exclusive source block can enter this worker's free pool.
            if !self.shared[cursor.block] {
                assert_eq!(self.owner[cursor.block], j);
                job.free.push(cursor.block);
            }
            if job.at[r] < job.end[r] {
                cursor.block += 1;
                cursor.next = self.addresses.at(self.source[cursor.block]).cast();
                cursor.left = self.b.min(job.end[r] - job.at[r]);
            }
        }
        if job.current.is_none() {
            let dest = job.free.pop().expect("output seed capacity exhausted");
            job.current = Some(dest);
            job.output_cursor.next = self.addresses.at(dest).cast();
            job.min_free = job.min_free.min(job.free.len());
        }
        // SAFETY: output owns this slot; overwrite stale bytes without dropping.
        // After write the output owns value, not the consumed source cell.
        unsafe {
            job.output_cursor.next.cast::<T>().write(value);
            job.output_cursor.next = job.output_cursor.next.add(std::mem::size_of::<T>());
        }
        job.fill += 1;
        job.remaining -= 1;
        if job.fill == self.b || job.remaining == 0 {
            if job.fill != self.b {
                assert_eq!(j, self.p - 1);
            }
            job.out.push(job.current.take().unwrap());
            job.fill = 0;
        }
        // Commit barrier: no moved T remains in an unrecorded local variable.
        // All ownership/cursor/output bookkeeping precedes every user key call.
        if job.remaining == 0 || job.panic.is_some() {
            return;
        }
        if job.at[r] < job.end[r] {
            let next = job.cursors[r].next.cast::<T>();
            let result = catch_unwind(AssertUnwindSafe(|| {
                // SAFETY: this unconsumed record belongs exclusively to this
                // job, including when allocated in a shared boundary snapshot.
                unsafe { (self.key)(&*next) }
            }));
            match result {
                Ok(head) => job.heads[r] = head,
                Err(payload) => {
                    job.panic = Some(payload);
                    return;
                }
            }
        }
        job.replay_losers(r, self.p, self.tree_base);
    }
}

/// Restores an injective mixed input/scratch directory using paths and cycles.
///
/// # Safety
/// Every accessed address names a disjoint allocated block of at least b T
/// cells; unaccessed original-tail table entries may be shorter. Each
/// output entry is a distinct initialized full block, outside the temp block;
/// every destination 0..output.len() is a full-capacity block and excludes temp.
/// location is the inverse output directory or NONE for unoccupied slots.
/// visited is zeroed and has output.len() entries. No other access may overlap.
#[cfg(all(feature = "pull-restore", any(not(feature = "parallel-restore"), test)))]
unsafe fn restore_pull<T>(
    addresses: &[*mut T],
    output: &[usize],
    location: &[usize],
    visited: &mut [bool],
    b: usize,
    temp: usize,
) {
    let full = output.len();
    assert_eq!(visited.len(), full);
    // An original destination absent from the source directory is a hole. Pull
    // along the unique predecessor path until reaching an external scratch
    // source. This writes every destination once and never overwrites a source
    // before its only consumer has read it. Distinct paths cannot intersect.
    for start in 0..full {
        if location[start] != NONE {
            continue;
        }
        let mut target = start;
        loop {
            assert!(!visited[target]);
            visited[target] = true;
            let source = output[target];
            assert_ne!(source, target);
            // SAFETY: target is the current hole, source its initialized full
            // output block. Source and target are distinct disjoint blocks.
            unsafe {
                std::ptr::copy_nonoverlapping(addresses[source], addresses[target], b);
            }
            if source >= full {
                break;
            }
            target = source;
        }
    }
    // Components remaining after the hole paths are pure original-slot cycles.
    // Save one block to break a cycle, then pull each successor exactly once.
    for start in 0..full {
        if visited[start] {
            continue;
        }
        if output[start] == start {
            visited[start] = true;
            continue;
        }
        // SAFETY: start is occupied in this cycle; temp is allocated, disjoint,
        // and excluded from both output sources and input destinations.
        unsafe {
            std::ptr::copy_nonoverlapping(addresses[start], addresses[temp], b);
        }
        let mut target = start;
        loop {
            assert!(!visited[target]);
            visited[target] = true;
            let source = output[target];
            assert!(source < full);
            if source == start {
                // SAFETY: saved original start is this last target's output.
                unsafe {
                    std::ptr::copy_nonoverlapping(addresses[temp], addresses[target], b);
                }
                break;
            }
            // SAFETY: source is the next unread block in this disjoint cycle;
            // it cannot equal target because singleton cycles were handled.
            unsafe {
                std::ptr::copy_nonoverlapping(addresses[source], addresses[target], b);
            }
            target = source;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::Arc;

    struct Owned {
        key: u64,
        id: usize,
        text: String,
        wide: [u64; 14],
        touches: Cell<usize>,
        drops: Arc<Vec<AtomicUsize>>,
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            self.drops[self.id].fetch_add(1, Ordering::SeqCst);
        }
    }
    fn fixture(n: usize) -> (Vec<Owned>, Arc<Vec<AtomicUsize>>) {
        let drops = Arc::new((0..n).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
        let values = (0..n)
            .map(|id| Owned {
                key: if id % 11 == 0 {
                    u64::MAX
                } else {
                    (id * 37 % 17) as u64
                },
                id,
                text: format!("owned-{id}"),
                wide: [id as u64; 14],
                touches: Cell::new(0),
                drops: drops.clone(),
            })
            .collect();
        (values, drops)
    }
    fn check_permutation(values: &[Owned], drops: &[AtomicUsize]) {
        let mut ids: Vec<_> = values.iter().map(|value| value.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, (0..values.len()).collect::<Vec<_>>());
        for value in values {
            assert_eq!(value.text, format!("owned-{}", value.id));
            assert_eq!(value.wide, [value.id as u64; 14]);
        }
        assert!(drops.iter().all(|count| count.load(Ordering::SeqCst) == 0));
    }
    fn check_drops(values: Vec<Owned>, drops: &[AtomicUsize]) {
        drop(values);
        assert!(drops.iter().all(|count| count.load(Ordering::SeqCst) == 1));
    }
    fn owned_case(n: usize, p: usize, b: usize, mode: usize) {
        let (mut values, drops) = fixture(n);
        let mut expected: Vec<_> = values.iter().map(|r| (r.key, r.id)).collect();
        expected.sort_by_key(|r| r.0);
        let key = |value: &Owned| {
            value.touches.set(value.touches.get() + 1);
            value.key
        };
        sort_impl(&mut values, p, b, 19, mode == 2, mode == 1, &key, None);
        assert_eq!(
            values.iter().map(|r| (r.key, r.id)).collect::<Vec<_>>(),
            expected
        );
        check_permutation(&values, &drops);
        check_drops(values, &drops);
    }
    fn panic_case(mode: usize, target_phase: usize, fail_after: usize) {
        let (mut values, drops) = fixture(97);
        let phase = AtomicUsize::new(0);
        let calls = AtomicUsize::new(0);
        let key = |value: &Owned| {
            value.touches.set(value.touches.get() + 1);
            if phase.load(Ordering::SeqCst) == target_phase
                && calls.fetch_add(1, Ordering::SeqCst) == fail_after
            {
                panic!("injected phase {target_phase}");
            }
            value.key
        };
        let failed = catch_unwind(AssertUnwindSafe(|| {
            sort_impl(
                &mut values,
                3,
                5,
                19,
                mode == 2,
                mode == 1,
                &key,
                Some(&phase),
            )
        }));
        assert!(
            failed.is_err(),
            "key failure must unwind, never silently abort/succeed"
        );
        assert!(calls.load(Ordering::SeqCst) > fail_after);
        if target_phase == 4 {
            assert_eq!(
                phase.load(Ordering::SeqCst),
                5,
                "restore must precede resume"
            );
        }
        check_permutation(&values, &drops);
        check_drops(values, &drops);
    }
    #[test]
    fn owned_string_cell_matrix() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();
        pool.install(|| {
            for mode in [0, 1, 2] {
                for n in [0, 1, 7, 31, 97, 257] {
                    for p in [1, 2, 3, 5] {
                        for b in [1, 3, 8] {
                            owned_case(n, p, b, mode);
                        }
                    }
                }
            }
        });
    }
    #[test]
    fn all_key_phases_recover() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(3)
            .build()
            .unwrap();
        pool.install(|| {
            for mode in [0, 1, 2] {
                for phase in [1, 2, 3, 4] {
                    for after in [0, 1, 3] {
                        panic_case(mode, phase, after);
                    }
                }
            }
        });
    }
    #[test]
    fn inconsistent_keys_preserve_ownership() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(3)
            .build()
            .unwrap();
        pool.install(|| {
            for mode in [0, 1, 2] {
                let (mut values, drops) = fixture(257);
                let calls = AtomicUsize::new(0);
                let key = |row: &Owned| {
                    if calls.fetch_add(1, Ordering::Relaxed) % 2 == 0 {
                        row.key
                    } else {
                        !row.key
                    }
                };
                sort_impl(&mut values, 5, 7, 19, mode == 2, mode == 1, &key, None);
                check_permutation(&values, &drops);
                check_drops(values, &drops);
            }
        });
    }
    #[test]
    fn nested_rayon_key_panic_recovers() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(3)
            .build()
            .unwrap();
        pool.install(|| {
            let (mut values, drops) = fixture(97);
            let phase = AtomicUsize::new(0);
            let calls = AtomicUsize::new(0);
            let key = |row: &Owned| {
                if phase.load(Ordering::SeqCst) == 4 && calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    rayon::join(|| panic!("nested user callback"), || {});
                }
                let k = row.key;
                rayon::join(move || k, || 42).0
            };
            let failed = catch_unwind(AssertUnwindSafe(|| {
                sort_impl(&mut values, 3, 5, 0, true, false, &key, Some(&phase))
            }));
            assert!(failed.is_err());
            assert_eq!(phase.load(Ordering::SeqCst), 5);
            check_permutation(&values, &drops);
            check_drops(values, &drops);
        });
    }
    fn refs_case(scoped: bool, panic_merge: bool) {
        let mut backing: Vec<u64> = (0..31).rev().collect();
        let phase = AtomicUsize::new(0);
        let calls = AtomicUsize::new(0);
        {
            let mut refs: Vec<&mut u64> = backing.iter_mut().collect();
            let key = |row: &&mut u64| {
                if panic_merge
                    && phase.load(Ordering::SeqCst) == 4
                    && calls.fetch_add(1, Ordering::SeqCst) == 1
                {
                    panic!("borrowed row key");
                }
                **row % 7
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                sort_impl(&mut refs, 3, 3, 19, false, scoped, &key, Some(&phase))
            }));
            assert_eq!(result.is_err(), panic_merge);
            let mut original: Vec<_> = refs.iter().map(|row| **row).collect();
            original.sort_unstable();
            assert_eq!(original, (0..31).collect::<Vec<_>>());
            for row in refs {
                *row += 100;
            }
        }
        backing.sort_unstable();
        assert_eq!(backing, (100..131).collect::<Vec<_>>());
    }
    #[test]
    fn exclusive_refs_and_recovery() {
        for scoped in [false, true] {
            for panic_merge in [false, true] {
                refs_case(scoped, panic_merge);
            }
        }
    }
    #[test]
    fn owned_overflow_and_zst() {
        let (mut values, drops) = fixture(1);
        let failed = catch_unwind(AssertUnwindSafe(|| {
            sort(&mut values, 1, 1usize << (usize::BITS - 3), |r| r.key)
        }));
        assert!(failed.is_err());
        check_permutation(&values, &drops);
        check_drops(values, &drops);
        static ZST_DROPS: AtomicUsize = AtomicUsize::new(0);
        struct Zst;
        impl Drop for Zst {
            fn drop(&mut self) {
                ZST_DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }
        ZST_DROPS.store(0, Ordering::SeqCst);
        let mut values: Vec<_> = (0..17).map(|_| Zst).collect();
        sort(&mut values, 3, 2, |_| 0);
        drop(values);
        assert_eq!(ZST_DROPS.load(Ordering::SeqCst), 17);
    }
    fn multiple_job_panics(mode: usize) {
        use std::sync::atomic::AtomicBool;
        struct Payload {
            job: usize,
            phase: Arc<AtomicUsize>,
            payload_drops: Arc<Vec<AtomicUsize>>,
            record_drops: Arc<Vec<AtomicUsize>>,
            early_drop: Arc<AtomicBool>,
        }
        impl Drop for Payload {
            fn drop(&mut self) {
                // Never panic from Drop, including during another panic unwind.
                // Record any ordering violation for the caller to assert later.
                if self.phase.load(Ordering::SeqCst) != 5
                    || self
                        .record_drops
                        .iter()
                        .any(|n| n.load(Ordering::SeqCst) != 0)
                {
                    self.early_drop.store(true, Ordering::SeqCst);
                }
                self.payload_drops[self.job].fetch_add(1, Ordering::SeqCst);
            }
        }
        let (mut values, record_drops) = fixture(97);
        let runs = 3;
        let b = 5;
        let mut ranked: Vec<_> = values.iter().map(|row| (row.key, row.id)).collect();
        ranked.sort_by_key(|row| row.0);
        let blocks = values.len().div_ceil(b);
        let mut owner = vec![usize::MAX; values.len()];
        let mut begin = 0;
        for job in 0..runs {
            let end = (blocks * (job + 1) / runs * b).min(values.len());
            for &(_, id) in &ranked[begin..end] {
                owner[id] = job;
            }
            begin = end;
        }
        assert_eq!(begin, values.len());
        let phase = Arc::new(AtomicUsize::new(0));
        let calls: Vec<_> = (0..runs).map(|_| AtomicUsize::new(0)).collect();
        let payload_drops = Arc::new((0..runs).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
        let early_drop = Arc::new(AtomicBool::new(false));
        let key = |row: &Owned| {
            if phase.load(Ordering::SeqCst) == 4 {
                let job = owner[row.id];
                if calls[job].fetch_add(1, Ordering::SeqCst) == 0 {
                    std::panic::panic_any(Payload {
                        job,
                        phase: phase.clone(),
                        payload_drops: payload_drops.clone(),
                        record_drops: record_drops.clone(),
                        early_drop: early_drop.clone(),
                    });
                }
            }
            row.key
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            sort_impl(
                &mut values,
                runs,
                b,
                19,
                mode == 2,
                mode == 1,
                &key,
                Some(&phase),
            )
        }));
        let payload = match result {
            Err(payload) => match payload.downcast::<Payload>() {
                Ok(payload) => payload,
                Err(_) => panic!("unexpected panic payload"),
            },
            Ok(_) => panic!("all three jobs must independently fail"),
        };
        assert_eq!(phase.load(Ordering::SeqCst), 5);
        assert!(
            calls.iter().all(|n| n.load(Ordering::SeqCst) == 1),
            "failed jobs must drain without more keys"
        );
        check_permutation(&values, &record_drops);
        assert_eq!(payload.job, 0, "panic selection is by job index");
        for job in 0..runs {
            assert_eq!(
                payload_drops[job].load(Ordering::SeqCst),
                usize::from(job != payload.job)
            );
        }
        assert!(!early_drop.load(Ordering::SeqCst));
        // The resumed payload stays alive until complete original-slice
        // permutation/drop checks above. All other payloads were already dropped.
        drop(payload);
        assert!(payload_drops.iter().all(|n| n.load(Ordering::SeqCst) == 1));
        assert!(!early_drop.load(Ordering::SeqCst));
        check_drops(values, &record_drops);
    }
    #[test]
    fn independent_job_panics_restore_before_payload_drop() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(3)
            .build()
            .unwrap();
        pool.install(|| {
            multiple_job_panics(1);
            multiple_job_panics(2);
        });
    }
    #[test]
    fn owned_miri_multiple_job_panics() {
        multiple_job_panics(1);
    }
    #[test]
    fn owned_miri_smoke() {
        owned_case(31, 3, 3, 0);
        owned_case(31, 3, 3, 1);
        panic_case(0, 4, 1);
        panic_case(1, 4, 1);
        refs_case(false, true);
        refs_case(true, true);
    }
}
