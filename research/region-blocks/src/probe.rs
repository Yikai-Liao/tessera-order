// SPDX-License-Identifier: BSD-2-Clause
//! Read-only diagnostics for the exact stable-rank whole-block directory.
//! All allocations here are O(n/B + PR) words. They are separate from movement.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub full_blocks: usize,
    pub physical_blocks: usize,
    pub fixed_blocks: usize,
    pub reciprocal_pairs: usize,
    pub paths: usize,
    pub cycles: usize,
    pub largest_component: usize,
    pub component_sizes: Vec<usize>,
    pub independent_exchange_wave_sizes: Vec<usize>,
    pub split_snapshots: usize,
    pub job_steps: Vec<usize>,
    pub bucket_ready_steps: Vec<usize>,
    pub bucket_records: Vec<usize>,
    pub max_job_steps: usize,
    pub source_target_intervals: Vec<(usize, usize, usize)>,
}
/// Models the existing aligned split-cycle scheduler with equal-speed jobs.
/// Snapshot creation is a prerequisite, not part of this ideal movement clock.
/// Steps count whole-block copies, including short-cycle temporary copies. The
/// partial tail is conservatively charged one block-copy step after the join.
/// Job assignment follows the control's node-count load, not this copy clock.
/// Each target is retired only after its final write and all root reads finish.
pub fn analyze(
    directory: &[usize],
    total: usize,
    n: usize,
    b: usize,
    workers: usize,
    ends: &[usize],
) -> Report {
    assert!(b > 0);
    let full = n / b;
    assert_eq!(directory.len(), n.div_ceil(b));
    let mut report = Report {
        full_blocks: full,
        physical_blocks: total,
        ..Report::default()
    };
    let mut incoming = vec![false; total];
    for (target, &source) in directory.iter().take(full).enumerate() {
        assert!(source < total && !incoming[source]);
        incoming[source] = true;
        report.fixed_blocks += usize::from(source == target);
        if source > target && source < full && directory[source] == target {
            report.reciprocal_pairs += 1;
        }
        report
            .source_target_intervals
            .push((source * b, target * b, b));
    }
    if n % b != 0 {
        report
            .source_target_intervals
            .push((directory[full] * b, full * b, n % b));
    }
    let mut visited = vec![false; total];
    // Open paths begin at destinations that contain no remaining live source.
    for start in 0..full {
        if incoming[start] {
            continue;
        }
        let mut at = start;
        let mut len = 0;
        while at < full && !visited[at] {
            visited[at] = true;
            len += 1;
            at = directory[at];
        }
        assert!(at >= full);
        report.paths += 1;
        report.component_sizes.push(len);
    }
    for start in 0..full {
        if visited[start] {
            continue;
        }
        let mut at = start;
        let mut len = 0;
        while !visited[at] {
            visited[at] = true;
            len += 1;
            at = directory[at];
        }
        assert_eq!(at, start);
        report.cycles += 1;
        report.component_sizes.push(len);
    }
    report.largest_component = report.component_sizes.iter().copied().max().unwrap_or(0);
    // A maximum-degree-two dependency graph admits a greedy disjoint exchange
    // wave. This is coverage only; no arbitrary bucket-level pair is accepted.
    let mut touched = vec![false; total];
    let mut selected = 0;
    for (target, &source) in directory.iter().take(full).enumerate() {
        if source != target && !touched[source] && !touched[target] {
            touched[source] = true;
            touched[target] = true;
            selected += 1;
        }
    }
    report.independent_exchange_wave_sizes.push(selected);
    let mut sources = vec![usize::MAX; total];
    for (target, &source) in directory.iter().take(full).enumerate() {
        sources[target] = source;
    }
    if n % b != 0 {
        let tail = directory[full];
        assert!(tail >= full && !incoming[tail]);
        sources[tail] = tail;
        incoming[tail] = true;
    }
    let mut unused = incoming
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
    report.job_steps = vec![0; workers];
    let mut ready = vec![0; full];
    let mut job_load = vec![0; workers];
    visited.fill(false);
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
        let chunk_size = if nodes.len() <= cap { nodes.len() } else { cap };
        if nodes.len() > cap {
            report.split_snapshots += nodes.len().div_ceil(cap);
        }
        for chunk in nodes.chunks(chunk_size) {
            let job = (0..workers).min_by_key(|&j| job_load[j]).unwrap();
            if nodes.len() <= cap {
                report.job_steps[job] += 1;
            }
            for (i, &target) in chunk.iter().enumerate() {
                if target < full {
                    ready[target] = report.job_steps[job] + i + 1;
                }
            }
            report.job_steps[job] += chunk.len();
            job_load[job] += chunk.len();
        }
    }
    report.max_job_steps =
        report.job_steps.iter().copied().max().unwrap_or(0) + usize::from(n % b != 0);
    let mut previous = 0;
    for &end in ends {
        assert!(end >= previous && end <= n);
        let step = if end > previous {
            ready[previous / b..end.div_ceil(b).min(full)]
                .iter()
                .copied()
                .max()
                .unwrap_or(0)
                .max(if end > full * b {
                    report.max_job_steps
                } else {
                    0
                })
        } else {
            0
        };
        report.bucket_ready_steps.push(step);
        report.bucket_records.push(end - previous);
        previous = end;
    }
    report
}
