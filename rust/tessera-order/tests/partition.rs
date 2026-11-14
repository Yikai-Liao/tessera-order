// SPDX-License-Identifier: BSD-2-Clause

//! Stable binary partition and ownership contracts.

use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};
use tessera_order::partition;

#[test]
fn binary_partition_keeps_both_input_orders_and_returns_the_cut() {
    let lengths: &[usize] = if cfg!(miri) {
        &[0, 1, 2, 65]
    } else {
        &[0, 1, 2, 49, 8193, 10000]
    };
    for &len in lengths {
        for mode in 0..3 {
            let input: Vec<_> = (0..len).map(|id| (id, Box::new(id))).collect();
            let matching = |id: usize| match mode {
                0 => false,
                1 => true,
                _ => !id.is_multiple_of(3),
            };
            let mut expected = input.clone();
            expected.sort_by_key(|record| !matching(record.0));
            let expected_cut = input.iter().filter(|r| matching(r.0)).count();
            let mut records = input;
            let mut visited = Vec::new();
            let cut = partition(&mut records, |record| {
                visited.push(record.0);
                matching(record.0)
            });
            assert_eq!(cut, expected_cut);
            assert_eq!(records, expected);
            assert_eq!(visited, (0..len).collect::<Vec<_>>());
        }
    }
}

#[test]
fn zero_sized_records_still_evaluate_the_predicate() {
    let mut records = [(); 65];
    let mut calls = 0;
    assert_eq!(
        partition(&mut records, |_| {
            calls += 1;
            calls % 2 == 0
        }),
        32
    );
    assert_eq!(calls, records.len());
}

#[test]
fn tile_and_block_failures_restore_owned_and_borrowed_records() {
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
    // 65 records fit a tile; 129 exceed its byte capacity and scatter through
    // blocks. Both retain failures before/after a block boundary under Miri.
    for len in [65, 129] {
        for failure in [1, 32, 33, len / 2, len] {
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
                partition(&mut records, |r| {
                    assert_eq!(r.id, calls);
                    calls += 1;
                    assert_ne!(calls, failure, "predicate failed");
                    r.id % 3 == 0
                })
            }));
            assert!(outcome.is_err());
            assert_eq!(calls, failure, "classifier ran during recovery");
            assert!(drops.iter().all(|drop| drop.get() == 0));
            records.sort_by_key(|r| r.id);
            assert!(records.iter().enumerate().all(|(id, r)| id == r.id));
            drop(records);
            assert!(drops.iter().all(|drop| drop.get() == 1));
        }
    }
    let mut values: Vec<_> = (0..129).collect();
    // Padding forces borrowed records through blocks without a large Miri input.
    let mut records: Vec<_> = values
        .iter_mut()
        .enumerate()
        .map(|(id, value)| (id, value, [0u8; 512]))
        .collect();
    let mut calls = 0;
    assert!(
        catch_unwind(AssertUnwindSafe(|| partition(&mut records, |_| {
            calls += 1;
            assert_ne!(calls, 33);
            calls % 2 == 0
        })))
        .is_err()
    );
    records.sort_by_key(|r| r.0);
    for (id, (original, value, _padding)) in records.iter_mut().enumerate() {
        assert_eq!(*original, id);
        assert_eq!(**value, id);
        **value += 129;
    }
    drop(records);
    assert_eq!(values, (129..258).collect::<Vec<_>>());
}
