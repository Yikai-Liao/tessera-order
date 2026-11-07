// SPDX-License-Identifier: BSD-2-Clause

//! Parallel ordering and ownership contracts in explicitly sized Rayon pools.
//! Rayon execution is excluded under Miri; the sequential recovery suite tests
//! the underlying record movement there, not the parallel scheduler.

#![cfg(all(feature = "rayon", not(miri)))]

use std::cell::Cell;
use std::cmp::Reverse;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tessera_sort::{par_stable_sort, par_stable_sort_by_key};

fn pool(threads: usize) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap()
}

fn random(id: usize) -> u64 {
    let mut x = (id as u64).wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

#[test]
fn integer_keys_in_each_pool() {
    for threads in [1, 2, 4] {
        pool(threads).install(|| {
            macro_rules! check {
                ($($ty:ty),*) => {$(
                    // Cross the 1 MiB parallel gate for every multi-byte key.
                    // Single-byte keys intentionally use the one-digit route.
                    let bytes = std::mem::size_of::<$ty>();
                    let len = if bytes == 1 { 65537 } else { (1024 * 1024 / bytes + 1).max(65537) };
                    let mut values: Vec<$ty> = (0..len).map(|id| random(id) as $ty).collect();
                    values.extend([<$ty>::MIN, 0, <$ty>::MAX]);
                    let mut expected = values.clone();
                    expected.sort();
                    par_stable_sort(&mut values);
                    assert_eq!(values, expected);
                    let mut values: Vec<_> = values.into_iter().map(Reverse).collect();
                    par_stable_sort(&mut values);
                    expected.reverse();
                    assert_eq!(values.into_iter().map(|x| x.0).collect::<Vec<_>>(), expected);
                )*};
            }
            check!(
                u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
            );
        });
    }
}

#[test]
fn stable_records_at_routing_and_grain_boundaries() {
    let grain = 256 * 1024 / std::mem::size_of::<(u128, usize)>();
    let mut lengths = vec![0, 1, 2, 47, 48, 49, 4095, 4096, 4097, 16383, 16384, 16385];
    for tasks in [1, 2, 4] {
        lengths.extend([tasks * grain - 1, tasks * grain, tasks * grain + 1]);
    }
    lengths.sort_unstable();
    lengths.dedup();
    for threads in [1, 2, 4] {
        pool(threads).install(|| {
            for &len in &lengths {
                for shape in 0..9 {
                    let mut values: Vec<_> = (0..len)
                        .map(|id| {
                            let r = random(id);
                            let key = match shape {
                                0 => u128::from(r),
                                1 => u128::from(r) << 64,
                                2 => (u128::from(r) << 64) | u128::from(random(id + len)),
                                3 => u128::from(r % 16),
                                4 => (id / 3) as u128,
                                5 => ((len - id) / 3) as u128,
                                6 => 9,
                                7 => {
                                    if r % 100 < 98 {
                                        0
                                    } else {
                                        u128::from(r)
                                    }
                                }
                                _ => u128::from(r % 8192) * 0x01010101010101010101010101,
                            };
                            (key, id)
                        })
                        .collect();
                    let mut expected = values.clone();
                    expected.sort_by_key(|x| x.0);
                    par_stable_sort_by_key(&mut values, |x| x.0);
                    assert_eq!(
                        values, expected,
                        "threads={threads} len={len} shape={shape}"
                    );
                }
            }
        });
    }
}

#[test]
fn float_bit_patterns_and_reverse_are_stable() {
    for threads in [1, 2, 4] {
        pool(threads).install(|| {
            macro_rules! check {
                ($f:ty, $u:ty, $special:expr) => {{
                    let special: &[$u] = &$special;
                    let mut rows: Vec<_> = (0..65537)
                        .map(|id| {
                            let bits = if id % 2 == 0 {
                                special[id % special.len()]
                            } else {
                                random(id) as $u
                            };
                            (<$f>::from_bits(bits), id)
                        })
                        .collect();
                    let mut expected = rows.clone();
                    expected.sort_by(|a, b| a.0.total_cmp(&b.0));
                    par_stable_sort_by_key(&mut rows, |x| x.0);
                    let bits = |rows: &[($f, usize)]| {
                        rows.iter()
                            .map(|x| (x.0.to_bits(), x.1))
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(bits(&rows), bits(&expected));
                    expected.sort_by(|a, b| b.0.total_cmp(&a.0));
                    par_stable_sort_by_key(&mut rows, |x| Reverse(x.0));
                    assert_eq!(bits(&rows), bits(&expected));
                    // Use fresh unordered bits rather than the already sorted
                    // rows, and exceed the gate for the direct key layout too.
                    let len = 1024 * 1024 / std::mem::size_of::<$f>() + 1;
                    let mut direct: Vec<_> = (0..len)
                        .map(|id| <$f>::from_bits(random(id) as $u))
                        .collect();
                    direct.extend(special.iter().copied().map(<$f>::from_bits));
                    let mut expected = direct.clone();
                    expected.sort_by(<$f>::total_cmp);
                    par_stable_sort(&mut direct);
                    assert_eq!(
                        direct.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                        expected.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
                    );
                }};
            }
            check!(
                f32,
                u32,
                [
                    0, 1, 0x80000000, 0x80000001, 0x7f800000, 0xff800000, 0x7fc00001, 0x7f800001,
                    0xffc00001
                ]
            );
            check!(
                f64,
                u64,
                [
                    0,
                    1,
                    0x8000000000000000,
                    0x8000000000000001,
                    0x7ff0000000000000,
                    0xfff0000000000000,
                    0x7ff8000000000001,
                    0x7ff0000000000001,
                    0xfff8000000000001
                ]
            );
        });
    }
}

// Cell deliberately makes this Send but not Sync. Its edits must survive both
// successful sorting and unwinding, without requiring shared access to records.
struct Owned {
    id: usize,
    text: String,
    edited: Cell<bool>,
    drops: Arc<Vec<AtomicUsize>>,
}

impl Drop for Owned {
    fn drop(&mut self) {
        self.drops[self.id].fetch_add(1, Ordering::Relaxed);
    }
}

fn owned(len: usize) -> (Vec<Owned>, Arc<Vec<AtomicUsize>>) {
    let drops = Arc::new((0..len).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
    let values = (0..len)
        .map(|id| Owned {
            id,
            text: format!("record {id}"),
            edited: Cell::new(false),
            drops: Arc::clone(&drops),
        })
        .collect();
    (values, drops)
}

fn verify_owned(mut values: Vec<Owned>, drops: &Arc<Vec<AtomicUsize>>) {
    assert!(drops.iter().all(|x| x.load(Ordering::Relaxed) == 0));
    values.sort_by_key(|x| x.id);
    for (id, value) in values.iter().enumerate() {
        assert_eq!(value.id, id);
        assert_eq!(value.text, format!("record {id}"));
    }
    // A subsequent parallel operation must also be safe after recovery.
    par_stable_sort_by_key(&mut values, |x| Reverse(x.id));
    assert!(values.windows(2).all(|x| x[0].id > x[1].id));
    drop(values);
    assert!(drops.iter().all(|x| x.load(Ordering::Relaxed) == 1));
}

#[test]
fn send_only_owned_records_preserve_stability_and_edits() {
    for threads in [1, 2, 4] {
        pool(threads).install(|| {
            let (mut values, drops) = owned(32769);
            let mut expected: Vec<_> = values.iter().map(|x| x.id).collect();
            expected.sort_by_key(|&id| random(id) % 8192);
            par_stable_sort_by_key(&mut values, |x| {
                x.edited.set(true);
                random(x.id) % 8192
            });
            assert_eq!(values.iter().map(|x| x.id).collect::<Vec<_>>(), expected);
            assert!(values.iter().all(|x| x.edited.get()));
            verify_owned(values, &drops);
        });
    }
}

#[test]
fn panic_payload_and_owned_records_survive_parent_and_worker_panics() {
    for threads in [1, 2, 4] {
        pool(threads).install(|| {
            for at in [1, 48, 8192] {
                let (mut values, drops) = owned(32769);
                let calls = AtomicUsize::new(0);
                let result = catch_unwind(AssertUnwindSafe(|| {
                    par_stable_sort_by_key(&mut values, |x| {
                        x.edited.set(true);
                        if calls.fetch_add(1, Ordering::Relaxed) + 1 == at {
                            std::panic::panic_any(0x71u32);
                        }
                        random(x.id)
                    });
                }));
                assert_eq!(*result.unwrap_err().downcast::<u32>().unwrap(), 0x71);
                assert!(values.iter().any(|x| x.edited.get()));
                verify_owned(values, &drops);
            }
            if threads > 1 {
                let origin = std::thread::current().id();
                let worker_panicked = AtomicBool::new(false);
                let (mut values, drops) = owned(131073);
                let result = catch_unwind(AssertUnwindSafe(|| {
                    par_stable_sort_by_key(&mut values, |x| {
                        x.edited.set(true);
                        // Only child completion runs outside the installing
                        // worker, after the parent partition is materialized.
                        if std::thread::current().id() != origin {
                            worker_panicked.store(true, Ordering::Relaxed);
                            std::panic::panic_any(0x72u32);
                        }
                        random(x.id)
                    });
                }));
                assert!(
                    worker_panicked.load(Ordering::Relaxed),
                    "no worker child was exercised"
                );
                assert_eq!(*result.unwrap_err().downcast::<u32>().unwrap(), 0x72);
                verify_owned(values, &drops);
            }
        });
    }
}

#[test]
fn inconsistent_keys_preserve_owned_records() {
    for threads in [1, 2, 4] {
        pool(threads).install(|| {
            for at in [1, 8192, 100000] {
                let (mut values, drops) = owned(32769);
                let calls = AtomicUsize::new(0);
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    par_stable_sort_by_key(&mut values, |x| {
                        let call = calls.fetch_add(1, Ordering::Relaxed);
                        if call < at {
                            random(x.id)
                        } else {
                            random(call)
                        }
                    });
                }));
                verify_owned(values, &drops);
            }
        });
    }
}

