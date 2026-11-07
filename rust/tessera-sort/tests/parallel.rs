// SPDX-License-Identifier: BSD-2-Clause

//! Contracts of the feature-gated Rayon entry points, including the fallback.

#![cfg(feature = "rayon")]

use core::cell::Cell;
use core::cmp::Reverse;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
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
