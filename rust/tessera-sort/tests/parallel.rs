// SPDX-License-Identifier: BSD-2-Clause

//! Contracts of the feature-gated Rayon entry points, including the fallback.

#![cfg(feature = "rayon")]

use core::cell::Cell;
use core::cmp::Reverse;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tessera_sort::{par_stable_sort, par_stable_sort_by_key};

#[test]
fn keys_keep_total_order() {
    let mut keys = [
        f64::from_bits(0xfff8_0000_0000_0001),
        f64::NAN,
        -0.0,
        1.0,
        f64::NEG_INFINITY,
        0.0,
        f64::INFINITY,
    ];
    let mut expected = keys;
    expected.sort_by(f64::total_cmp);
    par_stable_sort(&mut keys);
    assert_eq!(keys.map(f64::to_bits), expected.map(f64::to_bits));

    let mut wide = [Reverse(i128::MIN), Reverse(i128::MAX), Reverse(-1)];
    par_stable_sort(&mut wide);
    assert_eq!(wide, [Reverse(i128::MAX), Reverse(-1), Reverse(i128::MIN)]);
    par_stable_sort::<u8>(&mut []);
    par_stable_sort(&mut [1u8]);
}

#[test]
#[cfg(not(miri))]
fn counted_keys_keep_bits() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let len = 1usize << 18;
    pool.install(|| {
        for shift in [0, 64] {
            let mut keys: Vec<_> = (0..len)
                .map(|id| (0x1234u128 << (64 - shift)) | (((id * 7919) & 255) as u128) << shift)
                .collect();
            let mut expected = keys.clone();
            expected.sort();
            par_stable_sort(&mut keys);
            assert_eq!(keys, expected);
        }
        let mut keys: Vec<_> = (0..len)
            .map(|id| (((id * 7919) & 255) as i32 - 512) << 16)
            .collect();
        let mut expected = keys.clone();
        expected.sort();
        par_stable_sort(&mut keys);
        assert_eq!(keys, expected);

        // Distinct NaN payloads, for both signs, must survive run filling.
        for sign in [0, 1u64 << 63] {
            let mut keys: Vec<_> = (0..len)
                .map(|id| f64::from_bits(sign | 0x7ff8_0000_0000_0000 | ((id * 7919) & 255) as u64))
                .collect();
            let mut reverse: Vec<_> = keys.iter().copied().map(Reverse).collect();
            let mut reversed_expected = reverse.clone();
            reversed_expected.sort_by(|a, b| b.0.total_cmp(&a.0));
            par_stable_sort(&mut reverse);
            assert!(
                reverse
                    .iter()
                    .zip(&reversed_expected)
                    .all(|(a, b)| a.0.to_bits() == b.0.to_bits())
            );
            let mut expected = keys.clone();
            expected.sort_by(f64::total_cmp);
            par_stable_sort(&mut keys);
            assert!(
                keys.iter()
                    .zip(&expected)
                    .all(|(a, b)| a.to_bits() == b.to_bits())
            );
        }
    });
}

#[test]
fn send_only_records_sort() {
    // Cell makes the records Send but not Sync. A captured MutexGuard makes
    // the key function Sync but not Send; only a shared borrow must cross tasks.
    let offset = Mutex::new(1u32);
    let guard = offset.lock().unwrap();
    let key = move |row: &(Cell<u32>, String)| Reverse(row.0.get() + *guard);
    let mut rows = [
        (Cell::new(2), String::from("first")),
        (Cell::new(1), String::from("middle")),
        (Cell::new(2), String::from("last")),
    ];
    par_stable_sort_by_key(&mut rows, key);
    let names = rows.each_ref().map(|row| row.1.as_str());
    assert_eq!(names, ["first", "last", "middle"]);
}

