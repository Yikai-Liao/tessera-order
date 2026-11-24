// SPDX-License-Identifier: BSD-2-Clause

//! Cached-key sorting: order, stability, key evaluation and ownership.

use std::{
    cell::Cell,
    cmp::{Ordering, Reverse},
    panic::{AssertUnwindSafe, catch_unwind},
};
use tessera_order::{RadixKey, sort_by_cached_key};

/// Lengths covering one tile, block movement and pair sorting beyond a tile.
fn lengths() -> &'static [usize] {
    if cfg!(miri) {
        &[0, 1, 2, 65]
    } else {
        &[0, 1, 2, 48, 129, 8193, 70_000]
    }
}

/// Sorts `(index, String)` records by `key` and checks the result against the
/// standard stable sort and that each key was evaluated once, in input order.
fn check<K: RadixKey>(key: impl Fn(usize) -> K, compare: impl Fn(&K, &K) -> Ordering) {
    for &len in lengths() {
        let mut records: Vec<_> = (0..len).map(|id| (id, id.to_string())).collect();
        let mut expected = records.clone();
        expected.sort_by(|a, b| compare(&key(a.0), &key(b.0)));
        let mut visited = Vec::new();
        sort_by_cached_key(&mut records, |record| {
            visited.push(record.0);
            key(record.0)
        });
        assert_eq!(records, expected, "len {len}");
        let expected_calls: Vec<_> = if len < 2 {
            Vec::new()
        } else {
            (0..len).collect()
        };
        assert_eq!(visited, expected_calls, "len {len}");
    }
}

/// A scrambled key with repeated values, so stability is observable.
fn scramble(id: usize) -> u64 {
    (id as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 7
}

#[test]
fn keys_varying_within_one_distribution_sort_stably_with_one_call_per_record() {
    check(|id| scramble(id) as u8, u8::cmp);
    check(|id| scramble(id) as i8, i8::cmp);
    check(|id| Reverse(scramble(id) as u8), Reverse::cmp);
    check(|id| (scramble(id) % 3) as u8, u8::cmp);
    check(|id| (scramble(id) % 16) as u16, u16::cmp);
    // A shifted window with constant bits on both sides.
    check(
        |id| 0xab00_0000 | ((scramble(id) % 64) << 12) as u32,
        u32::cmp,
    );
    // The widest single distribution, then one bit more.
    check(|id| scramble(id) % 4096, u64::cmp);
    check(|id| scramble(id) % 8192, u64::cmp);
    check(|id| Reverse((scramble(id) % 257) as u128), Reverse::cmp);
}

#[test]
fn keys_varying_beyond_one_distribution_sort_stably_with_one_call_per_record() {
    check(|id| scramble(id) as u16, u16::cmp);
    check(|id| scramble(id) as i32 >> 20, i32::cmp);
    check(|id| Reverse(scramble(id) >> 40), Reverse::cmp);
    check(|id| scramble(id) as u128 * u64::MAX as u128, u128::cmp);
    check(
        |id| match id % 5 {
            0 => f64::NAN,
            1 => -0.0,
            2 => 0.0,
            _ => scramble(id) as f64 - 1e18,
        },
        f64::total_cmp,
    );
}

#[test]
fn ordered_and_constant_keys_sort_with_one_call_per_record() {
    check(|_| 7u32, u32::cmp);
    check(|id| id as u64, u64::cmp);
    check(|id| (id / 3) as u8, u8::cmp);
    check(|id| u64::MAX - id as u64, u64::cmp);
    check(|id| u64::MAX - (id / 3) as u64, u64::cmp);
    let len = lengths().last().copied().unwrap_or_default();
    check(
        move |id| {
            if id == 0 {
                u64::MAX
            } else if id + 1 == len {
                0
            } else {
                1u64 << 63
            }
        },
        u64::cmp,
    );
    check(move |id| if id < len / 2 { u64::MAX } else { 0 }, u64::cmp);
    // Ordered except for the last record.
    check(
        move |id| if id + 1 == len { 0 } else { id as u32 + 1 },
        u32::cmp,
    );
}

#[test]
fn zero_sized_records_do_not_call_the_key() {
    let mut records = [(); 65];
    let mut calls = 0;
    sort_by_cached_key(&mut records, |_| {
        calls += 1;
        0u32
    });
    assert_eq!(calls, 0);
}

#[test]
fn key_unwind_keeps_records_and_interior_edits() {
    let len = if cfg!(miri) { 65 } else { 8193 };
    let mut values: Vec<_> = (0..len).collect();
    for failure in [1, len / 2, len] {
        let mut records: Vec<_> = values
            .iter_mut()
            .map(|v| (v.to_string(), Cell::new(false), v))
            .collect();
        let mut visited = Vec::new();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            sort_by_cached_key(&mut records, |r| {
                visited.push(*r.2);
                r.1.set(true);
                if visited.len() == failure {
                    std::panic::panic_any(*r.2);
                }
                Reverse(r.0.len())
            })
        }));
        assert_eq!(
            *outcome.unwrap_err().downcast::<usize>().unwrap(),
            failure - 1
        );
        assert!(visited.into_iter().eq(0..failure));
        records.sort_unstable_by_key(|r| *r.2);
        for (id, (text, touched, value)) in records.iter_mut().enumerate() {
            assert_eq!(*text, id.to_string());
            assert_eq!(**value, id);
            assert_eq!(touched.get(), id < failure);
        }
    }
}
