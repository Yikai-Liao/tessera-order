// SPDX-License-Identifier: BSD-2-Clause

//! Private rank consumers exercise the production executor before public APIs.

use super::*;
use crate::engine::key::{ByKey, RadixKey};
use core::cell::Cell;
use core::ops::Range;

/// Required positions are exact; every intervening region has exact members.
pub(super) fn assert_members(actual: &[usize], expected: &[usize], intervals: &[Range<usize>]) {
    let mut start = 0;
    for range in intervals {
        let mut gap = actual[start..range.start].to_vec();
        let mut reference = expected[start..range.start].to_vec();
        gap.sort_unstable();
        reference.sort_unstable();
        assert_eq!(gap, reference);
        assert_eq!(actual[range.clone()], expected[range.clone()]);
        start = range.end;
    }
    let mut tail = actual[start..].to_vec();
    let mut reference = expected[start..].to_vec();
    tail.sort_unstable();
    reference.sort_unstable();
    assert_eq!(tail, reference);
}

fn check_keys<K: RadixKey>(keys: &[K]) {
    let len = keys.len();
    let mut expected: Vec<_> = (0..len).collect();
    expected.sort_by_key(|&id| keys[id].encode());
    let mut workspace = Workspace::new();
    let mut check = |intervals: &[Range<usize>]| {
        for tile_bytes in [128, TILE_BYTES] {
            let mut records: Vec<_> = keys.iter().copied().zip(0..len).collect();
            refine_with_tile_bytes(
                &mut records,
                ByKey(|record: &(K, usize)| record.0),
                Requirement::new(len, intervals),
                Some(&mut workspace),
                tile_bytes,
            );
            let ids: Vec<_> = records.iter().map(|record| record.1).collect();
            assert_members(&ids, &expected, intervals);
            // All equal-key members, including skipped ones, must remain in their
            // original relative order so later refinement needs no original index.
            records.sort_by_key(|record| record.0.encode());
            assert_eq!(
                records.iter().map(|record| record.1).collect::<Vec<_>>(),
                expected
            );
        }
    };
    check(&[]);
    for range in [
        0..len,
        0..len / 3,
        len / 3..2 * len / 3,
        len / 2..len / 2 + 1,
        0..0,
        len / 2..len / 2,
        len..len,
    ] {
        check(core::slice::from_ref(&range));
    }
    check(&[
        len / 4..len / 4,
        len / 2..len / 2 + 1,
        3 * len / 4..3 * len / 4,
    ]);
    for count in [7, 31] {
        let intervals: Vec<_> = (1..=count)
            .map(|i| {
                let rank = i * len / (count + 1);
                rank..rank + 1
            })
            .collect();
        check(&intervals);
    }
}

#[test]
fn partial_ranks_match_stable_record_identities() {
    let len = if cfg!(miri) { 65 } else { 513 };
    let keys: Vec<_> = (0..len)
        .map(|id| (id as u64 % 97).wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .collect();
    check_keys(&keys);
    check_keys(
        &keys
            .iter()
            .map(|&key| u128::from(key) << 64 | 17)
            .collect::<Vec<_>>(),
    );
    check_keys(
        &keys
            .iter()
            .map(|&key| 17 << 64 | u128::from(key))
            .collect::<Vec<_>>(),
    );
    check_keys(
        &keys
            .iter()
            .map(|&key| u128::from(key) << 64 | u128::from(key.rotate_left(31)))
            .collect::<Vec<_>>(),
    );
    check_keys(
        &keys
            .iter()
            .map(|&key| core::cmp::Reverse(key as i64))
            .collect::<Vec<_>>(),
    );
    check_keys(&keys.iter().map(|&key| key as u8).collect::<Vec<_>>());
    let floats: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(id, &key)| {
            f64::from_bits(match id % 8 {
                0 => 0,
                1 => 1 << 63,
                2 => 0x7ff8_0000_0000_0001,
                3 => 0xfff8_0000_0000_0001,
                _ => key,
            })
        })
        .collect();
    check_keys(&floats);
    let dominant: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(id, &key)| if id % 4 == 0 { key } else { 1 << 63 })
        .collect();
    check_keys(&dominant);
}