#[test]
#[cfg(not(miri))]
fn bounded_callback_can_reenter() {
    // This crosses bounded finishing's production cutoff with Send-only
    // records, a Sync-but-not-Send callback, and nested Rayon work from a
    // private staged bucket rather than the original input or root tail.
    let len = (1usize << 21) + 37;
    let mut rows: Vec<_> = (0..len as u32)
        .map(|id| (id.wrapping_mul(7919) % 8192, Cell::new(id)))
        .collect();
    let begin = rows.as_ptr() as usize;
    let end = begin + rows.len() * core::mem::size_of_val(&rows[0]);
    let entered = AtomicBool::new(false);
    let offset = Mutex::new(0u32);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| {
        let guard = offset.lock().unwrap();
        let entered = &entered;
        par_stable_sort_by_key(&mut rows, move |row| {
            let address = core::ptr::from_ref(row) as usize;
            if !(begin..end).contains(&address)
                && row.1.get() < len as u32 - 1024
                && !entered.swap(true, Ordering::Relaxed)
            {
                let (left, right) =
                    rayon::join(|| (0..1024u64).sum::<u64>(), || (0..1024u64).sum::<u64>());
                assert_eq!(left, right);
            }
            row.0 + *guard
        });
    });
    assert!(entered.load(Ordering::Relaxed));
    let mut seen = vec![false; len];
    for row in &rows {
        let id = row.1.get() as usize;
        assert!(!core::mem::replace(&mut seen[id], true));
        assert_eq!(row.0, (id as u32).wrapping_mul(7919) % 8192);
    }
    for pair in rows.windows(2) {
        assert!(pair[0].0 <= pair[1].0);
        if pair[0].0 == pair[1].0 {
            assert!(pair[0].1.get() < pair[1].1.get());
        }
    }
}

#[test]
fn caller_pools_are_supported() {
    for threads in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let mut rows: Vec<_> = (0..16_384u32)
            .map(|index| ((index.wrapping_mul(7919) % 257), index))
            .collect();
        let mut expected = rows.clone();
        expected.sort_by_key(|row| row.0);
        pool.install(|| {
            par_stable_sort_by_key(&mut rows, |row| {
                assert_eq!(rayon::current_num_threads(), threads);
                row.0
            });
        });
        assert_eq!(rows, expected);
    }
}

#[test]
fn large_sort_uses_workers_and_recovers_task_panics() {
    const LEN: usize = 1_000_000;

    let mut rows: Vec<_> = (0..LEN as u32).map(|id| (mix(u64::from(id)), id)).collect();
    let original = rows.clone();
    let mut expected = original.clone();
    expected.sort_by_key(|row| row.0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();

    let calls = AtomicUsize::new(0);
    let workers = Mutex::new(HashSet::new());
    pool.install(|| {
        par_stable_sort_by_key(&mut rows, |row| {
            if calls.fetch_add(1, Ordering::Relaxed) % 4096 == 0 {
                workers
                    .lock()
                    .unwrap()
                    .insert(rayon::current_thread_index().unwrap());
            }
            row.0
        });
    });
    assert_eq!(rows, expected);
    assert!(
        workers.lock().unwrap().len() > 1,
        "sorting work stayed on one Rayon worker"
    );

    rows.clone_from_slice(&original);
    let calls = AtomicUsize::new(0);
    let origin = Mutex::new(None);
    let panicked = AtomicBool::new(false);
    let panic = catch_unwind(AssertUnwindSafe(|| {
        pool.install(|| {
            par_stable_sort_by_key(&mut rows, |row| {
                let call = calls.fetch_add(1, Ordering::Relaxed);
                let current = std::thread::current().id();
                if call == 0 {
                    *origin.lock().unwrap() = Some(current);
                } else if call > LEN
                    && origin
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_some_and(|origin| *origin != current)
                    && !panicked.swap(true, Ordering::Relaxed)
                {
                    panic!("key panic in a child task");
                }
                row.0
            });
        });
    }));
    assert!(panic.is_err());
    assert!(panicked.load(Ordering::Relaxed));
    let mut ids: Vec<_> = rows.into_iter().map(|row| row.1).collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..LEN as u32).collect::<Vec<_>>());
}

