// SPDX-License-Identifier: BSD-2-Clause
//! Focused source gates; no test in this file has been executed at source freeze.
use super::*;
use std::sync::Arc;

struct Fixture {
    input: Vec<u64>,
    scratch: Vec<MaybeUninit<u64>>,
    expected: Vec<u64>,
    full: usize,
    block: usize,
}
impl Fixture {
    fn new(directory: &[usize], full: usize, total: usize, block: usize, tail: usize) -> Self {
        let input = (0..full * block + tail).map(|x| x as u64).collect();
        let mut scratch = cells((total - full) * block);
        for (offset, cell) in scratch.iter_mut().enumerate() {
            cell.write((full * block + offset) as u64);
        }
        let expected = directory
            .iter()
            .enumerate()
            .flat_map(|(target, &source)| {
                (0..if target == full { tail } else { block })
                    .map(move |offset| (source * block + offset) as u64)
            })
            .collect();
        Self {
            input,
            scratch,
            expected,
            full,
            block,
        }
    }
    fn memory(&mut self) -> Memory<u64> {
        Memory {
            input: self.input.as_mut_ptr(),
            scratch: self.scratch.as_mut_ptr(),
            full: self.full,
            block: self.block,
        }
    }
}

#[test]
fn exact_target_shapes() {
    for (directory, full, total, tail) in [
        (vec![0, 1, 2], 3, 3, 0),
        (vec![3, 4, 5], 3, 6, 0),
        (vec![1, 2, 3], 3, 4, 0),
        (vec![1, 2, 0], 3, 3, 0),
        (vec![1, 0, 3, 2, 5, 4], 6, 6, 0),
        (vec![1, 3, 0, 4], 3, 5, 2),
        (vec![0], 0, 1, 2),
    ] {
        for workers in [1, 2, 3, 8] {
            for parallel in [false, true] {
                test_pool(workers, || {
                    let mut fixture = Fixture::new(&directory, full, total, 3, tail);
                    let n = fixture.input.len();
                    let ends = [n];
                    let mut state = State::new(fixture.memory(), total, n, workers, &ends, None);
                    state.seed(&directory);
                    let report = state.execute(parallel, None, 0);
                    assert_eq!(fixture.input, fixture.expected);
                    assert_eq!(
                        report.identity_blocks + report.one_copy_blocks + report.three_copy_blocks,
                        full
                    );
                    assert_eq!(
                        report.copy_records,
                        (report.one_copy_blocks + 3 * report.three_copy_blocks) * 3 + tail
                    );
                    assert!(
                        state
                            .shared
                            .active
                            .iter()
                            .all(|n| n.load(Ordering::Relaxed) == 0)
                    );
                });
            }
        }
    }
}

#[test]
fn stale_retired_source() {
    let mut fixture = Fixture::new(&[1, 2, 0], 3, 3, 1, 0);
    let ends = [1, 2, 3];
    let mut state = State::new(fixture.memory(), 3, 3, 3, &ends, Some(Policy::RetireInline));
    state.seed(&[1, 2, 0]);
    let stale = state.shared.positions[0].load(Ordering::Acquire);
    assert_eq!(stale, 1);
    let child = |_: &mut tessera::Workspace<u64>, bucket: &mut [u64]| {
        for value in bucket {
            *value += 100;
        }
    };
    // Complete owner 1 and mutate its child after owner 0 loaded source 1.
    state.shared.run(&mut state.workers[1], &child, false);
    assert_eq!(fixture.input[1], 102);
    assert_eq!(
        state
            .shared
            .exchange(0, stale, state.workers[0].temp.as_mut_ptr()),
        None
    );
    state.execute_with(false, Some(Policy::RetireInline), 0, &child);
    assert_eq!(fixture.input, [101, 102, 100]);
}

