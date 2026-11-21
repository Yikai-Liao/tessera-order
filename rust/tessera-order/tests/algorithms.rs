// SPDX-License-Identifier: BSD-2-Clause

//! Public slice-operation contracts and adapter validation.

use std::{
    cell::Cell,
    cmp::Reverse,
    panic::{AssertUnwindSafe, catch_unwind},
};
use tessera_order::*;

#[test]
fn selection_preserves_stable_members_across_node_and_tile_boundaries() {
    let len = if cfg!(miri) { 97 } else { 20_003 };
    let mut values: Vec<_> = (0..len)
        .map(|i| (((i * 7919 + 17) % 31) as u64, i))
        .collect();
    let mut expected = values.clone();
    expected.sort_by_key(|r| r.0);
    let index = len / 2;
    let (before, selected, after) = select_nth_by_key(&mut values, index, |r| r.0);
    assert_eq!(*selected, expected[index]);
    // Stable sorting each side must recover the original stable reference.
    before.sort_by_key(|r| r.0);
    after.sort_by_key(|r| r.0);
    assert_eq!(before, &expected[..index]);
    assert_eq!(after, &expected[index + 1..]);
}

#[test]
fn selection_rejects_out_of_bounds_and_empty_before_mutation_or_extraction() {
    let original = [2u32, 1, 3];
    for len in [0, 3] {
        for index in [len, usize::MAX] {
            let mut values = original;
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    select_nth(&mut values[..len], index);
                }))
                .is_err()
            );
            assert_eq!(values, original);

            let mut calls = 0;
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    select_nth_by_key(&mut values[..len], index, |key| {
                        calls += 1;
                        *key
                    });
                }))
                .is_err()
            );
            assert_eq!(calls, 0);
            assert_eq!(values, original);
        }
    }
}

#[test]
fn selection_returns_a_mutable_borrow_of_the_stable_rank() {
    let mut source = [2u32, 1, 2];
    let mut refs = source.iter_mut().enumerate().collect::<Vec<_>>();
    let (_, selected, _) = select_nth_by_key(&mut refs, 1, |r| *r.1);
    assert_eq!(selected.0, 0);
    *selected.1 = 9;
    assert_eq!(source, [9, 1, 2]);
}

#[test]
fn selection_unwind_keeps_all_records_and_the_panicking_callbacks_edit() {
    let len = if cfg!(miri) { 97 } else { 20_003 };
    let key = |id: usize| (id as u64).wrapping_mul(0x9e3779b97f4a7c15);
    let mut values: Vec<_> = (0..len).map(|id| (id, Cell::new(false))).collect();
    let mut calls = 0;
    select_nth_by_key(&mut values, len / 2, |(id, _)| {
        calls += 1;
        key(*id)
    });
    let fail_at = calls / 2;
    values.sort_unstable_by_key(|r| r.0);

    let mut calls = 0;
    let panic = catch_unwind(AssertUnwindSafe(|| {
        select_nth_by_key(&mut values, len / 2, |(id, edited)| {
            calls += 1;
            if calls == fail_at {
                edited.set(true);
                std::panic::panic_any(*id);
            }
            key(*id)
        });
    }))
    .unwrap_err();
    let edited_id = *panic.downcast::<usize>().unwrap();
    assert_eq!(calls, fail_at);
    values.sort_unstable_by_key(|r| r.0);
    for (id, record) in values.iter().enumerate() {
        assert_eq!(record.0, id);
        assert_eq!(record.1.get(), id == edited_id);
    }
}

#[test]
fn inconsistent_keys_preserve_records_and_any_returned_indices() {
    let len = 97;
    let mut values: Vec<_> = (0..len).map(|id| (id, Cell::new(false))).collect();
    let key = |(id, flip): &(usize, Cell<bool>)| {
        if flip.replace(!flip.get()) {
            *id
        } else {
            len - id
        }
    };
    if let Ok(mut indices) = catch_unwind(AssertUnwindSafe(|| argsort_by_key(&values, key))) {
        indices.sort_unstable();
        assert!(indices.into_iter().eq(0..len));
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        select_nth_by_key(&mut values, len / 2, key);
    }));
    values.sort_unstable_by_key(|r| r.0);
    assert!(values.iter().map(|r| r.0).eq(0..len));
}

#[test]
fn argsort_keeps_source_and_float_bits() {
    let source = [
        (2u64, String::from("first")),
        (1, String::from("middle")),
        (2, String::from("last")),
    ];
    let indices = argsort_by_key(&source, |r| r.0);
    assert_eq!(indices, [1, 0, 2]);
    assert_eq!(source[0].1, "first");
    let bits = [
        0x7ff8_0000_0000_0002u64,
        0,
        0x8000_0000_0000_0000,
        0xfff8_0000_0000_0001,
        0,
    ];
    let floats = bits.map(f64::from_bits);
    let mut expected: Vec<_> = (0..bits.len()).collect();
    expected.sort_by(|&a, &b| floats[b].total_cmp(&floats[a]));
    assert_eq!(argsort(&floats.map(Reverse)), expected);
    assert_eq!(floats.map(f64::to_bits), bits);
    assert!(argsort::<u64>(&[]).is_empty());
}

#[test]
#[cfg_attr(miri, ignore = "large input crosses the machine-word prefix boundary")]
fn argsort_refines_prefix_ties_and_preserves_complete_ties() {
    // Many source indices share the same high prefix. Their distinguishing
    // low bits cannot fit above the index in one word, even on a 64-bit host.
    let keys: Vec<_> = (0..16_393u64)
        .map(|i| {
            let mixed = i.wrapping_mul(0x9e37_79b9_7f4a_7c15);
            (u128::from(mixed >> 56) << 120)
                | (u128::from((mixed >> 55) & 1) << 64)
                | u128::from((mixed >> 31) & 7)
        })
        .collect();
    let mut expected: Vec<_> = (0..keys.len()).collect();
    expected.sort_by_key(|&i| keys[i]);
    assert_eq!(argsort(&keys), expected);
    let mut rows: Vec<_> = (0..keys.len()).collect();
    sort_by_cached_key(&mut rows, |&i| keys[i]);
    assert_eq!(rows, expected);
}

#[test]
fn argsort_unwind_keeps_source_positions_and_interior_edits() {
    let source = [3u32, 1, 2].map(|id| (Box::new(id), Cell::new(0)));
    let mut edits = [0; 4];
    let mut calls = 0;
    let panic = catch_unwind(AssertUnwindSafe(|| {
        argsort_by_key(&source, |(id, touched)| {
            edits[**id as usize] += 1;
            touched.set(touched.get() + 1);
            calls += 1;
            if calls == 2 {
                std::panic::panic_any(**id);
            }
            **id
        })
    }))
    .unwrap_err();
    assert!(panic.is::<u32>());
    assert_eq!(calls, 2);
    assert_eq!(source.each_ref().map(|r| *r.0), [3, 1, 2]);
    for (id, touched) in &source {
        assert_eq!(touched.get(), edits[**id as usize]);
    }
}
