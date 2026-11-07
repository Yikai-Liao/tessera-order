// SPDX-License-Identifier: BSD-2-Clause
//! Research-only aligned native root partition for sealed, fixed-key Copy records.
//!
//! Histogram and scatter use the same pure intrinsic key. This is NOT a generic
//! key-function API: arbitrary callbacks would require mismatch recovery. Output
//! consists of real contiguous bucket slices. Allocation or scheduler panic may
//! leave valid initialized but unsorted/duplicated Copy values, not a permutation.
//! Scratch is O(P R B) records; metadata is O(n/B + P R), where P is the shard
//! count, R the radix, and B balances these terms with explicit constant floors.
use rayon::prelude::*;
use std::{
    mem::MaybeUninit,
    time::{Duration, Instant},
};

mod broker;
pub mod probe;
mod restore;

mod sealed {
    pub trait Sealed {}
    impl Sealed for u64 {}
    impl Sealed for super::Row16 {}
    impl Sealed for super::Row128 {}
}
/// Record supported by the fixed-key research kernel; external implementations are sealed.
pub trait Record: sealed::Sealed + Copy + Send + Sync {
    /// Returns the intrinsic unsigned key, consistently across both passes.
    fn key(self) -> u64;
    /// Sorts a materialized child using the matching existing serial API.
    fn sort_child(workspace: &mut tessera::Workspace<Self>, values: &mut [Self]);
}
impl Record for u64 {
    #[inline]
    fn key(self) -> u64 {
        self
    }
    fn sort_child(workspace: &mut tessera::Workspace<Self>, values: &mut [Self]) {
        workspace.stable_sort(values);
    }
}
/// Fixed-key record with an original-position payload for stability checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Row16 {
    pub key: u64,
    pub id: u64,
}
impl Record for Row16 {
    #[inline]
    fn key(self) -> u64 {
        self.key
    }
    fn sort_child(workspace: &mut tessera::Workspace<Self>, values: &mut [Self]) {
        workspace.stable_sort_by_key(values, |record| record.key);
    }
}
/// Wide Copy record with a stable ID and payload checked by full equality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Row128 {
    pub key: u64,
    pub id: u64,
    pub payload: [u64; 14],
}
impl Record for Row128 {
    fn key(self) -> u64 {
        self.key
    }
    fn sort_child(workspace: &mut tessera::Workspace<Self>, values: &mut [Self]) {
        workspace.stable_sort_by_key(values, |record| record.key);
    }
}
/// Experimental root controls. Production routing is deliberately not implied.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub shards: usize,
    pub radix_bits: usize,
    pub shift: usize,
    pub block_size: Option<usize>,
    pub parallel: bool,
    pub parallel_restore: bool,
    pub broker_restore: bool,
    pub capture_plan: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            shards: if cfg!(miri) {
                1
            } else {
                rayon::current_num_threads()
            },
            radix_bits: 8,
            shift: 56,
            block_size: None,
            parallel: !cfg!(miri),
            parallel_restore: false,
            broker_restore: false,
            capture_plan: false,
        }
    }
}
/// Stage wall times (including their allocations) and logical record-copy counts.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub histogram: Duration,
    pub plan: Duration,
    pub scatter: Duration,
    pub restore: Duration,
    pub children: Duration,
    pub block_size: usize,
    pub shards: usize,
    pub boundary_blocks: usize,
    pub scratch_records: usize,
    pub scatter_records: usize,
    pub restore_records: usize,
    pub probe: Option<probe::Report>,
    pub broker: Option<broker::Report>,
}

