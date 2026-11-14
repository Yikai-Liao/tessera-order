// SPDX-License-Identifier: BSD-2-Clause

//! Weak tie contracts still require exact ranked keys and every original payload.

use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
};
use tessera_order::*;

fn run<T>(values: &mut [T], operation: usize, mut key: impl FnMut(&T) -> u64) {
    let len = values.len();
    match operation {
        0 => {
            unstable_top_k_by_key(values, len / 3, &mut key);
        }
        1 => unstable_sort_prefix_by_key(values, len / 3, &mut key),
        2 => {
            unstable_select_nth_by_key(values, len / 2, &mut key);
        }
        3 => unstable_sort_rank_range_by_key(values, len / 4..len * 3 / 4, &mut key),
        _ => unstable_select_nth_many_by_key(values, &[len - 1, 0, len / 2, len / 2], key),
    }
}

#[test]
fn unstable_ranks_preserve_keys_and_payloads_across_nodes_and_tiles() {
    for len in if cfg!(miri) { [17, 97] } else { [37, 20_003] } {
        for distribution in 0..3 {
            let original: Vec<_> = (0..len)
                .map(|id| {
                    let key = (id as u64).wrapping_mul(0x9e3779b97f4a7c15);
                    let key = match distribution {
                        0 => key % 31,
                        1 => key,
                        // Broad variation and an equal band containing target cuts.
                        _ if id % 10 < 8 => 1 << 63,
                        _ if id % 10 == 8 => key >> 1,
                        _ => key | (1 << 63),
                    };
                    (key, id)
                })
                .collect();
            let mut reference = original.clone();
            reference.sort_by_key(|r| r.0);
            for operation in 0..5 {
                let mut values = original.clone();
                run(&mut values, operation, |r| r.0);
                let ranges = match operation {
                    0 => std::iter::once(len / 3..len / 3).collect(),
                    1 => std::iter::once(0..len / 3).collect(),
                    2 => std::iter::once(len / 2..len / 2 + 1).collect(),
                    3 => std::iter::once(len / 4..len * 3 / 4).collect(),
                    _ => vec![0..1, len / 2..len / 2 + 1, len - 1..len],
                };
                let mut start = 0;
                for range in ranges {
                    let mut gap: Vec<_> = values[start..range.start].iter().map(|r| r.0).collect();
                    gap.sort_unstable();
                    assert!(
                        gap.iter()
                            .copied()
                            .eq(reference[start..range.start].iter().map(|r| r.0))
                    );
                    assert!(
                        values[range.clone()]
                            .iter()
                            .map(|r| r.0)
                            .eq(reference[range.clone()].iter().map(|r| r.0))
                    );
                    start = range.end;
                }
                let mut tail: Vec<_> = values[start..].iter().map(|r| r.0).collect();
                tail.sort_unstable();
                assert!(
                    tail.iter()
                        .copied()
                        .eq(reference[start..].iter().map(|r| r.0))
                );
                values.sort_unstable_by_key(|r| r.1);
                assert_eq!(values, original);
            }
        }
    }
}

#[test]
fn noops_and_invalid_arguments_do_not_evaluate_keys() {
    let original = [4u64, 1, 3, 2];
    for operation in 0..6 {
        let mut values = original;
        let calls = Cell::new(0);
        let mut key = |r: &u64| {
            calls.set(calls.get() + 1);
            *r
        };
        let failed = catch_unwind(AssertUnwindSafe(|| match operation {
            0 => {
                unstable_top_k_by_key(&mut values, 5, &mut key);
            }
            1 => unstable_sort_prefix_by_key(&mut values, 5, &mut key),
            2 => {
                unstable_select_nth_by_key(&mut values, 4, &mut key);
            }
            3 => unstable_sort_rank_range_by_key(&mut values, 5..5, &mut key),
            4 => unstable_sort_rank_range_by_key(
                &mut values,
                std::ops::Range { start: 3, end: 2 },
                &mut key,
            ),
            _ => unstable_select_nth_many_by_key(&mut values, &[0, 4, 1], &mut key),
        }));
        assert!(failed.is_err());
        assert_eq!(calls.get(), 0);
        assert_eq!(values, original);
    }
    let mut values = original;
    let fail = |_: &u64| -> u64 { panic!("no-op evaluated a key") };
    unstable_top_k_by_key(&mut values, 0, fail);
    unstable_top_k_by_key(&mut values, original.len(), fail);
    unstable_sort_prefix_by_key(&mut values, 0, fail);
    unstable_sort_rank_range_by_key(&mut values, 2..2, fail);
    unstable_select_nth_many_by_key(&mut values, &[], fail);
    assert_eq!(values, original);
}

#[test]
fn panic_and_changing_keys_preserve_mutable_record_owners() {
    for len in if cfg!(miri) { [17, 37] } else { [37, 9001] } {
        let mut source: Vec<_> = (0..len)
            .map(|id| ((id as u64).wrapping_mul(0x9e3779b97f4a7c15), Box::new(id)))
            .collect();
        for operation in 0..5 {
            let mut total = 0;
            let mut records: Vec<_> = source.iter_mut().collect();
            run(&mut records, operation, |r| {
                total += 1;
                r.0
            });
            drop(records);
            for changing in [false, true] {
                for step in 1..=4 {
                    let mut calls = 0;
                    let mut records: Vec<_> = source.iter_mut().collect();
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        run(&mut records, operation, |r| {
                            calls += 1;
                            if calls >= total * step / 4 {
                                if changing {
                                    return calls as u64 % 3;
                                }
                                panic!("key failed");
                            }
                            r.0
                        })
                    }));
                    // Inconsistent keys may return or panic; either outcome
                    // must restore every record and keep its exclusive borrow.
                    if !changing {
                        assert!(result.is_err());
                    }
                    records.sort_unstable_by_key(|r| *r.1);
                    assert!(records.iter().enumerate().all(|(id, r)| *r.1 == id));
                }
            }
        }
    }
}

#[test]
fn direct_entries_preserve_encoded_wide_and_float_order() {
    let original = [u128::MAX, 0, 1 << 100, 1, (1 << 100) + 1, 0];
    let mut expected = original;
    expected.sort_unstable();
    let mut values = original;
    let (smallest, _) = unstable_top_k(&mut values, 3);
    smallest.sort_unstable();
    assert_eq!(smallest, &expected[..3]);
    unstable_sort_prefix(&mut values, 3);
    assert_eq!(values[..3], expected[..3]);
    unstable_sort_rank_range(&mut values, 2..5);
    assert_eq!(values[2..5], expected[2..5]);
    assert_eq!(*unstable_select_nth(&mut values, 4).1, expected[4]);
    unstable_select_nth_many(&mut values, &[0, 2, 5]);
    for i in [0, 2, 5] {
        assert_eq!(values[i], expected[i]);
    }
    let mut rows: Vec<_> = original
        .into_iter()
        .enumerate()
        .map(|(id, key)| (key, id))
        .collect();
    unstable_sort_prefix_by_key(&mut rows, original.len(), |r| r.0);
    assert!(rows.iter().map(|r| r.0).eq(expected));
    rows.sort_unstable_by_key(|r| r.1);
    assert!(rows.iter().map(|r| r.0).eq(original));
    let floats = [f64::NAN, -0.0, 0.0, f64::NEG_INFINITY, -f64::NAN];
    let mut expected = floats;
    expected.sort_unstable_by(f64::total_cmp);
    let mut actual = floats;
    unstable_sort_prefix(&mut actual, 3);
    assert_eq!(
        actual[..3].iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        expected[..3]
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>()
    );
}
