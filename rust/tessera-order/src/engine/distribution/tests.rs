// SPDX-License-Identifier: BSD-2-Clause

//! Recovery when a category callback unwinds during stable distribution.

use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

fn distribute<T>(values: &mut [T], mut predicate: impl FnMut(&T) -> bool) {
    super::distribute(values, 1, |record| u8::from(!predicate(record)));
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
                distribute(&mut records, |r| {
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
        catch_unwind(AssertUnwindSafe(|| distribute(&mut records, |_| {
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
