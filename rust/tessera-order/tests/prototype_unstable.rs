// SPDX-License-Identifier: BSD-2-Clause

//! Temporary research coverage for the unstable movement boundary.

use std::panic::{AssertUnwindSafe, catch_unwind};
use tessera_order::prototype_unstable_sort_by_key as sort;

#[test]
fn unstable_keeps_borrowed_records_on_changing_keys_and_panics() {
    let len = 20000;
    let key = |id: usize| (id as u64).wrapping_mul(0x9e3779b97f4a7c15);
    let mut total = 0;
    let mut backing: Vec<_> = (0..len).collect();
    let mut records: Vec<_> = backing.iter_mut().enumerate().collect();
    sort(&mut records, |x| {
        total += 1;
        key(x.0)
    });
    assert!(records.windows(2).all(|p| key(p[0].0) <= key(p[1].0)));
    drop(records);
    for changing in [false, true] {
        for step in 1..=8 {
            let at = step * total / 8;
            let mut records: Vec<_> = backing.iter_mut().enumerate().collect();
            let mut calls = 0;
            let result = catch_unwind(AssertUnwindSafe(|| {
                sort(&mut records, |x| {
                    calls += 1;
                    if calls >= at {
                        if changing {
                            return calls as u64 % 3;
                        }
                        panic!("key failed");
                    }
                    key(x.0)
                });
            }));
            assert_eq!(result.is_ok(), changing);
            records.sort_unstable_by_key(|x| x.0);
            for (id, (identity, value)) in records.into_iter().enumerate() {
                assert_eq!(identity, id);
                assert_eq!(*value, id);
                *value = id;
            }
        }
    }
}
