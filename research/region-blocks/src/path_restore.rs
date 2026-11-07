// SPDX-License-Identifier: BSD-2-Clause
//! Direct open paths with LPT whole-component assignment and a long-chain fallback.
//! Pull paths/cycles are established machinery in the earlier rank-merge control;
//! this experiment changes their use and scheduling on the actual aligned plan.
use super::*;
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub copy_records: usize,
    pub paths: usize,
    pub cycles: usize,
    pub fixed: usize,
    pub largest_component: usize,
    /// Exact direct-route copy load; zero is unavailable on the fallback route.
    pub max_job_copies: usize,
    /// Exact direct-route temporary slots; a conservative9PB bound on fallback.
    pub temporary_records: usize,
    pub used_split_cycle_fallback: bool,
}
struct Component {
    begin: usize,
    len: usize,
    endpoint: usize,
    cycle: bool,
}
struct Job<T> {
    components: Vec<usize>,
    temp: Vec<MaybeUninit<T>>,
}

pub(super) fn restore<T: Record>(
    memory: &Memory<T>,
    directory: &[usize],
    total: usize,
    n: usize,
    workers: usize,
    parallel: bool,
) -> Report {
    let full = memory.full;
    let b = memory.block;
    assert_eq!(directory.len(), n.div_ceil(b));
    let workers = workers.max(1).min(total.max(1));
    let mut incoming = vec![false; total];
    for &source in directory.iter().take(full) {
        assert!(source < total && !incoming[source]);
        incoming[source] = true;
    }
    if n % b != 0 {
        let tail = directory[full];
        assert!(tail >= full && tail < total && !incoming[tail]);
    }
    let mut nodes = Vec::with_capacity(full);
    let mut components = Vec::new();
    let mut visited = vec![false; full];
    let mut report = Report::default();
    let mut collect = |start: usize, cycle: bool| {
        let begin = nodes.len();
        let mut at = start;
        loop {
            assert!(at < full && !visited[at]);
            visited[at] = true;
            nodes.push(at);
            let source = directory[at];
            if cycle && source == start {
                at = source;
                break;
            }
            if source >= full {
                assert!(!cycle);
                at = source;
                break;
            }
            at = source;
        }
        let len = nodes.len() - begin;
        report.largest_component = report.largest_component.max(len);
        if cycle && len == 1 {
            report.fixed += 1;
        } else {
            if cycle {
                report.cycles += 1;
            } else {
                report.paths += 1;
            }
            components.push(Component {
                begin,
                len,
                endpoint: at,
                cycle,
            });
        }
    };
    for (start, &is_live_source) in incoming.iter().take(full).enumerate() {
        if !is_live_source {
            collect(start, false);
        }
    }
    // End the closure borrow before checking visited for remaining components.
    drop(collect);
    for start in 0..full {
        if visited[start] {
            continue;
        }
        let begin = nodes.len();
        let mut at = start;
        loop {
            assert!(at < full && !visited[at]);
            visited[at] = true;
            nodes.push(at);
            at = directory[at];
            if at == start {
                break;
            }
        }
        let len = nodes.len() - begin;
        report.largest_component = report.largest_component.max(len);
        if len == 1 {
            report.fixed += 1;
        } else {
            report.cycles += 1;
            components.push(Component {
                begin,
                len,
                endpoint: start,
                cycle: true,
            });
        }
    }
    assert_eq!(nodes.len(), full);
    // Whole components can be badly unbalanced on adversarial chains. Preserve
    // the validated bounded-snapshot control as an explicit pre-movement route.
    let threshold = full.div_ceil(workers).max(1).checked_mul(2).unwrap();
    if report.largest_component > threshold {
        drop(nodes);
        drop(components);
        drop(visited);
        drop(incoming);
        report.used_split_cycle_fallback = true;
        report.temporary_records = workers
            .checked_mul(b)
            .and_then(|x| x.checked_mul(9))
            .unwrap();
        report.copy_records =
            super::restore::parallel(memory, directory, total, n, workers, parallel);
        return report;
    }
    components.sort_unstable_by_key(|c| std::cmp::Reverse(c.len + usize::from(c.cycle)));
    let mut loads = vec![0usize; workers];
    let mut counts = vec![0usize; workers];
    let mut assignment = Vec::with_capacity(components.len());
    for component in &components {
        let owner = (0..workers).min_by_key(|&i| loads[i]).unwrap();
        loads[owner] = loads[owner]
            .checked_add(component.len + usize::from(component.cycle))
            .unwrap();
        counts[owner] += 1;
        assignment.push(owner);
    }
    report.max_job_copies = loads.iter().copied().max().unwrap_or(0);
    report.copy_records = loads
        .iter()
        .try_fold(0usize, |sum, &n| sum.checked_add(n))
        .unwrap()
        .checked_mul(b)
        .and_then(|x| x.checked_add(n % b))
        .unwrap();
    report.temporary_records = workers.checked_mul(b).unwrap();
    let mut jobs: Vec<_> = counts
        .iter()
        .map(|&count| Job {
            components: Vec::with_capacity(count),
            temp: cells::<T>(b),
        })
        .collect();
    for (component, &owner) in assignment.iter().enumerate() {
        jobs[owner].components.push(component);
    }
    // All allocation, bounds, injectivity and arithmetic checks precede copies.
    // Separate paths/cycles have no common target OR source, including scratch.
    let run = |job: &mut Job<T>| {
        for &index in &job.components {
            let component = &components[index];
            let chain = &nodes[component.begin..component.begin + component.len];
            if component.cycle {
                // SAFETY: the complete cycle belongs to this job. Its first
                // source is initialized and the private temp is disjoint.
                unsafe {
                    std::ptr::copy_nonoverlapping(memory.ptr(chain[0]), job.temp.as_mut_ptr(), b);
                }
            }
            for pair in chain.windows(2) {
                // SAFETY: a path starts at a dead destination. Each interior
                // source is read before the same job overwrites it. In cycles
                // the initial token is protected by the job's temporary. Nodes
                // are distinct and components do not share physical endpoints.
                unsafe {
                    std::ptr::copy_nonoverlapping(memory.ptr(pair[1]), memory.ptr(pair[0]), b);
                }
            }
            let source = if component.cycle {
                job.temp.as_ptr()
            } else {
                memory.ptr(component.endpoint)
            };
            // SAFETY: path endpoint is a live initialized scratch block never
            // targeted by any root job. Cycle endpoint is this job's temp. The
            // last target is distinct from that source and uniquely owned.
            unsafe {
                std::ptr::copy_nonoverlapping(source, memory.ptr(*chain.last().unwrap()), b);
            }
        }
    };
    execute_jobs(&mut jobs, parallel, run);
    let tail = n % b;
    if tail != 0 {
        // SAFETY: tail scratch was excluded from every full-block source and
        // target. Only its initialized prefix is copied after all jobs join.
        unsafe {
            std::ptr::copy_nonoverlapping(
                memory.ptr(directory[full]).cast::<T>(),
                memory.input.add(full * b),
                tail,
            );
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(
        directory: Vec<usize>,
        full: usize,
        total: usize,
        tail: usize,
        workers: usize,
    ) -> Report {
        let b = 3;
        let n = full * b + tail;
        let mut input: Vec<u64> = (0..n as u64).collect();
        let mut scratch = cells::<u64>((total - full) * b);
        for (target, &source) in directory.iter().enumerate() {
            if source >= full {
                let len = if target == full { tail } else { b };
                for j in 0..len {
                    scratch[(source - full) * b + j].write((source * b + j) as u64);
                }
            }
        }
        let expected: Vec<u64> = directory
            .iter()
            .enumerate()
            .flat_map(|(target, &source)| {
                (0..if target == full { tail } else { b }).map(move |j| (source * b + j) as u64)
            })
            .collect();
        let memory = Memory {
            input: input.as_mut_ptr(),
            scratch: scratch.as_mut_ptr(),
            full,
            block: b,
        };
        let report = test_pool(workers, || {
            restore(&memory, &directory, total, n, workers, true)
        });
        assert_eq!(input, expected);
        report
    }
    #[test]
    fn direct_paths_cycles_fixed_scratch_and_tail() {
        for workers in [1, 3] {
            assert_eq!(fixture(vec![0, 1, 2], 3, 3, 0, workers).copy_records, 0);
            let all_scratch = fixture(vec![3, 4, 5], 3, 6, 0, workers);
            assert_eq!(all_scratch.copy_records, 9);
            assert_eq!(all_scratch.paths, 3);
            let mixed = fixture(vec![1, 0, 7, 4, 8, 5, 9], 6, 10, 2, workers);
            assert_eq!(mixed.copy_records, 20);
            assert_eq!(mixed.paths, 2);
            assert_eq!(mixed.cycles, 1);
            assert!(!mixed.used_split_cycle_fallback);
        }
    }
    #[test]
    fn adversarial_long_components_take_explicit_fallback() {
        let direct_path = fixture((1..=23).collect(), 23, 24, 0, 1);
        assert!(!direct_path.used_split_cycle_fallback);
        assert_eq!(direct_path.copy_records, 69);
        let mut cycle_direct: Vec<_> = (1..23).collect();
        cycle_direct.push(0);
        assert_eq!(fixture(cycle_direct, 23, 24, 0, 1).copy_records, 72);
        let long_path: Vec<_> = (1..=23).collect();
        let path = fixture(long_path, 23, 24, 0, 4);
        assert!(path.used_split_cycle_fallback);
        assert_eq!(path.largest_component, 23);
        let mut long_cycle: Vec<_> = (1..23).collect();
        long_cycle.push(0);
        let cycle = fixture(long_cycle, 23, 24, 0, 4);
        assert!(cycle.used_split_cycle_fallback);
    }
    #[test]
    fn duplicate_and_tail_alias_rejected_before_copy() {
        for directory in [vec![1, 1, 4], vec![1, 3, 3]] {
            let mut input = vec![0u64, 1, 2, 3, 4];
            let before = input.clone();
            let mut scratch = cells::<u64>(6);
            let memory = Memory {
                input: input.as_mut_ptr(),
                scratch: scratch.as_mut_ptr(),
                full: 2,
                block: 2,
            };
            assert!(
                std::panic::catch_unwind(|| restore(&memory, &directory, 5, 5, 2, false)).is_err()
            );
            assert_eq!(input, before);
        }
    }
}