#[derive(Clone, Copy)]
struct Memory<T> {
    input: *mut T,
    scratch: *mut MaybeUninit<T>,
    full: usize,
    block: usize,
}
// SAFETY: addresses derive from one original mutable input ancestor and one
// scratch allocation. Each phase assigns disjoint writes and joins before reuse.
unsafe impl<T: Send> Send for Memory<T> {}
// SAFETY: the wrapper forms no shared T references. Scatter only reads original
// shard ranges before retiring them; every output cell has exactly one writer.
unsafe impl<T: Send> Sync for Memory<T> {}
impl<T> Memory<T> {
    fn ptr(&self, id: usize) -> *mut MaybeUninit<T> {
        assert!(id != usize::MAX);
        // SAFETY: checked construction bounds all physical IDs by full + scratch
        // blocks. Input full blocks exclude its partial tail.
        unsafe {
            if id < self.full {
                self.input.add(id * self.block).cast()
            } else {
                self.scratch.add((id - self.full) * self.block)
            }
        }
    }
}
struct Directory(*mut usize);
// SAFETY: only body blocks are written, and each body belongs to one segment;
// boundary mappings are initialized once before workers and never changed.
unsafe impl Sync for Directory {}
impl Directory {
    fn set(&self, logical: usize, physical: usize) {
        // SAFETY: construction gives each logical full-body block one owner;
        // directory bounds follow from the checked rank interval.
        unsafe {
            self.0.add(logical).write(physical);
        }
    }
}
fn cells<T>(n: usize) -> Vec<MaybeUninit<T>> {
    let mut v = Vec::with_capacity(n);
    v.resize_with(n, MaybeUninit::uninit);
    v
}
// Miri uses standard scoped threads only to bypass the separately reproduced
// Crossbeam Stacked-Borrows failure. Production and Miri run the identical
// ownership plan and movement closure; Rayon integration has ordinary tests.
fn execute_jobs<X: Send>(jobs: &mut [X], parallel: bool, run: impl Fn(&mut X) + Sync) {
    if parallel {
        #[cfg(miri)]
        std::thread::scope(|scope| {
            for job in jobs {
                let run = &run;
                scope.spawn(move || run(job));
            }
        });
        #[cfg(not(miri))]
        jobs.par_iter_mut().for_each(&run);
    } else {
        jobs.iter_mut().for_each(run);
    }
}
#[cfg(test)]
fn test_pool<R: Send>(threads: usize, run: impl FnOnce() -> R + Send) -> R {
    #[cfg(miri)]
    {
        let _ = threads;
        run()
    }
    #[cfg(not(miri))]
    {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(run)
    }
}

struct Cursor<T> {
    next: *mut MaybeUninit<T>,
    end: *mut MaybeUninit<T>,
    logical: usize,
    first_offset: usize,
    end_rank: usize,
}
// SAFETY: a cursor is owned by one shard. Its writable interval ends at its
// own segment boundary, even when the physical block has other shard writers.
// All physical pointers derive from the shared original allocation ancestors.
unsafe impl<T: Send> Send for Cursor<T> {}
struct Shard<T> {
    first: usize,
    end: usize,
    cursors: Vec<Cursor<T>>,
    free: Vec<usize>,
}