#[test]
fn tiny_shared_tail_buckets() {
    for (block, full, total, directory, ends) in [
        (4, 1, 3, vec![2, 1], vec![0, 1, 1, 2, 3, 4, 5, 5, 6, 7]),
        (8, 0, 1, vec![0], vec![0, 1, 1, 2, 3]),
    ] {
        for workers in [1, 2, 3, 8] {
            for parallel in [false, true] {
                let n = *ends.last().unwrap();
                let mut fixture = Fixture::new(&directory, full, total, block, n % block);
                let calls = AtomicUsize::new(0);
                let mut state = State::new(
                    fixture.memory(),
                    total,
                    n,
                    workers,
                    &ends,
                    Some(Policy::RetireInline),
                );
                state.seed(&directory);
                let child = |_: &mut tessera::Workspace<u64>, bucket: &mut [u64]| {
                    assert!(!bucket.is_empty());
                    calls.fetch_add(1, Ordering::Relaxed);
                    for value in bucket {
                        *value += 1000;
                    }
                };
                test_pool(workers, || {
                    state.execute_with(parallel, Some(Policy::RetireInline), 0, &child)
                });
                assert_eq!(
                    fixture.input,
                    fixture
                        .expected
                        .iter()
                        .map(|x| x + 1000)
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    calls.load(Ordering::Relaxed),
                    ends.iter()
                        .enumerate()
                        .filter(|&(index, &end)| end > if index == 0 { 0 } else { ends[index - 1] })
                        .count()
                );
                assert!(
                    state
                        .workers
                        .iter()
                        .all(|job| job.remaining.iter().all(|&n| n == 0))
                );
            }
        }
    }
}

struct PanicPayload(Arc<AtomicBool>);
impl Drop for PanicPayload {
    fn drop(&mut self) {
        assert!(
            self.0.load(Ordering::Acquire),
            "panic payload dropped before root restoration"
        );
    }
}

#[test]
fn child_panic_restores_root() {
    for worker_also_panics in [false, true] {
        let mut fixture = Fixture::new(&[1, 2, 3, 0], 4, 4, 1, 0);
        let ends = [1, 2, 3, 4];
        let mut state = State::new(fixture.memory(), 4, 4, 1, &ends, Some(Policy::RetireInline));
        state.seed(&[1, 2, 3, 0]);
        if worker_also_panics {
            state.workers[0].panic_after = Some(2);
        }
        let calls = AtomicUsize::new(0);
        let restored = Arc::new(AtomicBool::new(false));
        let child = |_: &mut tessera::Workspace<u64>, bucket: &mut [u64]| {
            if calls.fetch_add(1, Ordering::Relaxed) == 1 {
                std::panic::panic_any(PanicPayload(Arc::clone(&restored)));
            }
            bucket[0] += 100;
        };
        let error = catch_unwind(AssertUnwindSafe(|| {
            state.execute_with(false, Some(Policy::RetireInline), 0, &child)
        }))
        .expect_err("injected child panic must propagate");
        assert_eq!(fixture.input, [101, 2, 3, 0]);
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        restored.store(true, Ordering::Release);
        drop(error);
    }
}

#[test]
fn worker_panic_completes_targets() {
    for parallel in [false, true] {
        let mut fixture = Fixture::new(&[1, 2, 3, 0, 5, 4], 6, 6, 1, 0);
        let ends = [6];
        let mut state = State::new(fixture.memory(), 6, 6, 3, &ends, None);
        state.seed(&[1, 2, 3, 0, 5, 4]);
        state.workers[0].panic_after = Some(1);
        let error = catch_unwind(AssertUnwindSafe(|| {
            test_pool(3, || state.execute(parallel, None, 0))
        }));
        assert!(error.is_err());
        assert_eq!(fixture.input, fixture.expected);
    }
}

#[test]
fn unwind_guard_finishes_directory() {
    let mut fixture = Fixture::new(&[1, 2, 3, 0], 4, 4, 1, 0);
    let ends = [4];
    let mut state = State::new(fixture.memory(), 4, 4, 2, &ends, None);
    state.seed(&[1, 2, 3, 0]);
    let error = catch_unwind(AssertUnwindSafe(|| {
        let guard = Finish {
            state: &mut state,
            armed: true,
        };
        guard
            .state
            .shared
            .run(&mut guard.state.workers[0], &u64::sort_child, true);
        panic!("injected dispatcher boundary unwind");
    }));
    assert!(error.is_err());
    assert_eq!(fixture.input, fixture.expected);
}