#[test]
fn empty_requirement_does_not_evaluate_keys() {
    let mut records = [3u64, 1, 2];
    refine(
        &mut records,
        ByKey(|_: &u64| -> u64 { panic!("empty request evaluated a key") }),
        Requirement::new(3, &[]),
        None,
    );
    assert_eq!(records, [3, 1, 2]);
}

#[test]
fn skipped_bands_receive_only_parent_classification() {
    let len = if cfg!(miri) { 97 } else { 193 };
    // Both cache-hot tile finishing and deferred large-child continuation must
    // prune before key analysis. Execute a real decision to isolate that rule
    // from root probes and the planner's advisory sampling.
    for tile_len in [2, len / 3 * 2] {
        let mut records: Vec<_> = (0..len)
            .map(|id| {
                let low = (id as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 1;
                let key = low | if id % 3 == 0 { 0 } else { 1 << 63 };
                (key, id, Cell::new(false))
            })
            .collect();
        let mut expected: Vec<_> = (0..len).collect();
        expected.sort_by_key(|&id| records[id].0);
        let ranks = 2..9;
        let intervals = core::slice::from_ref(&ranks);
        let mut extractor = ByKey(|record: &(u64, usize, Cell<bool>)| {
            let classified = record.2.replace(true);
            assert!(
                !classified || record.0 >> 63 == 0,
                "skipped band was analyzed"
            );
            record.0
        });
        Sorter::<_, _> {
            scratch: &mut Workspace::new(),
            extractor: &mut extractor,
            tile_len,
        }
        .execute_node(
            &mut records,
            u64::MAX as u128,
            NodePlan::Digit(Digit {
                shift: 63,
                width: 1,
            }),
            Requirement::new(len, intervals),
        );
        assert_members(
            &records.iter().map(|record| record.1).collect::<Vec<_>>(),
            &expected,
            intervals,
        );
    }
}

#[test]
fn equality_band_satisfies_multiple_ranks_without_analysis() {
    let len = if cfg!(miri) { 65 } else { 193 };
    let equal = 1u64 << 63;
    let mut records: Vec<_> = (0..len)
        .map(|id| {
            let key = match id % 5 {
                0 => id as u64,
                1 => equal + id as u64 + 1,
                _ => equal,
            };
            (key, id, Cell::new(false))
        })
        .collect();
    let mut expected: Vec<_> = (0..len).collect();
    expected.sort_by_key(|&id| records[id].0);
    let first = records.iter().filter(|record| record.0 < equal).count();
    let count = records.iter().filter(|record| record.0 == equal).count();
    let middle = first + count / 2;
    let last = first + count - 2;
    let intervals = [first + 1..first + 2, middle..middle + 1, last..last + 1];
    let mut extractor = ByKey(|record: &(u64, usize, Cell<bool>)| {
        assert!(
            !record.2.replace(true),
            "completed or unrequested band was analyzed"
        );
        record.0
    });
    Sorter::<_, _> {
        scratch: &mut Workspace::new(),
        extractor: &mut extractor,
        tile_len: 2,
    }
    .execute_node(
        &mut records,
        u64::MAX as u128,
        NodePlan::Equal(equal),
        Requirement::new(len, &intervals),
    );
    assert_members(
        &records.iter().map(|record| record.1).collect::<Vec<_>>(),
        &expected,
        &intervals,
    );
}

#[test]
fn rotated_runs_project_post_rotation_coordinates() {
    let len = if cfg!(miri) { 65 } else { 193 };
    let high_len = len / 3;
    let low_len = len - high_len;
    for ranks in [2..9, low_len - 2..low_len + 3] {
        let mut records: Vec<_> = (0..len)
            .map(|id| {
                let low = ((id % 19) as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 1;
                (low | if id < high_len { 1 << 63 } else { 0 }, id)
            })
            .collect();
        let mut expected: Vec<_> = (0..len).collect();
        expected.sort_by_key(|&id| records[id].0);
        let intervals = core::slice::from_ref(&ranks);
        let mut extractor = ByKey(|record: &(u64, usize)| {
            assert!(
                ranks.end > low_len || record.1 >= high_len,
                "unrequested rotated run was analyzed"
            );
            record.0
        });
        Sorter::<_, _> {
            scratch: &mut Workspace::new(),
            extractor: &mut extractor,
            tile_len: 2,
        }
        .execute_node(
            &mut records,
            u64::MAX as u128,
            NodePlan::Runs {
                boundary: high_len,
                set: true,
            },
            Requirement::new(len, intervals),
        );
        assert_members(
            &records.iter().map(|record| record.1).collect::<Vec<_>>(),
            &expected,
            intervals,
        );
        records.sort_by_key(|record| record.0);
        assert_eq!(
            records.iter().map(|record| record.1).collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn partial_child_failure_restores_records_and_releases_workspace() {
    let len = if cfg!(miri) { 97 } else { 193 };
    use std::rc::Rc;
    struct Owned {
        key: u64,
        id: usize,
        text: String,
        seen: Cell<bool>,
        drops: Rc<Vec<Cell<usize>>>,
    }
    impl Drop for Owned {
        fn drop(&mut self) {
            assert_eq!(self.text, self.id.to_string());
            self.drops[self.id].set(self.drops[self.id].get() + 1);
        }
    }
    for tile_len in [2, len / 3 * 2] {
        for changing in [false, true] {
            let drops = Rc::new((0..len).map(|_| Cell::new(0)).collect::<Vec<_>>());
            let mut records: Vec<_> = (0..len)
                .map(|id| {
                    let low = (id as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 1;
                    Owned {
                        key: low | if id % 3 == 0 { 0 } else { 1 << 63 },
                        id,
                        text: id.to_string(),
                        seen: Cell::new(false),
                        drops: drops.clone(),
                    }
                })
                .collect();
            let ranks = 2..9;
            let mut workspace = Workspace::new();
            let entered = Cell::new(false);
            let mut calls = 0u64;
            let outcome = crate::unwind::catch(|| {
                let mut extractor = ByKey(|record: &Owned| {
                    calls += 1;
                    if record.seen.replace(true) {
                        entered.set(true);
                        assert!(changing, "panic in requested child analysis");
                        return calls.wrapping_mul(0x9e37_79b9_7f4a_7c15);
                    }
                    record.key
                });
                Sorter::<_, _> {
                    scratch: &mut workspace,
                    extractor: &mut extractor,
                    tile_len,
                }
                .execute_node(
                    &mut records,
                    u64::MAX as u128,
                    NodePlan::Digit(Digit {
                        shift: 63,
                        width: 1,
                    }),
                    Requirement::new(len, core::slice::from_ref(&ranks)),
                );
            });
            assert!(entered.get(), "fixture never reached child analysis");
            assert_eq!(outcome.is_err(), !changing);
            assert!(drops.iter().all(|count| count.get() == 0));
            let mut ids: Vec<_> = records.iter().map(|record| record.id).collect();
            ids.sort_unstable();
            assert_eq!(ids, (0..len).collect::<Vec<_>>());

            // No movement state survives return or unwind. Reuse the same
            // capacity for an independent complete operation on these records.
            let mut expected: Vec<_> = records
                .iter()
                .map(|record| (record.key, record.id))
                .collect();
            expected.sort_by_key(|record| record.0);
            let all = 0..len;
            refine_with_tile_bytes(
                &mut records,
                ByKey(|record: &Owned| record.key),
                Requirement::new(len, core::slice::from_ref(&all)),
                Some(&mut workspace),
                128,
            );
            assert_eq!(
                records
                    .iter()
                    .map(|record| (record.key, record.id))
                    .collect::<Vec<_>>(),
                expected
            );
            drop(workspace);
            assert!(drops.iter().all(|count| count.get() == 0));
            drop(records);
            assert!(drops.iter().all(|count| count.get() == 1));
        }
    }
}
