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
    let ends = block::partition_digit_with_ends(
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
    job.run::<E, true>(extractor, 3, TILE_BYTES, &pool);
    scratch.block.recycle_ends(ends);
}

#[test]
fn bucket_slices_keep_provenance() {
    let mut records = [3_u64, 2, 1, 0, 7, 6, 5, 4];
    sort_group(&mut records, Direct);
    assert_eq!(records, [0, 1, 2, 3, 4, 5, 6, 7]);
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
