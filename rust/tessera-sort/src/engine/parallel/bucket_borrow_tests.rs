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
fn equality_ranks_keep_key_bounds_and_disable_filling() {
    let pivot = 1u128 << 64;
    let mut rank = EqualityRank {
        extractor: Direct,
        equal: pivot,
    };
    let mut records = [0u128, pivot - 1, pivot, pivot + 1, u128::MAX];
    assert_eq!(records.map(|record| rank.key(&record)), [0, 0, 1, 2, 2]);
    let original = records;
    assert!(!rank.try_count_window(&mut records, 0, 2));
    assert_eq!(records, original);
    for (pivot, ranks) in [(0, [1, 2]), (u128::MAX, [0, 1])] {
        let mut rank = EqualityRank {
            extractor: Direct,
            equal: pivot,
        };
        assert_eq!([0u128, u128::MAX].map(|record| rank.key(&record)), ranks);
    }
}

#[test]
fn equality_partition_keeps_exclusive_owned_records() {
    let mut records: Vec<_> = [7u64, 3, 1, 3, 9, 2, 3, 6]
        .into_iter()
        .enumerate()
        .map(|(id, key)| (Cell::new(key), Box::new(id)))
        .collect();
    let mut rank = EqualityRank {
        extractor: ByKey(&|row: &(Cell<u64>, Box<usize>)| row.0.get()),
        equal: 3,
    };
    let mut buffers = block::Buffers::new();
    let ends = block::partition_digit(&mut records, 2, 0, &mut rank, &mut buffers, &mut |_, _| {});
    assert_eq!(ends, [2, 5, 8, 8]);
    assert_eq!(
        records.iter().map(|row| *row.1).collect::<Vec<_>>(),
        [2, 5, 1, 3, 6, 0, 4, 7]
    );
    for row in records {
        assert_eq!(row.0.get(), [7, 3, 1, 3, 9, 2, 3, 6][*row.1]);
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