#[test]
fn full_sort_matches_stable_oracle() {
    let lengths: &[usize] = if cfg!(miri) {
        &[1, 7, 19]
    } else {
        &[0, 1, 7, 31, 257, 4097]
    };
    for &n in lengths {
        for workers in [1, 2, 3, 8] {
            for shape in 0..6 {
                let mut seed = 19u64;
                let input: Vec<_> = (0..n)
                    .map(|id| {
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                        let key = match shape {
                            0 => seed,
                            1 => (seed % 4) << 60,
                            2 => {
                                if seed % 100 < 98 {
                                    0
                                } else {
                                    seed
                                }
                            }
                            3 => 0,
                            4 => (id as u64) << 32,
                            _ => ((n - id) as u64) << 32,
                        };
                        Row16 { key, id: id as u64 }
                    })
                    .collect();
                let mut expected = input.clone();
                expected.sort_by_key(|row| row.key);
                for policy in [Policy::Barrier, Policy::RetireInline] {
                    for parallel in [false, true] {
                        let mut actual = input.clone();
                        test_pool(workers, || {
                            sort_async(
                                &mut actual,
                                Config {
                                    shards: workers,
                                    radix_bits: 4,
                                    shift: 60,
                                    block_size: Some(7),
                                    parallel,
                                    ..Config::default()
                                },
                                policy,
                            )
                        });
                        assert_eq!(
                            actual, expected,
                            "n={n}, P={workers}, shape={shape}, {policy:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn scoped_workers_share_kernel() {
    for workers in [1, 2, 3, 8] {
        let directory = [1, 2, 0, 5, 3, 6];
        let mut fixture = Fixture::new(&directory, 6, 7, 3, 0);
        let ends = [18];
        let mut state = State::new(fixture.memory(), 7, 18, workers, &ends, None);
        state.seed(&directory);
        let shared = &state.shared;
        std::thread::scope(|scope| {
            for job in &mut state.workers {
                scope.spawn(move || shared.run(job, &u64::sort_child, false));
            }
        });
        assert_eq!(fixture.input, fixture.expected);
        assert!(
            state
                .shared
                .active
                .iter()
                .all(|n| n.load(Ordering::Acquire) == 0)
        );
    }
}

#[test]
fn concurrent_stale_child_borrow() {
    let mut fixture = Fixture::new(&[1, 2, 0], 3, 3, 1, 0);
    let ends = [1, 2, 3];
    let mut state = State::new(fixture.memory(), 3, 3, 3, &ends, Some(Policy::RetireInline));
    state.seed(&[1, 2, 0]);
    // Dedicated scoped threads, not a barrier in a work-stealing task graph.
    // Phase 1 fixes the speculative load before worker 1 proceeds; phase 2
    // keeps worker 1's exact mutable child borrow alive during the stale attempt;
    // phase 3 proves that attempt completed before the child borrow ends.
    let rendezvous = std::sync::Barrier::new(2);
    let rejected = AtomicBool::new(false);
    let shared = &state.shared;
    let child = |_: &mut tessera::Workspace<u64>, bucket: &mut [u64]| {
        let is_second = bucket[0] == 2;
        bucket[0] += 100;
        if is_second {
            assert!(shared.active[0].load(Ordering::Acquire) > 0);
            assert!(shared.active[2].load(Ordering::Acquire) > 0);
            rendezvous.wait();
            rendezvous.wait();
            assert!(rejected.load(Ordering::Acquire));
            assert_eq!(bucket[0], 102);
        }
    };
    let (first, rest) = state.workers.split_at_mut(1);
    let (second, third) = rest.split_at_mut(1);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let stale = shared.positions[0].load(Ordering::Acquire);
            assert_eq!(stale, 1);
            rendezvous.wait();
            rendezvous.wait();
            assert_eq!(shared.exchange(0, stale, first[0].temp.as_mut_ptr()), None);
            rejected.store(true, Ordering::Release);
            rendezvous.wait();
            shared.run(&mut first[0], &child, false);
        });
        scope.spawn(|| {
            rendezvous.wait();
            shared.run(&mut second[0], &child, false);
        });
    });
    shared.run(&mut third[0], &child, false);
    assert!(rejected.load(Ordering::Acquire));
    assert_eq!(fixture.input, [101, 102, 100]);
}

#[test]
fn u64_public_child_oracle() {
    for workers in [1, 2, 3, 8] {
        for policy in [Policy::Barrier, Policy::RetireInline] {
            let mut seed = 83u64;
            let mut values: Vec<_> = (0..if cfg!(miri) { 19 } else { 4099 })
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    seed
                })
                .collect();
            let mut expected = values.clone();
            expected.sort();
            test_pool(workers, || {
                sort_async(
                    &mut values,
                    Config {
                        shards: workers,
                        block_size: Some(7),
                        parallel: true,
                        ..Config::default()
                    },
                    policy,
                )
            });
            assert_eq!(values, expected);
        }
    }
}

