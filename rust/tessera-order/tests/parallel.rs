// SPDX-License-Identifier: BSD-2-Clause

//! Contracts of the feature-gated Rayon entry points, including the fallback.

#![cfg(feature = "rayon")]

use core::cell::Cell;
use core::cmp::Reverse;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tessera_order::{RadixKey, par_argsort, par_argsort_by_key, par_sort, par_sort_by_key};

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
    ignore = "compile-time generic API bound check; exercises only the serial fallback"
)]
fn generic_radix_key_calls_par_sort_without_send_bound() {
    fn sort_generic<K: RadixKey>(values: &mut [K]) {
        par_sort(values);
    }

    fn argsort_generic<K: RadixKey>(values: &[K]) -> Vec<usize> {
        par_argsort(values)
    }

    #[allow(deprecated)]
    fn stable_sort_generic<K: RadixKey>(values: &mut [K]) {
        tessera_order::par_stable_sort(values);
    }

    let mut values = [3u64, 1, 2, 1];
    assert_eq!(argsort_generic(&values), [1, 3, 2, 0]);
    sort_generic(&mut values);
    assert_eq!(values, [1, 1, 2, 3]);

    let mut legacy = [4i32, -2, 0, -2];
    stable_sort_generic(&mut legacy);
    assert_eq!(legacy, [-2, -2, 0, 4]);
}

#[test]
fn par_argsort_sync_only_records_and_small_fallback() {
    // MutexGuard makes both records and the key closure Sync but not Send.
    struct SyncOnly<'a>(
        u32,
        core::marker::PhantomData<std::sync::MutexGuard<'a, ()>>,
    );
    let lock = Mutex::new(());
    let guard = lock.lock().unwrap();
    let offset = Mutex::new(1u32);
    let offset_guard = offset.lock().unwrap();
    let rows = [
        SyncOnly(2, core::marker::PhantomData),
        SyncOnly(1, core::marker::PhantomData),
        SyncOnly(2, core::marker::PhantomData),
    ];
    let _ = &guard;
    let key = move |row: &SyncOnly<'_>| Reverse(row.0 + *offset_guard);
    assert_eq!(par_argsort_by_key(&rows, key), [0, 2, 1]);
    assert_eq!(tessera_order::par_argsort::<u32>(&[]), Vec::<usize>::new());
    assert_eq!(tessera_order::par_argsort(&[42u64]), [0]);

    let floats = [
        f64::from_bits(0x7ff8_0000_0000_0002),
        0.0,
        -0.0,
        f64::from_bits(0xfff8_0000_0000_0001),
        f64::from_bits(0x7ff8_0000_0000_0001),
        -0.0,
    ];
    assert_eq!(
        tessera_order::par_argsort(&floats),
        tessera_order::argsort(&floats)
    );
    let one = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let tuples: Vec<_> = (0..128u32).map(|i| (i % 7, Reverse(i / 7))).collect();
    let got = one.install(|| tessera_order::par_argsort(&tuples));
    assert_eq!(got, tessera_order::argsort(&tuples));
}

