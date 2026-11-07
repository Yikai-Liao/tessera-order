// SPDX-License-Identifier: BSD-2-Clause
//! Exact-final-rank exchanges, grouped by monotonically retired bucket prefixes.
use super::*;

/// Actual measured completion times; these are not an idealized speed model.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub copy_records: usize,
    pub waves: usize,
    pub exchanges: usize,
    pub direct_moves: usize,
    pub max_batch: usize,
    pub temporary_records: usize,
    pub bucket_ready_ns: Vec<u128>,
}
struct Exchange {
    target: usize,
    source: usize,
    displaced: bool,
}
struct Job<T> {
    exchanges: Vec<Exchange>,
    temp: Vec<MaybeUninit<T>>,
}

pub(super) fn restore<T: Record>(
    memory: &Memory<T>,
    directory: &mut [usize],
    total: usize,
    n: usize,
    workers: usize,
    parallel: bool,
    ends: &[usize],
    capture_ready: bool,
) -> Report {
    let b = memory.block;
    assert_eq!(directory.len(), n.div_ceil(b));
    assert_eq!(ends.last().copied(), Some(n));
    assert!(ends.windows(2).all(|w| w[0] <= w[1]));
    let workers = workers.max(1).min(total.max(1));
    let mut inverse = vec![usize::MAX; total];
    for (target, &source) in directory.iter().enumerate() {
        assert!(source < total);
        assert_eq!(inverse[source], usize::MAX);
        inverse[source] = target;
    }
    if n % b != 0 {
        assert!(directory[memory.full] >= memory.full);
    }
    // All capacities are allocated before any movement. Each wave schedules at
    // most F targets, round-robin across P jobs, and never grows these vectors.
    let capacity = memory.full.div_ceil(workers);
    let mut jobs: Vec<_> = (0..workers)
        .map(|_| Job {
            exchanges: Vec::with_capacity(capacity),
            temp: cells::<T>(b),
        })
        .collect();
    let mut touched = vec![0usize; total];
    let mut report = Report {
        temporary_records: workers.checked_mul(b).unwrap(),
        bucket_ready_ns: Vec::with_capacity(if capture_ready { ends.len() } else { 0 }),
        ..Report::default()
    };
    // At most F successful waves plus one terminating scan per bucket, checked before
    // raw movement. Copy counters are likewise bounded before execution.
    memory
        .full
        .checked_add(ends.len())
        .and_then(|x| x.checked_add(1))
        .expect("generation overflow");
    memory
        .full
        .checked_mul(b)
        .and_then(|x| x.checked_mul(3))
        .expect("copy count overflow");
    let start = capture_ready.then(Instant::now);
    let mut prefix = 0;
    let mut generation = 0;
    let mut tail_copied = false;
    for &end in ends {
        let limit = end.div_ceil(b).min(memory.full);
        loop {
            for job in &mut jobs {
                job.exchanges.clear();
            }
            generation += 1;
            let mut count = 0;
            for target in prefix..limit {
                let source = directory[target];
                if source == target
                    || touched[target] == generation
                    || touched[source] == generation
                {
                    continue;
                }
                let displaced = inverse[target];
                touched[target] = generation;
                touched[source] = generation;
                // The exact block token, not merely a bucket label, is placed.
                // Updating this ledger is safe before execution because all
                // endpoints chosen in the wave are disjoint.
                if displaced != usize::MAX {
                    directory[displaced] = source;
                    inverse[source] = displaced;
                } else {
                    inverse[source] = usize::MAX;
                    report.direct_moves += 1;
                }
                directory[target] = target;
                inverse[target] = target;
                jobs[count % workers].exchanges.push(Exchange {
                    target,
                    source,
                    displaced: displaced != usize::MAX,
                });
                report.copy_records += if displaced == usize::MAX { b } else { 3 * b };
                count += 1;
            }
            if count == 0 {
                break;
            }
            report.waves += 1;
            report.exchanges += count;
            report.max_batch = report.max_batch.max(count);
            let run = |job: &mut Job<T>| {
                for exchange in &job.exchanges {
                    // SAFETY: each endpoint is in range, different, and has one
                    // owner in this wave. Its source is the exact live full
                    // logical block. A displaced live block is saved before
                    // overwrite. Temp storage is private to the job. Every
                    // pointer derives from the unchanged allocation ancestor.
                    unsafe {
                        if exchange.displaced {
                            std::ptr::copy_nonoverlapping(
                                memory.ptr(exchange.target),
                                job.temp.as_mut_ptr(),
                                b,
                            );
                        }
                        std::ptr::copy_nonoverlapping(
                            memory.ptr(exchange.source),
                            memory.ptr(exchange.target),
                            b,
                        );
                        if exchange.displaced {
                            std::ptr::copy_nonoverlapping(
                                job.temp.as_ptr(),
                                memory.ptr(exchange.source),
                                b,
                            );
                        }
                    }
                }
            };
            execute_jobs(&mut jobs, parallel, run);
        }
        // All targets below limit now own their exact final block. Their inverse
        // entries point to themselves, so no later target can read or write any
        // of these blocks. There are no snapshots or deferred endpoint reads.
        debug_assert!((prefix..limit).all(|i| directory[i] == i));
        prefix = limit;
        if end == n && n % b != 0 && !tail_copied {
            let tail = n % b;
            // SAFETY: this reserved tail source is never a full-block target and
            // cannot be displaced. Only its initialized records are copied.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    memory.ptr(directory[memory.full]).cast::<T>(),
                    memory.input.add(memory.full * b),
                    tail,
                );
            }
            report.copy_records += tail;
            tail_copied = true;
        }
        if let Some(start) = start {
            report.bucket_ready_ns.push(start.elapsed().as_nanos());
        }
    }
    report
}
