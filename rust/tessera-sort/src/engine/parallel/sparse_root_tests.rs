// SPDX-License-Identifier: BSD-2-Clause

//! A widened root digit must run even when it leaves no child key bits.

use super::*;
use crate::key::ByKey;
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

const LEN: usize = 1025;
const TEST_TILE_BYTES: usize = 256;
const VARYING: u128 = 0x103;

fn sparse_key(id: usize) -> u64 {
    let k = (id * 5) % 8;
    ((k & 3) | ((k & 4) << 6)) as u64
}

/// Reproduce the pre-partition samples and assert that this fixture selects
/// a digit covering every varying bit, without the equality or outlier routes.
fn assert_whole_root_digit<T, E: Extract<T, Value = u64>>(values: &[T], key: &mut E) {
    let mut keys = [0; MODE_SAMPLE_LEN];
    sample_keys(values, &mut keys, key);
    assert!(sample_mode(&mut keys).is_none());
    assert!(outlier_run(values, &keys, 1 << 8, key).is_none());
    let digit = choose_split_digit(values, 9, TEST_TILE_BYTES, key);
    assert_eq!((digit.width, digit.shift), (9, 0));
    assert_eq!(VARYING & !digit.mask::<u64>().to_u128(), 0);
}

#[test]
fn widened_root_digit_sorts_direct_and_stable_owned_records() {
    let mut values: Vec<_> = (0..LEN).map(sparse_key).collect();
    let mut expected = values.clone();
    expected.sort();
    assert_whole_root_digit(&values, &mut Direct);
    sort_prepared::<_, _, false>(&mut values, Direct, VARYING, TEST_TILE_BYTES);
    assert_eq!(values, expected);

    struct Row {
        key: u64,
        id: usize,
        payload: String,
    }
    let mut rows: Vec<_> = (0..LEN)
        .map(|id| Row {
            key: sparse_key(id),
            id,
            payload: id.to_string(),
        })
        .collect();
    let mut expected: Vec<_> = rows.iter().map(|row| (row.key, row.id)).collect();
    expected.sort_by_key(|row| row.0);
    let key = ByKey(|row: &Row| row.key);
    assert_whole_root_digit(&rows, &mut key.clone());
    sort_prepared::<_, _, true>(&mut rows, key, VARYING, TEST_TILE_BYTES);
    assert_eq!(
        rows.iter().map(|row| (row.key, row.id)).collect::<Vec<_>>(),
        expected
    );
    assert!(rows.iter().all(|row| row.payload == row.id.to_string()));
}

#[test]
fn widened_root_scatter_panic_restores_owned_records_once() {
    struct Row {
        key: u64,
        id: usize,
        payload: String,
        drops: Arc<AtomicUsize>,
    }
    impl Drop for Row {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Relaxed);
        }
    }

    for scatter_offset in [1, LEN / 2, LEN] {
        let drops = Arc::new(AtomicUsize::new(0));
        let mut rows: Vec<_> = (0..LEN)
            .map(|id| Row {
                key: sparse_key(id),
                id,
                payload: id.to_string(),
                drops: drops.clone(),
            })
            .collect();
        let calls = Cell::new(0);
        assert_whole_root_digit(
            &rows,
            &mut ByKey(|row: &Row| {
                calls.set(calls.get() + 1);
                row.key
            }),
        );
        let panic_at = calls.get() + scatter_offset;
        let calls = AtomicUsize::new(0);
        let key = ByKey(|row: &Row| {
            let call = calls.fetch_add(1, Relaxed) + 1;
            if call == panic_at {
                std::panic::panic_any(0x76_u32);
            }
            row.key
        });
        let panic = catch_unwind(AssertUnwindSafe(|| {
            sort_prepared::<_, _, true>(&mut rows, key, VARYING, TEST_TILE_BYTES);
        }))
        .expect_err("the selected root digit must call the extractor during scatter");
        assert_eq!(*panic.downcast::<u32>().unwrap(), 0x76);
        assert_eq!(calls.load(Relaxed), panic_at);
        assert_eq!(drops.load(Relaxed), 0);
        let mut ids: Vec<_> = rows.iter().map(|row| row.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, (0..LEN).collect::<Vec<_>>());
        assert!(rows.iter().all(|row| row.payload == row.id.to_string()));
        drop(rows);
        assert_eq!(drops.load(Relaxed), LEN);
    }
}

#[test]
fn widened_root_preserves_exclusive_cell_borrows() {
    for panic_in_scatter in [false, true] {
        let mut storage: Vec<_> = (0..LEN).map(|id| Cell::new((sparse_key(id), id))).collect();
        let mut values: Vec<_> = storage.iter_mut().collect();
        let mut expected: Vec<_> = values.iter().map(|row| row.get()).collect();
        expected.sort_by_key(|row| row.0);
        let samples = Cell::new(0);
        assert_whole_root_digit(
            &values,
            &mut ByKey(|row: &&mut Cell<(u64, usize)>| {
                samples.set(samples.get() + 1);
                row.get().0
            }),
        );
        let panic_at = samples.get() + LEN / 2;
        let calls = AtomicUsize::new(0);
        let key = ByKey(|row: &&mut Cell<(u64, usize)>| {
            let call = calls.fetch_add(1, Relaxed) + 1;
            assert!(
                !(panic_in_scatter && call == panic_at),
                "borrowed root panic"
            );
            row.get().0
        });
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            sort_prepared::<_, _, true>(&mut values, key, VARYING, TEST_TILE_BYTES);
        }));
        assert_eq!(outcome.is_err(), panic_in_scatter);
        if panic_in_scatter {
            assert_eq!(calls.load(Relaxed), panic_at);
        } else {
            assert_eq!(
                values.iter().map(|row| row.get()).collect::<Vec<_>>(),
                expected
            );
        }
        let mut ids: Vec<_> = values.iter().map(|row| row.get().1).collect();
        ids.sort_unstable();
        assert_eq!(ids, (0..LEN).collect::<Vec<_>>());
        for row in values {
            let (key, id) = row.get();
            row.set((key + 1000, id));
        }
        assert!(
            storage
                .iter()
                .enumerate()
                .all(|(id, row)| row.get() == (sparse_key(id) + 1000, id))
        );
    }
}