#[test]
#[cfg_attr(
    miri,
    ignore = "131k-record Rayon stress fixture; par_argsort adds no unsafe code and Miri covers its serial fallback plus shared block::parallel and bucket_borrow_tests"
)]
fn par_argsort_matches_serial_in_caller_pool() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let len = 1usize << 17;

    // Narrow keys (remaining == 0): verifies stability and that every callback
    // runs inside the caller's Rayon pool rather than a private pool.
    let narrow: Vec<u32> = (0..len).map(|i| (mix(i as u64) & 0xfff) as u32).collect();
    let in_pool = AtomicUsize::new(0);
    let outside_pool = AtomicBool::new(false);
    let got = pool.install(|| {
        par_argsort_by_key(&narrow, |&v| {
            if pool.current_thread_index().is_some() {
                in_pool.fetch_add(1, Ordering::Relaxed);
            } else {
                outside_pool.store(true, Ordering::Relaxed);
            }
            v
        })
    });
    assert_eq!(got, tessera_order::argsort(&narrow));
    assert!(in_pool.load(Ordering::Relaxed) >= len);
    assert!(
        !outside_pool.load(Ordering::Relaxed),
        "par_argsort_by_key ran a callback outside the caller's Rayon pool"
    );

    // Wide u64 keys (remaining != 0): random, moderate prefix ties, and one huge prefix tie.
    let wide_random: Vec<u64> = (0..len).map(|i| mix(i as u64)).collect();
    assert_eq!(
        pool.install(|| par_argsort(&wide_random)),
        tessera_order::argsort(&wide_random)
    );

    let prefix_ties: Vec<u64> = (0..len)
        .map(|i| {
            // High bits at 60..63 leave low varying bits below prefix_shift (16),
            // exercising both `bands` (~4k ties) and `huge` (~98k tie > chunk).
            let hi = if i % 4 == 0 {
                (mix(i as u64) & 7) << 60
            } else {
                3u64 << 60
            };
            hi | (mix(i as u64) & 0xffff)
        })
        .collect();
    let tie_outside = AtomicBool::new(false);
    let tie_got = pool.install(|| {
        par_argsort_by_key(&prefix_ties, |&v| {
            if pool.current_thread_index().is_none() {
                tie_outside.store(true, Ordering::Relaxed);
            }
            v
        })
    });
    assert_eq!(tie_got, tessera_order::argsort(&prefix_ties));
    assert!(
        !tie_outside.load(Ordering::Relaxed),
        "tie refinement ran a callback outside the caller's Rayon pool"
    );

    // 1-thread pool fallback on a large input and ZST records on a multi-thread pool.
    let one = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    assert_eq!(
        one.install(|| par_argsort(&prefix_ties)),
        tessera_order::argsort(&prefix_ties)
    );
    let zsts = vec![(); len];
    assert_eq!(
        pool.install(|| par_argsort_by_key(&zsts, |()| 0u32)),
        (0..len).collect::<Vec<_>>()
    );

    // 128-bit keys, tuple keys, and narrow-mask reversed/primitive keys.
    let wide128: Vec<u128> = (0..len)
        .map(|i| (u128::from(mix(i as u64) & 0xff) << 96) | u128::from(mix((i ^ 0x55aa) as u64)))
        .collect();
    assert_eq!(
        pool.install(|| par_argsort(&wide128)),
        tessera_order::argsort(&wide128)
    );
    let tuples: Vec<(u32, Reverse<u64>)> = (0..len)
        .map(|i| ((i % 16) as u32, Reverse(mix(i as u64))))
        .collect();
    assert_eq!(
        pool.install(|| par_argsort(&tuples)),
        tessera_order::argsort(&tuples)
    );
    let rev_chars: Vec<Reverse<char>> = (0..len)
        .map(|i| {
            let cp = (mix(i as u64) as u32) % 0x10_0000;
            Reverse(
                char::from_u32(if (0xd800..=0xdfff).contains(&cp) {
                    0x41
                } else {
                    cp
                })
                .unwrap(),
            )
        })
        .collect();
    assert_eq!(
        pool.install(|| par_argsort(&rev_chars)),
        tessera_order::argsort(&rev_chars)
    );
    let rev_bools: Vec<Reverse<bool>> = (0..len).map(|i| Reverse(mix(i as u64) & 1 != 0)).collect();
    assert_eq!(
        pool.install(|| par_argsort(&rev_bools)),
        tessera_order::argsort(&rev_bools)
    );

    // Floats including signed zeros and distinct NaN payloads.
    let floats: Vec<f64> = (0..len)
        .map(|i| match i % 8 {
            0 => -0.0,
            1 => 0.0,
            2 => f64::from_bits(0x7ff8_0000_0000_0000 | (i as u64 & 0xff)),
            3 => f64::from_bits(0xfff8_0000_0000_0000 | (i as u64 & 0xff)),
            4 => f64::NEG_INFINITY,
            5 => f64::INFINITY,
            _ => (mix(i as u64) as i64) as f64,
        })
        .collect();
    assert_eq!(
        pool.install(|| par_argsort(&floats)),
        tessera_order::argsort(&floats)
    );

    // Ordered, descending (strict and with ties), constant, and flat-with-gap inputs.
    let ascending: Vec<u64> = (0..len as u64).collect();
    let strict_desc: Vec<u64> = (0..len as u64).rev().collect();
    let tied_desc: Vec<u64> = (0..len as u64).rev().map(|x| x / 4).collect();
    let constant = vec![7u64; len];
    let mut flat_unsorted = vec![7u64; len];
    flat_unsorted[50] = 9;
    flat_unsorted[51] = 3;
    let mut flat_ascending = vec![7u64; len];
    flat_ascending[50..].fill(9);
    let mut flat_descending = vec![9u64; len];
    flat_descending[50..].fill(7);
    // Huge-tie bands with a nonzero packed prefix (so `sub == 0` requires real
    // unmasking) and unsorted prefix-0 outliers (`i ^ 1`) supplying nonzero
    // `remaining` bits without tripping the ascending fast path:
    // (1) all-equal huge band, (2) inverted late witness, (3) cross-chunk
    // constant halves where only cross-chunk `(u1 | u2, i1 & i2)` sees variation.
    const HUGE_PREFIX: u64 = 1u64 << 62;
    let outliers = 128; // keeps huge-band length (len - outliers) divisible by 4
    let huge_tie = |fill: u64| {
        let mut data = vec![fill; len];
        for (i, slot) in data.iter_mut().take(outliers).enumerate() {
            *slot = (i ^ 1) as u64;
        }
        data
    };
    let skew_equal_huge = huge_tie(HUGE_PREFIX);
    let mut skew_late_low_bit = huge_tie(HUGE_PREFIX | 1);
    skew_late_low_bit[len - 1] = HUGE_PREFIX;
    let mut skew_cross_chunk = huge_tie(HUGE_PREFIX | 1);
    skew_cross_chunk[outliers + (len - outliers) / 2..].fill(HUGE_PREFIX);
    for case in [
        ascending,
        strict_desc,
        tied_desc,
        constant,
        flat_unsorted,
        flat_ascending,
        flat_descending,
        skew_equal_huge,
        skew_late_low_bit,
        skew_cross_chunk,
    ] {
        assert_eq!(
            pool.install(|| par_argsort(&case)),
            tessera_order::argsort(&case)
        );
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "131k-record Rayon panic/inconsistent-key stress fixture; par_argsort adds no unsafe code and Miri covers shared parallel recovery in block::parallel and bucket_borrow_tests"
)]
fn par_argsort_recovers_panics_and_inconsistent_keys() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let len = 1usize << 17;

    // Split 131,072 rows across three small-tie prefixes (24,576 + 24,576 +
    // 16,384, forming two non-empty `bands` under 4 threads where chunk = 32,768)
    // and one huge-tie prefix `4 << 60` (65,536 > chunk, refined via parallel
    // `Sorter::sort_root`). Bit 62 keeps all 16 low bits outside the packed prefix.
    struct Row {
        id: usize,
        key: u64,
        calls: AtomicUsize,
        edit: AtomicUsize,
    }
    let rows: Vec<Row> = (0..len)
        .map(|id| {
            let hi = match id % 16 {
                0..3 => 0u64 << 60,
                3..6 => 1u64 << 60,
                6..8 => 2u64 << 60,
                _ => 4u64 << 60,
            };
            Row {
                id,
                key: hi | (mix(id as u64) & 0xffff),
                calls: AtomicUsize::new(0),
                edit: AtomicUsize::new(0),
            }
        })
        .collect();

    // Observe baseline per-row call counts dynamically so panic injection does
    // not pin internal pass counts or sampling strides.
    let baseline = pool.install(|| {
        par_argsort_by_key(&rows, |row| {
            row.calls.fetch_add(1, Ordering::Relaxed);
            row.key
        })
    });
    assert!(
        baseline
            .windows(2)
            .all(|pair| (rows[pair[0]].key, pair[0]) < (rows[pair[1]].key, pair[1]))
    );
    let small_id = (len / 2..len).find(|&id| id % 16 == 6).unwrap();
    let huge_id = (len / 2..len).find(|&id| id % 16 == 8).unwrap();
    let small_calls = rows[small_id].calls.load(Ordering::Relaxed).max(1);
    let huge_calls = rows[huge_id].calls.load(Ordering::Relaxed).max(1);

    // Inject distinct panics at observed early, mid, small-tie, and huge-tie
    // call positions; each must propagate with its interior edit intact.
    for (tag, target_id, target_call, sentinel) in [
        ("early-panic", huge_id, 1usize, 101usize),
        ("mid-panic", huge_id, 2usize.min(huge_calls), 202usize),
        ("small-tie-panic", small_id, small_calls, 303usize),
        ("huge-tie-panic", huge_id, huge_calls, 404usize),
    ] {
        for row in &rows {
            row.calls.store(0, Ordering::Relaxed);
            row.edit.store(0, Ordering::Relaxed);
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            pool.install(|| {
                par_argsort_by_key(&rows, |row| {
                    let row_call = row.calls.fetch_add(1, Ordering::Relaxed) + 1;
                    if row.id == target_id && row_call == target_call {
                        row.edit.store(sentinel, Ordering::Release);
                        std::panic::panic_any(tag);
                    }
                    row.key
                })
            })
        }));
        let err = result.expect_err("key panic must propagate");
        assert_eq!(err.downcast_ref::<&str>(), Some(&tag));
        assert_eq!(rows[target_id].edit.load(Ordering::Acquire), sentinel);
    }

    // Inconsistent keys may panic or return a valid permutation of `0..len`.
    let source: Vec<usize> = (0..len).collect();
    for mode in 0..3 {
        let counter = AtomicUsize::new(0);
        let limit = 100 * len;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            pool.install(|| {
                par_argsort_by_key(&source, |_| {
                    let c = counter.fetch_add(1, Ordering::Relaxed);
                    assert!(c < limit, "inconsistent par_argsort made no progress");
                    match mode {
                        0 => (c.wrapping_mul(7919) & 0xffff) as u64,
                        1 => mix(c as u64),
                        _ => {
                            if c.is_multiple_of(16) {
                                u64::MAX
                            } else {
                                (c & 0xff) as u64
                            }
                        }
                    }
                })
            })
        }));
        assert!(counter.load(Ordering::Relaxed) <= limit);
        if let Ok(mut perm) = outcome {
            perm.sort_unstable();
            assert_eq!(perm, (0..len).collect::<Vec<_>>());
        }
    }
}
