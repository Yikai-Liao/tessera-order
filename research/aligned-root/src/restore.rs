// SPDX-License-Identifier: BSD-2-Clause
//! Packed whole-block restoration; long cycles use pre-staged boundary blocks.
use super::*;
struct Piece {
    nodes: Vec<usize>,
    snapshot: Option<usize>,
}
struct Job<T> {
    pieces: Vec<Piece>,
    temp: Vec<MaybeUninit<T>>,
    load: usize,
}
struct Snapshots<T>(Vec<MaybeUninit<T>>);
// SAFETY: initialized or uninitialized cells are read only via raw copies after
// snapshot construction. Disjoint jobs never mutate this allocation.
unsafe impl<T: Send> Sync for Snapshots<T> {}
impl<T> Snapshots<T> {
    fn at(&self, offset: usize) -> *const MaybeUninit<T> {
        // SAFETY: every offset comes from a complete preallocated snapshot block.
        unsafe { self.0.as_ptr().add(offset) }
    }
}
pub(super) fn parallel<T: Record>(
    memory: &Memory<T>,
    directory: &[usize],
    total: usize,
    n: usize,
    workers: usize,
    execute_parallel: bool,
) -> usize {
    let b = memory.block;
    let mut sources = vec![usize::MAX; total];
    let mut used = vec![false; total];
    for (target, &source) in directory.iter().take(memory.full).enumerate() {
        sources[target] = source;
        assert!(!used[source]);
        used[source] = true;
    }
    // The partial final block remains fixed in scratch until all full blocks
    // are restored. It is never copied as uninitialized padding into the input.
    if n % b != 0 {
        let tail = directory[memory.full];
        assert!(tail >= memory.full);
        sources[tail] = tail;
        assert!(!used[tail]);
        used[tail] = true;
    }
    let mut unused = used
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| (!v).then_some(i));
    for source in &mut sources {
        if *source == usize::MAX {
            *source = unused.next().unwrap();
        }
    }
    assert!(unused.next().is_none());
    let workers = workers.max(1).min(total.max(1));
    let cap = total.div_ceil(workers.saturating_mul(4)).max(2);
    let mut jobs: Vec<_> = (0..workers)
        .map(|_| Job {
            pieces: Vec::new(),
            temp: cells::<T>(b),
            load: 0,
        })
        .collect();
    let mut visited = vec![false; total];
    let mut boundaries = Vec::new();
    let mut copies = 0;
    for start in 0..total {
        if visited[start] {
            continue;
        }
        let mut nodes = Vec::new();
        let mut at = start;
        while !visited[at] {
            visited[at] = true;
            nodes.push(at);
            at = sources[at];
        }
        assert_eq!(at, start);
        if nodes.len() < 2 {
            continue;
        }
        let mut add = |piece: Piece| {
            let job = jobs.iter_mut().min_by_key(|j| j.load).unwrap();
            job.load += piece.nodes.len();
            job.pieces.push(piece);
        };
        copies += nodes.len() * b;
        if nodes.len() <= cap {
            copies += b;
            add(Piece {
                nodes,
                snapshot: None,
            });
        } else {
            let base = boundaries.len();
            let pieces = nodes.len().div_ceil(cap);
            boundaries.extend(nodes.chunks(cap).map(|c| c[0]));
            copies += pieces * b;
            for (i, chunk) in nodes.chunks(cap).enumerate() {
                add(Piece {
                    nodes: chunk.to_vec(),
                    snapshot: Some((base + (i + 1) % pieces) * b),
                });
            }
        }
    }
    assert!(boundaries.len() <= 8 * workers);
    let mut snapshots = Snapshots(cells::<T>(boundaries.len().checked_mul(b).unwrap()));
    for (i, &source) in boundaries.iter().enumerate() {
        // SAFETY: physical and snapshot blocks are disjoint B-cell allocations.
        // MaybeUninit preserves padding in unused scratch without reading T.
        unsafe {
            std::ptr::copy_nonoverlapping(
                memory.ptr(source),
                snapshots.0.as_mut_ptr().add(i * b),
                b,
            );
        }
    }
    let run = |job: &mut Job<T>| {
        for piece in &job.pieces {
            let final_source = if let Some(offset) = piece.snapshot {
                snapshots.at(offset)
            } else {
                // SAFETY: this short cycle is wholly owned by one job, and its
                // temp block is disjoint from all physical and snapshot blocks.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        memory.ptr(piece.nodes[0]),
                        job.temp.as_mut_ptr(),
                        b,
                    );
                }
                job.temp.as_ptr()
            };
            for pair in piece.nodes.windows(2) {
                // SAFETY: cycle nodes are distinct. Each source is read before
                // this job overwrites it; all inter-job sources are snapshotted.
                // Every input destination's planned source is a full live block.
                unsafe {
                    std::ptr::copy_nonoverlapping(memory.ptr(pair[1]), memory.ptr(pair[0]), b);
                }
            }
            // SAFETY: the final source is disjoint temporary/snapshot storage.
            // The planning phase assigned this destination to this job only.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    final_source,
                    memory.ptr(*piece.nodes.last().unwrap()),
                    b,
                );
            }
        }
    };
    execute_jobs(&mut jobs, execute_parallel, run);
    let tail = n % b;
    if tail != 0 {
        // SAFETY: the final tail is initialized reserved scratch, kept fixed by
        // the permutation. The original tail has a separate disjoint address.
        unsafe {
            std::ptr::copy_nonoverlapping(
                memory.ptr(directory[memory.full]).cast::<T>(),
                memory.input.add(memory.full * b),
                tail,
            );
        }
        copies += tail;
    }
    copies
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_short_split_cycles_and_uninitialized_spares() {
        for execute_parallel in [false, true] {
            for (directory, total) in [
                (vec![1, 2, 3, 4, 5, 6, 0, 8, 7, 9], 10),
                (vec![12, 1, 5, 0, 8, 3, 7, 4, 9, 14], 15),
            ] {
                let b = 3;
                let full = 9;
                let n = full * b + 2;
                let mut input: Vec<u64> = (0..n as u64).collect();
                let mut scratch = cells::<u64>((total - full) * b);
                for (logical, &physical) in directory.iter().enumerate() {
                    if physical >= full {
                        let len = if logical == full { 2 } else { b };
                        for offset in 0..len {
                            scratch[(physical - full) * b + offset]
                                .write((physical * b + offset) as u64);
                        }
                    }
                }
                let expected: Vec<u64> = directory
                    .iter()
                    .enumerate()
                    .flat_map(|(logical, &physical)| {
                        (0..if logical == full { 2 } else { b })
                            .map(move |offset| (physical * b + offset) as u64)
                    })
                    .collect();
                let memory = Memory {
                    input: input.as_mut_ptr(),
                    scratch: scratch.as_mut_ptr(),
                    full,
                    block: b,
                };
                test_pool(4, || {
                    parallel(&memory, &directory, total, n, 4, execute_parallel)
                });
                assert_eq!(input, expected);
            }
        }
    }
}