#[test]
#[cfg(not(miri))]
fn repeated_root_uses_workers_stably() {
    let len = 1usize << 18;
    let mut records: Vec<_> = (0..len)
        .map(|id| {
            let key = if id % 16 == 0 {
                mix(id as u64)
            } else {
                1 << 63
            };
            (Cell::new(key), id)
        })
        .collect();
    let mut expected: Vec<_> = records.iter().map(|row| (row.0.get(), row.1)).collect();
    expected.sort_by_key(|row| row.0);
    let calls = AtomicUsize::new(0);
    let workers = Mutex::new(HashSet::new());
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| {
        par_stable_sort_by_key(&mut records, |row| {
            if calls.fetch_add(1, Ordering::Relaxed) % 1024 == 0 {
                let mut seen = workers.lock().unwrap();
                seen.insert(rayon::current_thread_index().unwrap());
                let alone = seen.len() == 1;
                drop(seen);
                if alone {
                    // Let pending shards run even on oversubscribed hosts;
                    // callbacks may block without holding engine metadata.
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
            row.0.get()
        });
    });
    assert_eq!(
        records
            .iter()
            .map(|row| (row.0.get(), row.1))
            .collect::<Vec<_>>(),
        expected
    );
    assert!(
        workers.lock().unwrap().len() > 1,
        "repeated root stayed serial"
    );
}

#[test]
#[cfg(not(miri))]
fn repeated_root_key_panic_keeps_owned_records() {
    let len = 1usize << 17;
    let mut records: Vec<_> = (0..len as u32)
        .map(|id| {
            let key = match id {
                0 => 0,
                16 => u64::MAX,
                _ if id % 16 == 0 => mix(u64::from(id)),
                _ => 1 << 63,
            };
            (Cell::new(key), Box::new(id))
        })
        .collect();
    let offset = Mutex::new(0u64);
    let panicked = AtomicBool::new(false);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let result = catch_unwind(AssertUnwindSafe(|| {
        pool.install(|| {
            let guard = offset.lock().unwrap();
            let panicked = &panicked;
            par_stable_sort_by_key(&mut records, move |row| {
                // Sampling misses the last record; keys 0 and MAX complete
                // variation in the prefix. Fail during partition regardless
                // of which worker happens to process the final shard.
                if *row.1 == len as u32 - 1 {
                    panicked.store(true, Ordering::Relaxed);
                    panic!("repeated-root partition key panic");
                }
                row.0.get().wrapping_add(*guard)
            });
        });
    }));
    assert!(result.is_err());
    assert!(panicked.load(Ordering::Relaxed));
    for row in &records {
        let id = *row.1;
        let key = match id {
            0 => 0,
            16 => u64::MAX,
            _ if id % 16 == 0 => mix(u64::from(id)),
            _ => 1 << 63,
        };
        assert_eq!(row.0.get(), key);
    }
    let mut ids: Vec<_> = records.into_iter().map(|row| *row.1).collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..len as u32).collect::<Vec<_>>());
}

#[test]
#[cfg(not(miri))]
fn changing_repeated_keys_keep_records() {
    let len = 1usize << 17;
    let mut records: Vec<_> = (0..len).map(Box::new).collect();
    let calls = AtomicUsize::new(0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| {
        par_stable_sort_by_key(&mut records, |_| {
            let call = calls.fetch_add(1, Ordering::Relaxed);
            assert!(
                call < 100 * len,
                "changing equality keys did not make progress"
            );
            if call % 16 == 0 { u64::MAX } else { 0 }
        });
    });
    let mut ids: Vec<_> = records.into_iter().map(|record| *record).collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..len).collect::<Vec<_>>());
}

fn mix(value: u64) -> u64 {
    let mut value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[test]
fn sparse_root_keeps_stability() {
    const LEN: usize = 70_000;

    let keys: Vec<_> = (0..64).map(mix).collect();
    let mut rows: Vec<_> = (0..LEN as u32)
        .map(|id| (keys[(mix(u64::from(id)) % keys.len() as u64) as usize], id))
        .collect();
    let mut expected = rows.clone();
    expected.sort_by_key(|row| row.0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();

    pool.install(|| par_stable_sort_by_key(&mut rows, |row| row.0));

    assert_eq!(rows, expected);
}

#[test]
fn changing_keys_keep_records() {
    let mut records: Vec<_> = (0..16_384u32).map(Box::new).collect();
    let calls = AtomicUsize::new(0);
    par_stable_sort_by_key(&mut records, |_| {
        calls.fetch_add(7919, Ordering::Relaxed) as u32
    });
    let mut recovered: Vec<_> = records.into_iter().map(|record| *record).collect();
    recovered.sort();
    assert_eq!(recovered, (0..16_384).collect::<Vec<_>>());
}

#[test]
fn panic_keeps_borrowed_records() {
    let mut values: Vec<_> = (0..1024u32).collect();
    let mut records: Vec<_> = values.iter_mut().collect();
    let calls = AtomicUsize::new(0);
    let panic = catch_unwind(AssertUnwindSafe(|| {
        par_stable_sort_by_key(&mut records, |record| {
            assert_ne!(calls.fetch_add(1, Ordering::Relaxed), 41, "key panic");
            Reverse(**record)
        });
    }));
    assert!(panic.is_err());
    let mut recovered: Vec<_> = records.iter().map(|record| **record).collect();
    recovered.sort();
    assert_eq!(recovered, (0..1024).collect::<Vec<_>>());
    for record in records {
        *record += 1;
    }
    assert_eq!(values, (1..=1024).collect::<Vec<_>>());
}
