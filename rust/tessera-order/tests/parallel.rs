// SPDX-License-Identifier: BSD-2-Clause

//! Contracts of the feature-gated Rayon entry points, including the fallback.

#![cfg(feature = "rayon")]

use core::cell::Cell;
use core::cmp::Reverse;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tessera_order::par_sort;
use tessera_order::par_sort_by_key;

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
            par_sort(&mut keys);
            assert_eq!(keys, expected);
        }
        let mut keys: Vec<_> = (0..len)
            .map(|id| (((id * 7919) & 255) as i32 - 512) << 16)
            .collect();
        let mut expected = keys.clone();
        expected.sort();
        par_sort(&mut keys);
        assert_eq!(keys, expected);

        // Distinct NaN payloads, for both signs, must survive run filling.
        for sign in [0, 1u64 << 63] {
            let mut keys: Vec<_> = (0..len)
                .map(|id| f64::from_bits(sign | 0x7ff8_0000_0000_0000 | ((id * 7919) & 255) as u64))
                .collect();
            let mut reverse: Vec<_> = keys.iter().copied().map(Reverse).collect();
            let mut reversed_expected = reverse.clone();
            reversed_expected.sort_by(|a, b| b.0.total_cmp(&a.0));
            par_sort(&mut reverse);
            assert!(
                reverse
                    .iter()
                    .zip(&reversed_expected)
                    .all(|(a, b)| a.0.to_bits() == b.0.to_bits())
            );
            let mut expected = keys.clone();
            expected.sort_by(f64::total_cmp);
            par_sort(&mut keys);
            assert!(
                keys.iter()
                    .zip(&expected)
                    .all(|(a, b)| a.to_bits() == b.to_bits())
            );
        }
    });
}

/// Covering digits keep equal-key payload order without child finishing.
#[test]
#[cfg(not(miri))]
fn covering_digit_keeps_payload_order() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let mut rows: Vec<_> = (0..1usize << 17)
        .map(|id| (((id * 7919) & 255) as u64, Cell::new(id)))
        .collect();
    pool.install(|| par_sort_by_key(&mut rows, |row| row.0));
    assert!(rows.windows(2).all(|pair| pair[0].0 < pair[1].0
        || (pair[0].0 == pair[1].0 && pair[0].1.get() < pair[1].1.get())));
    for row in &rows {
        assert_eq!(row.0, ((row.1.get() * 7919) & 255) as u64);
    }
}

/// Both orientations of pre-separated buckets use the same stable run split.
#[test]
#[cfg(not(miri))]
fn separated_runs_match_serial() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let len = 1usize << 17;
    for set_first in [false, true] {
        let mut rows: Vec<_> = (0..len)
            .map(|id| {
                let set = (id < len / 2) == set_first;
                (((set as u64) << 63) | ((id as u64 * 7919) % 65521), id)
            })
            .collect();
        let mut expected = rows.clone();
        expected.sort_by_key(|row| row.0);
        let mut serial = rows.clone();
        tessera_order::sort_by_key(&mut serial, |row| row.0);
        pool.install(|| par_sort_by_key(&mut rows, |row| row.0));
        assert_eq!(serial, expected);
        assert_eq!(rows, expected);
    }
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
    par_sort_by_key(&mut rows, key);
    let names = rows.each_ref().map(|row| row.1.as_str());
    assert_eq!(names, ["first", "last", "middle"]);
}

