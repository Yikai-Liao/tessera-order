// SPDX-License-Identifier: BSD-2-Clause

//! Stable category distribution, classifier and ownership contracts.

use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};
use tessera_order::{MAX_PARTITION_BUCKETS, stable_partition, stable_partition_by_bucket};

#[test]
fn categories_preserve_stable_payloads_and_complete_offsets() {
    let lengths: &[usize] = if cfg!(miri) {
        &[0, 1, 2, 65]
    } else {
        &[0, 1, 2, 48, 129, 8193]
    };
    let counts: &[usize] = if cfg!(miri) {
        &[1, 2, 3, 7]
    } else {
        &[
            1,
            2,
            3,
            7,
            31,
            MAX_PARTITION_BUCKETS - 1,
            MAX_PARTITION_BUCKETS,
        ]
    };
    for &len in lengths {
        for &buckets in counts {
            let mut records: Vec<_> = (0..len).map(|id| (id, id.to_string())).collect();
            let category = |id: usize| id.wrapping_mul(251) % buckets;
            let mut expected = records.clone();
            expected.sort_by_key(|record| category(record.0));
            let mut visited = Vec::new();
            let offsets = stable_partition_by_bucket(&mut records, buckets, |record| {
                visited.push(record.0);
                category(record.0)
            });
            assert_eq!(visited, (0..len).collect::<Vec<_>>());
            assert_eq!(records, expected);
            assert_eq!(offsets.len(), buckets + 1);
            assert_eq!(offsets[0], 0);
            assert_eq!(offsets[buckets], len);
            for bucket in 0..buckets {
                assert!(offsets[bucket] <= offsets[bucket + 1]);
                assert!(
                    records[offsets[bucket]..offsets[bucket + 1]]
                        .iter()
                        .all(|r| category(r.0) == bucket)
                );
            }
        }
    }
}

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
            let cut = stable_partition(&mut records, |record| {
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
fn zero_sized_and_single_category_calls_still_evaluate_and_validate() {
    let mut records = [(); 65];
    let mut calls = 0;
    let offsets = stable_partition_by_bucket(&mut records, 3, |_| {
        let bucket = calls % 3;
        calls += 1;
        bucket
    });
    assert_eq!(calls, records.len());
    assert_eq!(offsets, [0, 22, 44, 65]);
    calls = 0;
    assert_eq!(
        stable_partition(&mut records, |_| {
            calls += 1;
            calls % 2 == 0
        }),
        32
    );
    assert_eq!(calls, records.len());
    assert!(
        catch_unwind(AssertUnwindSafe(|| stable_partition_by_bucket(
            &mut records,
            1,
            |_| 1
        )))
        .is_err()
    );
    let mut values = [3, 1, 2];
    calls = 0;
    assert_eq!(
        stable_partition_by_bucket(&mut values, 1, |_| {
            calls += 1;
            0
        }),
        [0, 3]
    );
    assert_eq!(calls, 3);
    assert_eq!(values, [3, 1, 2]);
}

#[test]
fn invalid_counts_are_rejected_before_calls_or_mutation() {
    for count in [0, MAX_PARTITION_BUCKETS + 1, usize::MAX] {
        let mut records = [3, 1, 2];
        let mut calls = 0;
        assert!(
            catch_unwind(AssertUnwindSafe(|| stable_partition_by_bucket(
                &mut records,
                count,
                |_| {
                    calls += 1;
                    0
                }
            )))
            .is_err()
        );
        assert_eq!(calls, 0);
        assert_eq!(records, [3, 1, 2]);
    }
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
            for invalid in [false, true] {
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
                    stable_partition_by_bucket(&mut records, 3, |r| {
                        assert_eq!(r.id, calls);
                        calls += 1;
                        if calls == failure {
                            assert!(invalid, "classification failed");
                            return usize::MAX;
                        }
                        r.id % 3
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
        catch_unwind(AssertUnwindSafe(|| stable_partition(&mut records, |_| {
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