#[test]
fn exclusive_borrows_survive_sort_changing_keys_and_panics() {
    struct Borrowed<'a> {
        id: usize,
        value: &'a mut usize,
    }
    for threads in [1, 2, 4] {
        pool(threads).install(|| {
            for mode in 0..3 {
                let len = 65537;
                let mut backing: Vec<_> = (0..len).collect();
                let mut values: Vec<_> = backing
                    .iter_mut()
                    .enumerate()
                    .map(|(id, value)| Borrowed { id, value })
                    .collect();
                let calls = AtomicUsize::new(0);
                let result = catch_unwind(AssertUnwindSafe(|| {
                    par_stable_sort_by_key(&mut values, |x| {
                        let call = calls.fetch_add(1, Ordering::Relaxed);
                        assert!(!(mode == 2 && call == 8192), "borrowed key panic");
                        if mode == 1 && call >= 8192 {
                            random(call)
                        } else {
                            random(x.id) % 8192
                        }
                    });
                }));
                if mode == 0 {
                    assert!(result.is_ok());
                    let mut expected: Vec<_> = (0..len).collect();
                    expected.sort_by_key(|&id| random(id) % 8192);
                    assert_eq!(values.iter().map(|x| x.id).collect::<Vec<_>>(), expected);
                } else if mode == 2 {
                    assert!(result.is_err());
                }
                let mut ids: Vec<_> = values.iter().map(|x| x.id).collect();
                ids.sort_unstable();
                assert_eq!(ids, (0..len).collect::<Vec<_>>());
                for x in &mut values {
                    assert_eq!(*x.value, x.id);
                    *x.value += len;
                }
                drop(values);
                assert_eq!(backing, (len..2 * len).collect::<Vec<_>>());
            }
        });
    }
}

