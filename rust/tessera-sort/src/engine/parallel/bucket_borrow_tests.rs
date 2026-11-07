// SPDX-License-Identifier: BSD-2-Clause
//! Exercise deferred, coalesced jobs through the actual partition callback.
use super::*;
use crate::key::ByKey;
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn grouped<T: Send, E: Extract<T> + Clone + Send>(values: &mut [T], mut key: E) {
    let mut workspace = Workspace::new();
    let mut pending: Option<Job<T>> = None;
    let mut visit = |bucket: &mut [T], _: &mut E| {
        let mut job = pending.take().unwrap_or_else(Job::new);
        assert!(job.push(bucket.as_mut_ptr(), bucket.len(), core::mem::size_of::<T>()));
        pending = Some(job);
    };
    block::partition_digit_with_ends(
        values,
        1,
        2,
        &mut key,
        workspace.block_buffers(),
        &mut visit,
    );
    let mut job = pending.expect("two adjacent materialized buckets");
    assert_eq!(job.buckets.len(), 2);
    job.run::<E, true>(key, 3, TILE_BYTES, &Pool::new());
}

#[test]
fn adjacent_materialized_buckets_keep_separate_borrows() {
    let mut values = [3_u64, 2, 1, 0, 7, 6, 5, 4];
    grouped(&mut values, Direct);
    assert_eq!(values, [0, 1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn grouped_owned_cell_records_preserve_stability_and_drops() {
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
    let mut values: Vec<_> = [3_u64, 2, 3, 0, 7, 6, 7, 4]
        .into_iter()
        .enumerate()
        .map(|(id, key)| Row {
            key: Cell::new(key),
            id: id.to_string(),
            drops: drops.clone(),
        })
        .collect();
    grouped(&mut values, ByKey(&|row: &Row| row.key.get()));
    assert_eq!(
        values.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["3", "1", "0", "2", "7", "5", "4", "6"]
    );
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    drop(values);
    assert_eq!(drops.load(Ordering::Relaxed), 8);
}

#[test]
fn grouped_exclusive_cell_records_keep_individual_borrows() {
    let mut storage = [3_u64, 2, 1, 0, 7, 6, 5, 4].map(Cell::new);
    let mut values: Vec<_> = storage.iter_mut().collect();
    grouped(&mut values, ByKey(&|row: &&mut Cell<u64>| row.get()));
    assert_eq!(
        values.iter().map(|row| row.get()).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5, 6, 7]
    );
    for (id, row) in values.into_iter().enumerate() {
        row.set(id as u64 + 10);
    }
    assert_eq!(
        storage.map(Cell::into_inner),
        [13, 12, 11, 10, 17, 16, 15, 14]
    );
}