/// A destructor witness used only with synchronous jobs and live fixture storage.
struct CleanupPayload {
    memory: Memory<u64>,
    expected: Vec<u64>,
    drops: Arc<AtomicUsize>,
    nested_drops: Arc<AtomicUsize>,
    panic_on_drop: bool,
}
struct NestedPayload(Arc<AtomicUsize>);
impl Drop for NestedPayload {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}
impl Drop for CleanupPayload {
    fn drop(&mut self) {
        for (index, &expected) in self.expected.iter().enumerate() {
            // SAFETY: this witness is installed only in the serial test below;
            // no other thread accesses the fixture, every input record remains
            // initialized even before restoration, and its Vec is still alive.
            let actual = unsafe { self.memory.input.add(index).read() };
            assert_eq!(actual, expected, "payload Drop ran before full restoration");
        }
        self.drops.fetch_add(1, Ordering::Relaxed);
        if self.panic_on_drop {
            std::panic::panic_any(NestedPayload(Arc::clone(&self.nested_drops)));
        }
    }
}

fn cleanup_case(panic_on_drop: bool) {
    let mut fixture = Fixture::new(&[1, 2, 3, 0], 4, 4, 1, 0);
    let ends = [4];
    let memory = fixture.memory();
    let mut state = State::new(memory, 4, 4, 2, &ends, None);
    state.seed(&[1, 2, 3, 0]);
    let primary_drops = Arc::new(AtomicUsize::new(0));
    let secondary_drops = Arc::new(AtomicUsize::new(0));
    let nested_drops = Arc::new(AtomicUsize::new(0));
    let payload = |drops, panic_on_drop| {
        Box::new(CleanupPayload {
            memory,
            expected: fixture.expected.clone(),
            drops,
            nested_drops: Arc::clone(&nested_drops),
            panic_on_drop,
        })
    };
    // Inject retained payloads before any target restoration. No test hook runs
    // in a transaction, and the real execute/finish/cleanup sequence is exercised.
    state.workers[0].panic = Some(payload(Arc::clone(&primary_drops), false));
    state.workers[0].worker_panic = Some(payload(Arc::clone(&secondary_drops), panic_on_drop));
    let error = catch_unwind(AssertUnwindSafe(|| state.execute(false, None, 0)))
        .expect_err("the saved primary must resume");
    assert_eq!(fixture.input, fixture.expected);
    assert_eq!(primary_drops.load(Ordering::Relaxed), 0);
    assert_eq!(secondary_drops.load(Ordering::Relaxed), 1);
    assert_eq!(nested_drops.load(Ordering::Relaxed), 0);
    assert!(error.is::<CleanupPayload>());
    drop(error);
    assert_eq!(primary_drops.load(Ordering::Relaxed), 1);
    assert_eq!(secondary_drops.load(Ordering::Relaxed), 1);
    assert_eq!(nested_drops.load(Ordering::Relaxed), 0);
}

#[test]
fn secondary_cleanup_waits_restoration() {
    cleanup_case(false);
}

#[test]
fn secondary_drop_panic_preserves_primary() {
    cleanup_case(true);
}