#[test]
fn zero_sized_and_overaligned_records() {
    #[repr(align(128))]
    struct Wide {
        key: u64,
        id: usize,
        payload: [u8; 257],
    }
    for threads in [1, 2, 4] {
        pool(threads).install(|| {
            let mut zero_sized = [(); 65537];
            par_stable_sort_by_key(&mut zero_sized, |_| 0u8);
            let mut values: Vec<_> = (0..4097)
                .map(|id| Wide {
                    key: random(id) % 1024,
                    id,
                    payload: [id as u8; 257],
                })
                .collect();
            let mut expected: Vec<_> = values.iter().map(|x| (x.key, x.id)).collect();
            expected.sort_by_key(|x| x.0);
            par_stable_sort_by_key(&mut values, |x| x.key);
            assert_eq!(
                values.iter().map(|x| (x.key, x.id)).collect::<Vec<_>>(),
                expected
            );
            assert!(values.iter().all(|x| x.payload == [x.id as u8; 257]));
        });
    }
}

#[test]
fn key_closure_needs_sync_but_not_send_or_clone() {
    pool(4).install(|| {
        let salt = std::sync::Mutex::new(0x1234u64);
        let guard = salt.lock().unwrap();
        let mut values: Vec<_> = (0..65537).map(|id| (random(id), id)).collect();
        let mut expected = values.clone();
        expected.sort_by_key(|x| x.0 ^ *guard);
        // Moving a MutexGuard into the closure makes the closure neither Send
        // nor Clone, while it remains Sync and can be shared by worker tasks.
        par_stable_sort_by_key(&mut values, move |x| x.0 ^ *guard);
        assert_eq!(values, expected);
    });
}