#[test]
#[cfg(not(miri))]
fn materialized_tile_callback_can_reenter() {
    // Exercise the whole-child tile allowance with Send-only records and a
    // Sync-but-not-Send callback. Reenter from tile scratch rather than the
    // original input or the root scatter's tail buffer.
    let len = (1usize << 21) + 37;
    let mut rows: Vec<_> = (0..len as u32)
        // Broad variation needs a second tile pass, so extraction actually
        // visits tile scratch after direct final-band delivery.
        .map(|id| ((id / 2).wrapping_mul(7919), Cell::new(id)))
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
        par_sort_by_key(&mut rows, move |row| {
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
        assert_eq!(row.0, (id as u32 / 2).wrapping_mul(7919));
    }
    for pair in rows.windows(2) {
        assert!(pair[0].0 <= pair[1].0);
        if pair[0].0 == pair[1].0 {
            assert!(pair[0].1.get() < pair[1].1.get());
        }
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
        par_sort_by_key(&mut rows, |row| {
            if calls.fetch_add(1, Ordering::Relaxed).is_multiple_of(4096) {
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
            par_sort_by_key(&mut rows, |row| {
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
fn repeated_root_uses_workers_and_recovers_owned_records() {
    let len = 1usize << 18;
    let original: Vec<_> = (0..len)
        .map(|id| {
            let key = match id {
                0 => 0,
                16 => u64::MAX,
                _ if id % 16 == 0 => mix(id as u64),
                _ => 1 << 63,
            };
            (Cell::new(key), Box::new(id))
        })
        .collect();
    let mut records = original.clone();
    let mut expected = original.clone();
    expected.sort_by_key(|row| row.0.get());
    let calls = AtomicUsize::new(0);
    let workers = Mutex::new(HashSet::new());
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| {
        par_sort_by_key(&mut records, |row| {
            if calls.fetch_add(1, Ordering::Relaxed).is_multiple_of(1024) {
                let mut seen = workers.lock().unwrap();
                seen.insert(rayon::current_thread_index().unwrap());
                let alone = seen.len() == 1;
                drop(seen);
                if alone {
                    // Let pending shards run on oversubscribed hosts without
                    // holding engine metadata across a blocking callback.
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
            row.0.get()
        });
    });
    assert_eq!(records, expected);
    assert!(
        workers.lock().unwrap().len() > 1,
        "repeated root stayed serial"
    );

    records.clone_from(&original);
    let offset = Mutex::new(0u64);
    let result = catch_unwind(AssertUnwindSafe(|| {
        pool.install(|| {
            // The guard makes this callback Sync but not Send.
            let guard = offset.lock().unwrap();
            par_sort_by_key(&mut records, move |row| {
                // Sampling misses the tail; 0 and MAX complete variation in
                // the prefix. Fail in partition regardless of shard scheduling.
                assert_ne!(*row.1, len - 1, "repeated-root partition key panic");
                row.0.get().wrapping_add(*guard)
            });
        });
    }));
    assert!(result.is_err());
    records.sort_by_key(|row| *row.1);
    assert_eq!(records, original);
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
    let limit = 100 * len;
    // Changing keys may make the sort panic or return; neither may lose a record.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        pool.install(|| {
            par_sort_by_key(&mut records, |_| {
                let call = calls.fetch_add(1, Ordering::Relaxed);
                assert!(call < limit, "stop a sort that makes no progress");
                if call.is_multiple_of(16) { u64::MAX } else { 0 }
            });
        });
    }));
    assert!(
        calls.load(Ordering::Relaxed) <= limit,
        "changing equality keys did not make progress"
    );
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

    pool.install(|| par_sort_by_key(&mut rows, |row| row.0));

    assert_eq!(rows, expected);
}

#[test]
fn changing_keys_keep_records() {
    let mut records: Vec<_> = (0..16_384u32).map(Box::new).collect();
    let calls = AtomicUsize::new(0);
    // Changing keys may make the sort panic or return; neither may lose a record.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        par_sort_by_key(&mut records, |_| {
            calls.fetch_add(7919, Ordering::Relaxed) as u32
        });
    }));
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
        par_sort_by_key(&mut records, |record| {
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

#[test]
#[cfg_attr(
    miri,
    ignore = "parallel root cutoff requires >1 MiB input; Miri covers serial confirmation and Rayon block recovery"
)]
fn ordered_constant_and_reversed_roots_match_serial() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    // Lengths above MIN_PARALLEL_BYTES, including odd and non-multiple-of-4 sizes.
    for len in [131_073usize, 262_144] {
        // Strictly descending: exercises parallel confirm + parallel reverse.
        let mut desc: Vec<u64> = (0..len as u64).rev().collect();
        pool.install(|| par_sort(&mut desc));
        assert!(desc.iter().copied().eq(0..len as u64));

        // Globally nonincreasing with strictly decreasing shards and a single
        // tie right across the first shard boundary: only the cross-shard check
        // sees the tie, so strict reversal would swap the tied pair's order.
        let shard_len = len.div_ceil(4);
        let mut tied: Vec<(u64, usize)> = (0..len)
            .map(|id| {
                let adjusted = if id >= shard_len { id - 1 } else { id };
                ((len - adjusted) as u64, id)
            })
            .collect();
        assert_eq!(tied[shard_len - 1].0, tied[shard_len].0);
        let mut expected = tied.clone();
        expected.sort_by_key(|row| row.0);
        pool.install(|| par_sort_by_key(&mut tied, |row| row.0));
        assert_eq!(tied, expected);

        // Full-width flat-sampled roots: ordered (0 at index 0) and unsorted
        // (0 at index 1) both produce a flat order sample and full-width prefix
        // variation; deferred confirmation must confirm the ordered one and
        // reject the unsorted one.
        let mut full_width = vec![u64::MAX; len];
        full_width[0] = 0;
        pool.install(|| par_sort(&mut full_width));
        assert_eq!(full_width[0], 0);
        assert!(full_width[1..].iter().all(|&v| v == u64::MAX));

        let mut full_width_unsorted = vec![u64::MAX; len];
        full_width_unsorted[1] = 0;
        pool.install(|| par_sort(&mut full_width_unsorted));
        assert_eq!(full_width_unsorted[0], 0);
        assert!(full_width_unsorted[1..].iter().all(|&v| v == u64::MAX));

        // Two-run constant suffix: 256 zeros followed by u64::MAX. Each suffix
        // shard's first key already proves full-width variation against key 0.
        let mut two_run = vec![u64::MAX; len];
        two_run[..256].fill(0);
        pool.install(|| par_sort(&mut two_run));
        assert!(two_run[..256].iter().all(|&v| v == 0));
        assert!(two_run[256..].iter().all(|&v| v == u64::MAX));

        // Constant input.
        let mut constant = vec![42u64; len];
        pool.install(|| par_sort(&mut constant));
        assert!(constant.iter().all(|&v| v == 42));
    }

    // Verify multiple Rayon workers execute parallel order confirmation
    // (ascending input) and parallel varying-bit scanning (constant input
    // reaches only the variation scan after flat sampling).
    let len = 131_073usize;
    for constant in [false, true] {
        let workers = Mutex::new(HashSet::new());
        let mut rows: Vec<_> = (0..len)
            .map(|id| (if constant { 7u64 } else { id as u64 }, Cell::new(id)))
            .collect();
        pool.install(|| {
            par_sort_by_key(&mut rows, |row| {
                if row.1.get().is_multiple_of(8192) {
                    let mut seen = workers.lock().unwrap();
                    seen.insert(rayon::current_thread_index().unwrap());
                    let alone = seen.len() == 1;
                    drop(seen);
                    if alone {
                        // Let pending shards run on oversubscribed hosts.
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }
                row.0
            });
        });
        assert!(
            workers.lock().unwrap().len() > 1,
            "parallel root preparation stayed on one worker (constant={constant})"
        );
    }

    // Trigger a panic on a nonsampled record (neither a multiple of 128 nor
    // immediately before one) inside a confirmation shard running on a worker
    // other than the thread that ran serial order sampling.
    let mut rows: Vec<_> = (0..len).map(|id| (id as u64, Cell::new(id))).collect();
    let panicked_id = std::sync::atomic::AtomicUsize::new(usize::MAX);
    let panic_worker = std::sync::atomic::AtomicUsize::new(usize::MAX);
    let sample_worker = std::sync::atomic::AtomicUsize::new(usize::MAX);
    let panic = catch_unwind(AssertUnwindSafe(|| {
        pool.install(|| {
            let origin = rayon::current_thread_index().unwrap();
            sample_worker.store(origin, Ordering::Relaxed);
            par_sort_by_key(&mut rows, |row| {
                let id = row.1.get();
                let worker = rayon::current_thread_index().unwrap();
                if id == 17 {
                    // Hold shard 0 briefly so another worker runs a peer shard.
                    for _ in 0..200 {
                        if panicked_id.load(Ordering::Acquire) != usize::MAX {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                } else if worker != origin
                    && id % 128 != 0
                    && (id + 1) % 128 != 0
                    && panicked_id
                        .compare_exchange(usize::MAX, id, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                {
                    panic_worker.store(worker, Ordering::Relaxed);
                    row.1.set(usize::MAX);
                    panic!("panic during parallel root confirmation");
                }
                row.0
            });
        });
    }));
    assert!(panic.is_err());
    let target = panicked_id.load(Ordering::Acquire);
    assert_ne!(target, usize::MAX);
    assert_ne!(
        panic_worker.load(Ordering::Relaxed),
        sample_worker.load(Ordering::Relaxed)
    );
    assert_eq!(rows[target].1.get(), usize::MAX);
    assert!(
        rows.iter()
            .enumerate()
            .all(|(i, row)| row.0 == i as u64 && (i == target || row.1.get() == i))
    );
}