/// Stably partitions records and returns cumulative ends of materialized buckets.
///
/// The selected digit must fit a u64. Tiny roots and multiple cuts in one block
/// use the same native layout; no n-sized record buffer or virtual child API exists.
pub fn partition<T: Record>(arr: &mut [T], config: Config) -> (Vec<usize>, Stats) {
    assert!((1..=12).contains(&config.radix_bits));
    assert!(config.shift <= 64 - config.radix_bits);
    let n = arr.len();
    let r = 1usize << config.radix_bits;
    if n == 0 {
        return (vec![0; r], Stats::default());
    }
    let p = config.shards.max(1).min(n);
    let size = size_of::<T>();
    let balanced = ((24.0 * n as f64) / (2.0 * p as f64 * r as f64 * size as f64))
        .sqrt()
        .ceil() as usize;
    let b = config
        .block_size
        .unwrap_or(balanced.max(1024 / size).max(32));
    assert!(b > 0);
    let f = n / b;
    let p = p.min(f.max(1));
    let m = n.div_ceil(b);
    let mut stats = Stats {
        block_size: b,
        shards: p,
        ..Stats::default()
    };
    let mut ranges = Vec::with_capacity(p);
    let mut first = 0;
    for j in 0..p {
        let count = f / p + usize::from(j < f % p);
        let end = if j + 1 == p { n } else { (first + count) * b };
        ranges.push((first * b, end));
        first += count;
    }
    let clock = Instant::now();
    // Read-only borrow is retired before any scatter write begins.
    let count = |&(begin, end): &(usize, usize)| {
        let mut counts = vec![0usize; r];
        for record in &arr[begin..end] {
            counts[((record.key() >> config.shift) as usize) & (r - 1)] += 1;
        }
        counts
    };
    let counts: Vec<Vec<usize>> = if config.parallel && !cfg!(miri) {
        ranges.par_iter().map(count).collect()
    } else {
        ranges.iter().map(count).collect()
    };
    stats.histogram = clock.elapsed();
    // All future raw accesses derive from this single ancestor. Do not create
    // input sub-slices or reborrow arr until every raw worker has joined.
    let _ = p
        .checked_mul(r)
        .and_then(|x| x.checked_mul(2))
        .and_then(|x| x.checked_mul(b))
        .and_then(|x| x.checked_mul(size))
        .expect("scratch capacity overflow");
    let input = arr.as_mut_ptr();
    let clock = Instant::now();
    let mut starts = vec![vec![0usize; r]; p];
    let mut ends = Vec::with_capacity(r);
    let mut directory = vec![usize::MAX; m];
    let mut boundary_count = 0;
    let mut rank = 0usize;
    for bucket in 0..r {
        for j in 0..p {
            starts[j][bucket] = rank;
            // Zero-length segments can share boundaries. A block is reserved
            // exactly once, regardless of how many shard/bucket cuts it holds.
            if rank < n && rank % b != 0 && directory[rank / b] == usize::MAX {
                directory[rank / b] = f + boundary_count;
                boundary_count += 1;
            }
            rank = rank.checked_add(counts[j][bucket]).expect("rank overflow");
        }
        ends.push(rank);
    }
    assert_eq!(rank, n);
    if n % b != 0 && directory[f] == usize::MAX {
        directory[f] = f + boundary_count;
        boundary_count += 1;
    }
    assert!(boundary_count <= p.checked_mul(r).unwrap());
    let scratch_blocks = boundary_count
        .checked_add(p.checked_mul(r).unwrap())
        .unwrap();
    let scratch_len = scratch_blocks.checked_mul(b).unwrap();
    let mut scratch = cells::<T>(scratch_len);
    let memory = Memory {
        input,
        scratch: scratch.as_mut_ptr(),
        full: f,
        block: b,
    };
    let mut shards = Vec::with_capacity(p);
    for j in 0..p {
        let (first, end) = ranges[j];
        let mut free = Vec::with_capacity((end - first) / b + r);
        free.extend((0..r).map(|k| f + boundary_count + j * r + k));
        let cursors = starts[j]
            .iter()
            .zip(&counts[j])
            .map(|(&start, &count)| Cursor {
                next: std::ptr::null_mut(),
                end: std::ptr::null_mut(),
                logical: start / b,
                first_offset: start % b,
                end_rank: start + count,
            })
            .collect();
        shards.push(Shard {
            first,
            end,
            cursors,
            free,
        });
    }
    let boundary_map = directory.clone();
    let output = Directory(directory.as_mut_ptr());
    stats.plan = clock.elapsed();
    let clock = Instant::now();
    let scatter = |shard: &mut Shard<T>| {
        for source_begin in (shard.first..shard.end).step_by(b) {
            let source_end = (source_begin + b).min(shard.end);
            for at in source_begin..source_end {
                // SAFETY: this shard exclusively reads its original interval.
                // Its free pool never contains an unconsumed source block.
                let record = unsafe { input_read(&memory, at) };
                let bucket = ((record.key() >> config.shift) as usize) & (r - 1);
                let cursor = &mut shard.cursors[bucket];
                if cursor.next == cursor.end {
                    let logical = cursor.logical;
                    let base_rank = logical.checked_mul(b).unwrap();
                    // These checks are per block, not per record. Even the last
                    // shared-boundary fragment stops at this segment's own end.
                    assert!(base_rank < cursor.end_rank);
                    let end_offset = (cursor.end_rank - base_rank).min(b);
                    assert!(cursor.first_offset < end_offset);
                    let id = if boundary_map[logical] != usize::MAX {
                        boundary_map[logical]
                    } else {
                        assert_eq!(cursor.first_offset, 0);
                        assert_eq!(end_offset, b);
                        // R seeds cover at most R active bodies while the
                        // current source block is still being consumed.
                        let id = shard.free.pop().expect("proved shard pool bound");
                        output.set(logical, id);
                        id
                    };
                    let physical = memory.ptr(id);
                    // SAFETY: both bounds lie inside this physical block. The
                    // fragment endpoint is capped to the owning segment, not
                    // merely to B, so no pointer advances into a foreign range.
                    unsafe {
                        cursor.next = physical.add(cursor.first_offset);
                        cursor.end = physical.add(end_offset);
                    }
                    cursor.first_offset = 0;
                    cursor.logical += 1;
                }
                // SAFETY: the cursor is inside this segment's assigned record
                // interval. A full block/fragment triggers allocation before the
                // next write; cursor.end may be one-past but is never written.
                unsafe {
                    cursor.next.write(MaybeUninit::new(record));
                    cursor.next = cursor.next.add(1);
                }
            }
            // The original short tail is never recycled as a full block.
            if source_end - source_begin == b {
                shard.free.push(source_begin / b);
            }
        }
        for cursor in &shard.cursors {
            if cursor.next.is_null() {
                assert_eq!(cursor.logical * b + cursor.first_offset, cursor.end_rank);
            } else {
                assert_eq!(cursor.next, cursor.end);
                assert_eq!(cursor.logical, cursor.end_rank.div_ceil(b));
            }
        }
    };
    execute_jobs(&mut shards, config.parallel, scatter);
    stats.scatter = clock.elapsed();
    assert!(directory.iter().all(|&id| id != usize::MAX));
    if config.capture_plan {
        stats.probe = Some(probe::analyze(
            &directory,
            f + scratch_blocks,
            n,
            b,
            p,
            &ends,
        ));
    }
    let clock = Instant::now();
    stats.restore_records = if config.broker_restore {
        let result = broker::restore(
            &memory,
            &mut directory,
            f + scratch_blocks,
            n,
            p,
            config.parallel,
            &ends,
            config.capture_plan,
        );
        let copies = result.copy_records;
        stats.broker = Some(result);
        copies
    } else if config.parallel_restore {
        restore::parallel(
            &memory,
            &directory,
            f + scratch_blocks,
            n,
            p,
            config.parallel,
        )
    } else {
        restore_serial(&memory, &mut directory, f + scratch_blocks, n)
    };
    stats.restore = clock.elapsed();
    stats.boundary_blocks = boundary_count;
    stats.scratch_records = scratch_len;
    stats.scatter_records = n;
    (ends, stats)
}
/// Reads an original record before its containing source block is retired.
///
/// # Safety
/// The caller owns the shard containing index and has not retired that block.
unsafe fn input_read<T: Copy>(memory: &Memory<T>, index: usize) -> T {
    // SAFETY: caller proves an initialized original input record remains here.
    unsafe { memory.input.add(index).read() }
}
fn restore_serial<T: Record>(
    memory: &Memory<T>,
    directory: &mut [usize],
    total: usize,
    n: usize,
) -> usize {
    let mut inverse = vec![usize::MAX; total];
    for (logical, &physical) in directory.iter().enumerate() {
        assert_eq!(inverse[physical], usize::MAX);
        inverse[physical] = logical;
    }
    let b = memory.block;
    let mut temp = cells::<T>(b);
    let mut copies = 0;
    for destination in 0..memory.full {
        let source = directory[destination];
        if source == destination {
            continue;
        }
        let displaced = inverse[destination];
        if displaced != usize::MAX {
            // SAFETY: both blocks are distinct and valid for B cells. All full
            // logical sources are initialized, and temporary storage is disjoint.
            unsafe {
                std::ptr::copy_nonoverlapping(memory.ptr(destination), temp.as_mut_ptr(), b);
            }
            copies += b;
        }
        // SAFETY: this full logical output is initialized. Its source differs
        // from destination. Writing Copy values leaves initialized input on panic.
        unsafe {
            std::ptr::copy_nonoverlapping(memory.ptr(source), memory.ptr(destination), b);
        }
        copies += b;
        if displaced != usize::MAX {
            // SAFETY: temporary contains exactly the displaced initialized block;
            // source now becomes that logical block's new unique location.
            unsafe {
                std::ptr::copy_nonoverlapping(temp.as_ptr(), memory.ptr(source), b);
            }
            directory[displaced] = source;
            inverse[source] = displaced;
            copies += b;
        } else {
            inverse[source] = usize::MAX;
        }
        directory[destination] = destination;
        inverse[destination] = destination;
    }
    let tail = n % b;
    if tail != 0 {
        let source = directory[memory.full];
        assert!(source >= memory.full);
        // SAFETY: the final partial output block is reserved scratch, initialized
        // through tail. The destination is the disjoint original input tail.
        unsafe {
            std::ptr::copy_nonoverlapping(
                memory.ptr(source).cast::<T>(),
                memory.input.add(memory.full * b),
                tail,
            );
        }
        copies += tail;
    }
    copies
}
/// Runs a most-significant root digit then serial Tessera per real bucket.
///
/// Unlike partition, this full-sort entry point requires shift + radix_bits = 64
/// so that concatenating internally sorted root buckets is globally ordered.
pub fn sort<T: Record>(arr: &mut [T], config: Config) -> Stats {
    assert!((1..=12).contains(&config.radix_bits));
    assert_eq!(config.shift, 64 - config.radix_bits);
    let (ends, mut stats) = partition(arr, config);
    let start = Instant::now();
    let mut rest = arr;
    let mut previous = 0;
    let mut buckets = Vec::with_capacity(ends.len());
    for end in ends {
        let (bucket, tail) = rest.split_at_mut(end - previous);
        buckets.push(bucket);
        rest = tail;
        previous = end;
    }
    // Fixed-P groups reuse one existing serial workspace per executing job.
    // The public serial API still performs child preparation, unlike the frozen
    // combined engine's internal prepared-node entry point.
    let chunk = buckets.len().div_ceil(stats.shards.max(1)).max(1);
    let run = |group: &mut [&mut [T]]| {
        let mut workspace = tessera::Workspace::new();
        for bucket in group {
            T::sort_child(&mut workspace, bucket);
        }
    };
    if config.parallel {
        buckets.par_chunks_mut(chunk).for_each(run);
    } else {
        run(&mut buckets);
    }
    stats.children = start.elapsed();
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    fn next(s: &mut u64) -> u64 {
        *s = s.wrapping_add(0x9e3779b97f4a7c15);
        let mut x = *s;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        x ^ (x >> 31)
    }
    #[test]
    fn invalid_config_is_rejected_before_movement() {
        let mut values = [3u64, 1, 2];
        for config in [
            Config {
                shift: usize::MAX,
                radix_bits: 1,
                ..Config::default()
            },
            Config {
                shift: 0,
                radix_bits: usize::MAX,
                ..Config::default()
            },
        ] {
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| partition(
                    &mut values,
                    config
                )))
                .is_err()
            );
            assert_eq!(values, [3, 1, 2]);
        }
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sort(
                &mut values,
                Config {
                    shift: 0,
                    radix_bits: 4,
                    ..Config::default()
                }
            )))
            .is_err()
        );
        assert_eq!(values, [3, 1, 2]);
    }
    #[test]
    fn native_partition_boundary_cases() {
        let lengths: &[usize] = if cfg!(miri) {
            &[1, 2, 3, 7, 17, 33, 67]
        } else {
            &[
                0, 1, 2, 3, 4, 7, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 257, 1009,
            ]
        };
        let blocks: &[usize] = if cfg!(miri) {
            &[1, 3, 16]
        } else {
            &[1, 2, 3, 7, 16, 31, 256]
        };
        for &n in lengths {
            for &b in blocks {
                for p in [1, 2, 8] {
                    for mode in 0..4 {
                        let mut state = 123;
                        let mut rows: Vec<_> = (0..n)
                            .map(|i| Row16 {
                                key: match mode {
                                    0 => next(&mut state) % 16,
                                    1 => 0,
                                    2 => (i % 2) as u64,
                                    _ => (i / 3 % 16) as u64,
                                },
                                id: i as u64,
                            })
                            .collect();
                        let mut expected = rows.clone();
                        expected.sort_by_key(|r| r.key & 15);
                        let config = Config {
                            shards: p,
                            radix_bits: 4,
                            shift: 0,
                            block_size: Some(b),
                            parallel: false,
                            parallel_restore: false,
                            broker_restore: false,
                            capture_plan: false,
                        };
                        let original = rows.clone();
                        let (ends, _) = partition(&mut rows, config);
                        let mut alternative = original;
                        partition(
                            &mut alternative,
                            Config {
                                parallel_restore: true,
                                broker_restore: false,
                                capture_plan: false,
                                ..config
                            },
                        );
                        assert_eq!(alternative, expected);
                        assert_eq!(rows, expected, "n={n} B={b} P={p} mode={mode}");
                        let mut prev = 0;
                        for (bucket, end) in ends.into_iter().enumerate() {
                            assert!(rows[prev..end].iter().all(|r| r.key as usize == bucket));
                            prev = end;
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn full_stable_sort_reference() {
        for n in [17, 257, 4097] {
            let mut s = 17;
            let mut rows: Vec<_> = (0..n)
                .map(|i| Row16 {
                    key: next(&mut s) % 1024,
                    id: i as u64,
                })
                .collect();
            let mut expected = rows.clone();
            expected.sort_by_key(|r| r.key);
            sort(
                &mut rows,
                Config {
                    shards: 4,
                    radix_bits: 4,
                    shift: 60,
                    block_size: Some(31),
                    parallel: false,
                    parallel_restore: false,
                    broker_restore: false,
                    capture_plan: false,
                },
            );
            assert_eq!(rows, expected);
        }
    }
    #[test]
    fn actual_parallel_tiny_boundary_and_restore() {
        for (n, block_size, shards) in [(29, 3, 2), (67, 16, 4), (33, 16, 2)] {
            let mut rows: Vec<_> = (0..n)
                .map(|i| Row16 {
                    key: (i * 7 % 16) as u64,
                    id: i as u64,
                })
                .collect();
            let mut expected = rows.clone();
            expected.sort_by_key(|r| r.key);
            test_pool(shards, || {
                partition(
                    &mut rows,
                    Config {
                        shards,
                        radix_bits: 4,
                        shift: 0,
                        block_size: Some(block_size),
                        parallel: true,
                        parallel_restore: true,
                        broker_restore: false,
                        capture_plan: false,
                    },
                )
            });
            assert_eq!(rows, expected);
        }
    }
    #[test]
    #[cfg(not(miri))]
    fn parallel_stable_reference() {
        for threads in [1, 2, 4, 8] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            for mode in 0..4 {
                let n = 20003;
                let mut s = 77;
                let mut rows: Vec<_> = (0..n)
                    .map(|i| Row16 {
                        key: match mode {
                            0 => next(&mut s),
                            1 => next(&mut s) % 1024,
                            2 => 0,
                            _ => (i % 8) as u64,
                        },
                        id: i as u64,
                    })
                    .collect();
                let mut expected = rows.clone();
                expected.sort_by_key(|r| r.key);
                pool.install(|| {
                    sort(
                        &mut rows,
                        Config {
                            shards: threads,
                            radix_bits: 8,
                            shift: 56,
                            block_size: Some(31),
                            parallel: true,
                            parallel_restore: true,
                            broker_restore: false,
                            capture_plan: false,
                        },
                    )
                });
                assert_eq!(rows, expected);
            }
        }
    }
}

#[cfg(test)]
mod region_tests {
    use super::*;
    fn next(s: &mut u64) -> u64 {
        *s = s.wrapping_add(0x9e3779b97f4a7c15);
        let mut x = *s;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        x ^ (x >> 31)
    }
    #[test]
    fn broker_stable_boundaries_and_modes() {
        let lengths: &[usize] = if cfg!(miri) {
            &[1, 7, 17, 33]
        } else {
            &[1, 2, 3, 7, 8, 9, 17, 31, 32, 33, 63, 64, 65, 257, 1027]
        };
        let shapes = if cfg!(miri) { 3 } else { 7 };
        for &n in lengths {
            for shape in 0..shapes {
                let mut seed = 9;
                let input: Vec<_> = (0..n)
                    .map(|id| {
                        let raw = next(&mut seed);
                        let key = match shape {
                            0 => raw,
                            1 => (raw % 3) << 61,
                            2 => 0,
                            3 => ((id as u64) << 56) / n as u64,
                            4 => (((n - id) as u64) << 56) / n as u64,
                            5 => {
                                if raw % 100 < 99 {
                                    0
                                } else {
                                    raw
                                }
                            }
                            _ => (raw % 16) << 60,
                        };
                        Row16 { key, id: id as u64 }
                    })
                    .collect();
                for p in [1, 3] {
                    for parallel in [false, true] {
                        let mut values = input.clone();
                        let mut expected = input.clone();
                        expected.sort_by_key(|r| r.key >> 61);
                        let config = Config {
                            shards: p,
                            radix_bits: 3,
                            shift: 61,
                            block_size: Some(4),
                            parallel,
                            parallel_restore: false,
                            broker_restore: true,
                            capture_plan: true,
                        };
                        let (ends, stats) = test_pool(p, || partition(&mut values, config));
                        assert_eq!(values, expected);
                        assert_eq!(ends.last(), Some(&n));
                        assert!(stats.broker.as_ref().unwrap().temporary_records <= p * 4);
                        assert_eq!(
                            stats
                                .probe
                                .as_ref()
                                .unwrap()
                                .bucket_records
                                .iter()
                                .sum::<usize>(),
                            n
                        );
                        // Same root also integrates with the current serial child.
                        if !cfg!(miri) {
                            let mut values = input.clone();
                            let mut expected = input.clone();
                            expected.sort_by_key(|r| r.key);
                            test_pool(p, || sort(&mut values, config));
                            assert_eq!(values, expected);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn broker_wide_record_payload_stability() {
        let mut seed = 11;
        let input: Vec<_> = (0..137)
            .map(|id| Row128 {
                key: (next(&mut seed) % 8) << 61,
                id,
                payload: [id.wrapping_mul(17); 14],
            })
            .collect();
        let mut expected = input.clone();
        expected.sort_by_key(|r| r.key);
        let mut values = input;
        test_pool(3, || {
            partition(
                &mut values,
                Config {
                    shards: 3,
                    radix_bits: 3,
                    shift: 61,
                    block_size: Some(7),
                    parallel: true,
                    parallel_restore: false,
                    broker_restore: true,
                    capture_plan: true,
                },
            )
        });
        assert_eq!(values, expected);
    }
}