struct MovedWorkerPanic<'a> {
    pool: &'a rayon::ThreadPool,
    origin: std::thread::ThreadId,
    start: usize,
    end: usize,
    worker_started: AtomicBool,
    moved_calls: AtomicUsize,
    at: usize,
}

impl<'a> MovedWorkerPanic<'a> {
    fn new<T>(values: &[T], at: usize, pool: &'a rayon::ThreadPool) -> Self {
        let start = values.as_ptr() as usize;
        Self {
            pool,
            origin: std::thread::current().id(),
            start,
            end: start + std::mem::size_of_val(values),
            worker_started: AtomicBool::new(false),
            moved_calls: AtomicUsize::new(0),
            at,
        }
    }

    fn check<T>(&self, value: &T) {
        if std::thread::current().id() == self.origin {
            // The parent partition has no queued Rayon jobs. Only wait once a
            // child job is available to steal, so this cannot block the serial
            // preparation that must finish before workers can begin. Waiting
            // here prevents a fast origin from consuming every child itself.
            if !self.worker_started.load(Ordering::Relaxed)
                && self.pool.current_thread_has_pending_tasks() == Some(true)
            {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while !self.worker_started.load(Ordering::Relaxed) {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "worker did not start queued child"
                    );
                    std::thread::yield_now();
                }
            }
            return;
        }
        self.worker_started.store(true, Ordering::Relaxed);
        let address = std::ptr::from_ref(value) as usize;
        if !(self.start..self.end).contains(&address) {
            // A variation scan only borrows the caller's allocation. Requiring
            // an external address proves a worker has moved a live record into
            // scratch or an insertion temporary before the injected panic.
            let call = self.moved_calls.fetch_add(1, Ordering::Relaxed) + 1;
            if call == self.at {
                std::panic::panic_any(0x73u32);
            }
        }
    }

    fn verify(&self, result: std::thread::Result<()>) {
        assert!(self.moved_calls.load(Ordering::Relaxed) >= self.at);
        assert_eq!(*result.unwrap_err().downcast::<u32>().unwrap(), 0x73);
    }
}

