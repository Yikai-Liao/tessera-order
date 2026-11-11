// SPDX-License-Identifier: BSD-2-Clause

//! Exercise scoped bucket handoff through the real partition callback.

use super::*;
use crate::key::ByKey;
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn sort_group<'records, T: Send, E: Extract<T> + Clone + Send>(
    records: &'records mut [T],
    mut extractor: E,
) {
    let mut scratch = Scratch::new();
    let pool = Pool::new();
    let mut pending: Option<Job<'records, T>> = None;
    let mut visit = |bucket: &'records mut [T], _: &mut E| {
        pending.get_or_insert_with(Job::new).push(bucket);
    };
    let ends = block::partition_digit(
        records,
        1,
        2,
        &mut extractor,
        &mut scratch.block,
        &mut visit,
    );
    drop(visit);

    let mut job = pending.expect("both output buckets are nonempty");
    assert_eq!(job.buckets.len(), 2);
    job.run::<E>(extractor, 3, tile_capacity::<T>(TILE_BYTES), &pool);
    scratch.block.recycle_ends(ends);
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
        .sort_child(&mut records, 3);
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
