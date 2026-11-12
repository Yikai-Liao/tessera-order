// SPDX-License-Identifier: BSD-2-Clause

//! Exercise scoped bucket handoff through the real partition callback.

use super::*;
use crate::key::ByKey;
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn sort_group<T: Send, E: Extract<T> + Clone + Send>(records: &mut [T], extractor: E) {
    let mut scratch = Scratch::new();
    let mut child_scratch = Scratch::new();
    let mut visit = |bucket: &mut [T], extractor: &mut E| {
        Sorter::<T, E> {
            scratch: &mut child_scratch,
            extractor,
            tile_len: tile_capacity::<T>(TILE_BYTES),
        }
        .sort_child(bucket, 3);
    };
    let ends = block::partition_digit(records, 1, 2, extractor, &mut scratch.block, &mut visit);
    scratch.block.recycle_ends(ends);
}

#[test]
#[cfg(not(miri))]
fn streamed_child_can_reenter_and_panic() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    // Cross the production shard-byte quota even though sort_root bypasses
    // the public entry point cutoff. A one-shard partition would fall back to
    // serial compaction and never exercise the streamed callback.
    let record_bytes = core::mem::size_of::<(u64, usize, &mut Cell<usize>)>();
    let len = (2 * MIN_TASK_BYTES).div_ceil(record_bytes) + 37;
    assert!(len * record_bytes / MIN_TASK_BYTES >= 2);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    for fail in [false, true] {
        let mut backing: Vec<_> = (0..len).map(Cell::new).collect();
        let mut records: Vec<_> = backing
            .iter_mut()
            .enumerate()
            .map(|(id, value)| ((id as u64).wrapping_mul(0x9e3779b97f4a7c15), id, value))
            .collect();
        let calls = AtomicUsize::new(0);
        let entered = std::sync::atomic::AtomicBool::new(false);
        let result = catch_unwind(AssertUnwindSafe(|| {
            pool.install(|| {
                let mut extractor = ByKey(|row: &(u64, usize, &mut Cell<usize>)| {
                    // sort_root skips preparation. For this input the bounded
                    // sample, early rejected two-run probe and root scatter
                    // together make fewer than 2 * len calls. Later calls are
                    // from a completed bucket's finisher, not root movement.
                    if calls.fetch_add(1, Ordering::Relaxed) > 2 * len
                        && !entered.swap(true, Ordering::Relaxed)
                    {
                        let (left, right) = rayon::join(|| 91, || 91);
                        assert_eq!(left, right);
                        assert!(!fail, "panic in streamed child after reentry");
                    }
                    row.0
                });
                // This is below staging eligibility; an ordinary digit root
                // must finish through the direct streamed callback.
                sort_root(
                    &mut records,
                    &mut extractor,
                    &mut Scratch::new(),
                    u128::from(u64::MAX),
                    8,
                );
            });
        }));
        assert!(entered.load(Ordering::Relaxed));
        assert_eq!(result.is_err(), fail);
        if !fail {
            assert!(records.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        }
        let mut ids = Vec::new();
        for (_, id, value) in records {
            assert_eq!(value.get(), id);
            value.set(id + 1);
            ids.push(id);
        }
        ids.sort_unstable();
        assert_eq!(ids, (0..len).collect::<Vec<_>>());
        assert!(
            backing
                .iter()
                .enumerate()
                .all(|(id, value)| value.get() == id + 1)
        );
    }
}

#[test]
#[cfg(not(miri))]
fn equality_roots_match_serial_policy() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    for equal in [0, 7, 1u128 << 64, u128::MAX] {
        let mut records: Vec<_> = (0..129)
            .map(|id| {
                let key = if id % 4 == 0 {
                    (id as u128).wrapping_mul(u128::MAX / 127)
                } else {
                    equal
                };
                (Cell::new(key), Box::new(id))
            })
            .collect();
        let mut serial = records.clone();
        let mut expected = records.clone();
        expected.sort_by_key(|row| row.0.get());
        let mut extractor = ByKey(|row: &(Cell<u128>, Box<usize>)| row.0.get());
        assert!(
            matches!(plan_node(&records, u128::MAX, 1, &mut extractor), NodePlan::Equal(pivot) if pivot == equal)
        );
        Sorter {
            scratch: &mut Scratch::new(),
            extractor: &mut extractor,
            tile_len: 1,
        }
        .partition_equal(&mut serial, equal, u128::MAX);
        pool.install(|| {
            sort_root(
                &mut records,
                &mut extractor,
                &mut Scratch::new(),
                u128::MAX,
                1,
            )
        });
        assert_eq!(records, serial);
        assert_eq!(records, expected);
    }
}

#[test]
fn bucket_slices_keep_provenance() {
    for mut records in [[3_u64, 2, 1, 0, 7, 6, 5, 4], [0, 1, 2, 3, 4, 5, 6, 7]] {
        sort_group(&mut records, Direct);
        assert_eq!(records, [0, 1, 2, 3, 4, 5, 6, 7]);
    }
}

#[test]
fn descending_children_keep_equal_key_order() {
    // Each bucket starts with an equal pair before descending. Both completion
    // routes must undo equal-run reversal, not just reverse the whole child.
    let input = [
        (3u64, 0),
        (3, 1),
        (2, 2),
        (0, 3),
        (7, 4),
        (7, 5),
        (6, 6),
        (4, 7),
    ];
    let mut expected = input;
    expected.sort_by_key(|row| row.0);
    let mut records = input;
    sort_group(&mut records, ByKey(&|row: &(u64, usize)| row.0));
    assert_eq!(records, expected);
    for band in input.chunks(4) {
        let mut records = band.to_vec();
        let mut expected = records.clone();
        expected.sort_by_key(|row| row.0);
        Sorter::<_, _> {
            scratch: &mut Scratch::new(),
            extractor: &mut ByKey(|row: &(u64, usize)| row.0),
            tile_len: 1,
        }
        .sort_staged_child(&mut records, 3);
        assert_eq!(records, expected);
    }
}

#[test]
fn owned_records_remain_stable() {
    struct Row {
        key: Cell<u64>,
        id: String,
        drops: Arc<AtomicUsize>,
    }
    impl Drop for Row {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::Relaxed);
        }
    }

    let drops = Arc::new(AtomicUsize::new(0));
    let mut records: Vec<_> = [3_u64, 2, 3, 0, 7, 6, 7, 4]
        .into_iter()
        .enumerate()
        .map(|(id, key)| Row {
            key: Cell::new(key),
            id: id.to_string(),
            drops: drops.clone(),
        })
        .collect();
    sort_group(&mut records, ByKey(&|row: &Row| row.key.get()));
    assert_eq!(
        records
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        ["3", "1", "0", "2", "7", "5", "4", "6"]
    );
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    drop(records);
    assert_eq!(drops.load(Ordering::Relaxed), 8);
}

#[test]
fn exclusive_borrows_stay_disjoint() {
    let mut storage = [3_u64, 2, 1, 0, 7, 6, 5, 4].map(Cell::new);
    let mut records: Vec<_> = storage.iter_mut().collect();
    sort_group(&mut records, ByKey(&|row: &&mut Cell<u64>| row.get()));
    assert_eq!(
        records.iter().map(|row| row.get()).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5, 6, 7]
    );
    for (index, record) in records.into_iter().enumerate() {
        record.set(index as u64 + 10);
    }
    assert_eq!(
        storage.map(Cell::into_inner),
        [13, 12, 11, 10, 17, 16, 15, 14]
    );
}