fn movement_key(id: usize, block_children: bool) -> u64 {
    if block_children {
        // Four balanced high-prefix buckets remain large even after the root
        // widens its digit. Each child exceeds the 64 KiB tile budget for both
        // Owned and Borrowed, forcing a worker block partition.
        random(id) & 0xc000_0000_ffff_ffff
    } else {
        // Uniform root buckets fit a tile; their remaining bits require more
        // than one counting pass, so later extraction reads moved scratch.
        random(id) & 0x00ff_ffff
    }
}

#[test]
fn delayed_worker_movement_panics_preserve_owned_records() {
    for threads in [2, 4] {
        let pool = pool(threads);
        pool.install(|| {
            for block_children in [false, true] {
                for at in [1, 7, 17] {
                    let (mut values, drops) = owned(131072);
                    let probe = MovedWorkerPanic::new(&values, at, &pool);
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        par_stable_sort_by_key(&mut values, |x| {
                            x.edited.set(true);
                            probe.check(x);
                            movement_key(x.id, block_children)
                        });
                    }));
                    probe.verify(result);
                    assert!(values.iter().any(|x| x.edited.get()));
                    verify_owned(values, &drops);
                }
            }
        });
    }
}

#[test]
fn delayed_worker_movement_panics_preserve_exclusive_borrows() {
    struct Borrowed<'a> {
        id: usize,
        value: &'a mut usize,
    }
    for threads in [2, 4] {
        let pool = pool(threads);
        pool.install(|| {
            for block_children in [false, true] {
                for at in [1, 7, 17] {
                    let len = 131072;
                    let mut backing: Vec<_> = (0..len).collect();
                    let mut values: Vec<_> = backing
                        .iter_mut()
                        .enumerate()
                        .map(|(id, value)| Borrowed { id, value })
                        .collect();
                    let probe = MovedWorkerPanic::new(&values, at, &pool);
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        par_stable_sort_by_key(&mut values, |x| {
                            probe.check(x);
                            movement_key(x.id, block_children)
                        });
                    }));
                    probe.verify(result);
                    values.sort_by_key(|x| x.id);
                    for (id, x) in values.iter_mut().enumerate() {
                        assert_eq!(x.id, id);
                        assert_eq!(*x.value, id);
                        *x.value += len;
                    }
                    // Reuse every exclusive reference in a second parallel
                    // sort before releasing it and checking the backing data.
                    par_stable_sort_by_key(&mut values, |x| Reverse(x.id));
                    assert!(values.windows(2).all(|x| x[0].id > x[1].id));
                    drop(values);
                    assert_eq!(backing, (len..2 * len).collect::<Vec<_>>());
                }
            }
        });
    }
}

#[test]
fn successful_large_sorts_use_multiple_workers() {
    for threads in [2, 4] {
        let pool = pool(threads);
        pool.install(|| {
            macro_rules! check {
                ($ty:ty, $make:expr, $compare:expr) => {{
                    let len = 1024 * 1024 / std::mem::size_of::<$ty>() + 1;
                    let mut values: Vec<$ty> = (0..len).map($make).collect();
                    let mut expected = values.clone();
                    expected.sort_by($compare);
                    let probe = MovedWorkerPanic::new(&values, usize::MAX, &pool);
                    let visited = AtomicUsize::new(0);
                    par_stable_sort_by_key(&mut values, |x| {
                        probe.check(x);
                        visited.fetch_or(
                            1 << rayon::current_thread_index().unwrap(),
                            Ordering::Relaxed,
                        );
                        *x
                    });
                    assert!(probe.worker_started.load(Ordering::Relaxed));
                    assert!(visited.load(Ordering::Relaxed).count_ones() >= 2);
                    assert!(
                        values
                            .iter()
                            .zip(&expected)
                            .all(|(a, b)| ($compare)(a, b).is_eq())
                    );
                }};
            }
            check!(u16, |id| random(id) as u16, u16::cmp);
            check!(u32, |id| random(id) as u32, u32::cmp);
            check!(u64, random, u64::cmp);
            check!(f32, |id| f32::from_bits(random(id) as u32), f32::total_cmp);
            check!(f64, |id| f64::from_bits(random(id)), f64::total_cmp);
        });
    }
}
