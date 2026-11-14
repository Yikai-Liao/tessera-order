// SPDX-License-Identifier: BSD-2-Clause

//! Cached-key sorting: order, stability, key evaluation and ownership.

use std::{
    cell::Cell,
    cmp::{Ordering, Reverse},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
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
    // Ordered except for the last record.
    let len = lengths().last().copied().unwrap_or_default();
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
fn key_failures_restore_owned_and_borrowed_records() {
    struct Owned {
        id: usize,
        text: String,
        padding: [u8; 512],
        drops: Rc<Vec<Cell<usize>>>,
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            assert_eq!(self.text, self.id.to_string());
            assert_eq!(self.padding, [self.id as u8; 512]);
            self.drops[self.id].set(self.drops[self.id].get() + 1);
        }
    }
    fn fail<K: RadixKey>(key: impl Fn(usize) -> K) {
        // 65 records fit a tile; 129 exceed its byte capacity and move through
        // blocks on the narrow route.
        for len in [65, 129] {
            for failure in [1, 33, len] {
                let drops = Rc::new((0..len).map(|_| Cell::new(0)).collect::<Vec<_>>());
                let mut records: Vec<_> = (0..len)
                    .map(|id| Owned {
                        id,
                        text: id.to_string(),
                        padding: [id as u8; 512],
                        drops: drops.clone(),
                    })
                    .collect();
                let mut calls = 0;
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    sort_by_cached_key(&mut records, |r| {
                        assert_eq!(r.id, calls);
                        calls += 1;
                        assert_ne!(calls, failure, "key failed");
                        key(r.id)
                    })
                }));
                assert!(outcome.is_err());
                assert_eq!(calls, failure, "key ran during recovery");
                assert!(drops.iter().all(|drop| drop.get() == 0));
                records.sort_by_key(|r| r.id);
                assert!(records.iter().enumerate().all(|(id, r)| id == r.id));
                drop(records);
                assert!(drops.iter().all(|drop| drop.get() == 1));
            }
        }
    }
    fail(|id| (id % 3) as u8);
    fail(|id| Reverse(id as u64));

    let mut values: Vec<_> = (0..129).collect();
    let mut records: Vec<_> = values.iter_mut().enumerate().collect();
    sort_by_cached_key(&mut records, |r| Reverse(r.0 as u16));
    for (rank, (original, value)) in records.iter_mut().enumerate() {
        assert_eq!(*original, 128 - rank);
        **value += 129;
    }
    drop(records);
    assert_eq!(values, (129..258).collect::<Vec<_>>());
}
