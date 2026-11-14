// SPDX-License-Identifier: BSD-2-Clause

//! Public slice-operation contracts and adapter validation.

use std::{
    cell::Cell,
    cmp::Reverse,
    ops::{Bound, Range},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};
use tessera_order::*;

type Bounds = (Bound<usize>, Bound<usize>);

fn ranked(values: &[(u64, usize)], original: &[(u64, usize)], ranges: &[std::ops::Range<usize>]) {
    let mut reference = original.to_vec();
    reference.sort_by_key(|r| r.0);
    let mut start = 0;
    for range in ranges {
        let mut gap = values[start..range.start].to_vec();
        gap.sort();
        assert_eq!(gap, reference[start..range.start]);
        assert_eq!(values[range.clone()], reference[range.clone()]);
        start = range.end;
    }
    let mut gap = values[start..].to_vec();
    gap.sort();
    assert_eq!(gap, reference[start..]);
}

#[test]
fn partial_calls_preserve_stable_members_across_node_and_tile_boundaries() {
    let len = if cfg!(miri) { 97 } else { 20_003 };
    let original: Vec<_> = (0..len)
        .map(|i| (((i * 7919 + 17) % 31) as u64, i))
        .collect();
    let mut sorted = original.clone();
    sorted.sort_by_key(|r| r.0);
    for k in [0, 1, len / 3, len] {
        let mut values = original.clone();
        let (selected, remaining) = k_smallest_by_key(&mut values, k, |r| r.0);
        assert_eq!((selected.len(), remaining.len()), (k, len - k));
        assert_eq!(*selected, sorted[..k]);
        ranked(&values, &original, std::slice::from_ref(&(k..k)));
        // Membership without order is an empty interior range.
        let mut values = original.clone();
        partial_sort_by_key(&mut values, k..k, |r| r.0);
        ranked(&values, &original, std::slice::from_ref(&(k..k)));
        let mut values = original.clone();
        partial_sort_by_key(&mut values, ..k, |r| r.0);
        ranked(&values, &original, std::slice::from_ref(&(0..k)));
        // Largest first, with equal keys in input order.
        let mut expected = original.clone();
        expected.sort_by_key(|r| Reverse(r.0));
        let mut values = original.clone();
        let (selected, remaining) = k_largest_by_key(&mut values, k, |r| r.0);
        assert_eq!((selected.len(), remaining.len()), (k, len - k));
        assert_eq!(*selected, expected[..k]);
    }
    let mut values = original.clone();
    let index = len / 2;
    let (before, selected, after) = select_nth_by_key(&mut values, index, |r| r.0);
    assert_eq!((before.len(), after.len()), (index, len - index - 1));
    let _ = selected;
    ranked(
        &values,
        &original,
        std::slice::from_ref(&(index..index + 1)),
    );
    let ranks = len / 5..len * 4 / 5;
    partial_sort_by_key(&mut values, ranks.clone(), |r| r.0);
    ranked(&values, &original, &[ranks]);
    // Unordered duplicates and adjacent indices exercise one-time normalization.
    let indices = [len - 1, 1, index, 0, index + 1, 1, index];
    let mut values = original.clone();
    select_nth_many_by_key(&mut values, &indices, |r| r.0);
    ranked(&values, &original, &[0..2, index..index + 2, len - 1..len]);
}

#[test]
fn partial_sort_accepts_every_range_form() {
    let original: Vec<_> = (0..64usize).map(|i| (((i * 37) % 11) as u64, i)).collect();
    let (a, b, k) = (10usize, 40usize, 25usize);
    let forms: [(Range<usize>, Bounds); 5] = [
        (0..k, (Bound::Unbounded, Bound::Excluded(k))),
        (a..b + 1, (Bound::Included(a), Bound::Included(b))),
        (a..64, (Bound::Included(a), Bound::Unbounded)),
        (0..64, (Bound::Unbounded, Bound::Unbounded)),
        (a + 1..b, (Bound::Excluded(a), Bound::Excluded(b))),
    ];
    for (ranks, bounds) in forms {
        let mut values = original.clone();
        partial_sort_by_key(&mut values, bounds, |r| r.0);
        ranked(&values, &original, &[ranks]);
    }
    // An empty range changes nothing at either end and partitions inside.
    for k in [0, 64] {
        let mut values = original.clone();
        partial_sort_by_key(&mut values, k..k, |_| -> u64 {
            panic!("no-op evaluated a key")
        });
        assert_eq!(values, original);
    }
    let mut values = original.clone();
    partial_sort_by_key(&mut values, k..k, |r| r.0);
    assert_ne!(values, original);
    ranked(&values, &original, std::slice::from_ref(&(k..k)));
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
    let failed = catch_unwind(AssertUnwindSafe(|| {
        argsort_by_key(&source, |_| -> u64 {
            panic!("key");
        })
    }));
    assert!(failed.is_err());
    assert_eq!(source[2].1, "last");
}

#[test]
fn chunks_keep_boundaries_and_fnmut() {
    let original: Vec<_> = (0..1000).map(|i| (((i * 137) % 19) as u64, i)).collect();
    // Length 1, a length that does not divide 1000, and lengths covering the slice.
    for chunk_len in [1, 7, 333, 1000, 5000] {
        let mut values = original.clone();
        let mut expected = original.clone();
        expected
            .chunks_mut(chunk_len)
            .for_each(|chunk| chunk.sort_by_key(|r| r.0));
        let calls = Rc::new(Cell::new(0)); // Serial extensions must not require Send or Sync.
        sort_chunks_by_key(&mut values, chunk_len, |r| {
            calls.set(calls.get() + 1);
            r.0
        });
        assert_eq!(values, expected);
        assert_eq!(calls.get() > 0, chunk_len > 1);
    }
    sort_chunks::<u64>(&mut [], 3);
    let mut direct = [3u64, 1, 9, 7, 8];
    sort_chunks(&mut direct, 2);
    assert_eq!(direct, [1, 3, 7, 9, 8]);
}

#[test]
fn invalid_arguments_and_empty_requests_do_not_evaluate_keys_or_mutate() {
    let original = [4u64, 1, 3, 2];
    for operation in 0..10 {
        let mut values = original;
        let calls = Cell::new(0);
        let mut key = |r: &u64| {
            calls.set(calls.get() + 1);
            *r
        };
        let result = catch_unwind(AssertUnwindSafe(|| match operation {
            0 => partial_sort_by_key(&mut values, ..5, &mut key),
            1 => partial_sort_by_key(&mut values, 5..5, &mut key),
            2 => partial_sort_by_key(&mut values, Range { start: 3, end: 2 }, &mut key),
            3 => {
                select_nth_by_key(&mut values, 4, &mut key);
            }
            4 => select_nth_many_by_key(&mut values, &[0, 4, 1], &mut key),
            5 => {
                k_smallest_by_key(&mut values, 5, &mut key);
            }
            6 => sort_chunks_by_key(&mut values, 0, &mut key),
            7 => {
                k_largest_by_key(&mut values, 5, &mut key);
            }
            8 => partial_sort_by_key(&mut values, 0..5, &mut key),
            _ => partial_sort_by_key(
                &mut values,
                (Bound::Excluded(usize::MAX), Bound::Unbounded),
                &mut key,
            ),
        }));
        assert!(result.is_err());
        assert_eq!(calls.get(), 0);
        assert_eq!(values, original);
    }
    let mut values = original;
    let key = |_: &u64| -> u64 { panic!("empty request evaluated a key") };
    k_smallest_by_key(&mut values, 0, key);
    k_largest_by_key(&mut values, 0, key);
    partial_sort_by_key(&mut values, ..0, key);
    partial_sort_by_key(&mut values, 0..0, key);
    partial_sort_by_key(&mut values, 4..4, key);
    select_nth_many_by_key(&mut values, &[], key);
    assert_eq!(values, original);
}

#[test]
fn borrowed_records_and_chunk_panics_retain_every_owner() {
    let mut source: Vec<_> = (0..97)
        .map(|i| (((i * 29) % 11) as u64, Box::new(i)))
        .collect();
    let mut records: Vec<_> = source.iter_mut().collect();
    select_nth_many_by_key(&mut records, &[70, 10, 50], |r| r.0);
    let mut ids: Vec<_> = records.iter().map(|r| *r.1).collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..97).collect::<Vec<_>>());
    drop(records);
    let failed = catch_unwind(AssertUnwindSafe(|| {
        sort_chunks_by_key(&mut source, 32, |r| {
            if *r.1 == 80 {
                panic!("later chunk");
            }
            r.0
        });
    }));
    assert!(failed.is_err());
    let mut ids: Vec<_> = source.iter().map(|r| *r.1).collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..97).collect::<Vec<_>>());
    assert!(source[..32].windows(2).all(|w| w[0].0 <= w[1].0));
}

#[test]
fn direct_entries_follow_encoded_rank_order() {
    let original = [u128::MAX, 0, 1 << 100, 1, (1 << 100) + 1, 0];
    let mut sorted = original;
    sorted.sort();
    let mut values = original;
    let (smallest, _) = k_smallest(&mut values, 3);
    assert_eq!(smallest, &sorted[..3]);
    let mut values = original;
    let (largest, _) = k_largest(&mut values, 3);
    assert_eq!(largest, [u128::MAX, (1 << 100) + 1, 1 << 100]);
    let mut values = original;
    partial_sort(&mut values, 3..3);
    values[..3].sort();
    assert_eq!(values[..3], sorted[..3]);
    partial_sort(&mut values, ..3);
    assert_eq!(values[..3], sorted[..3]);
    partial_sort(&mut values, 2..5);
    assert_eq!(values[2..5], sorted[2..5]);
    assert_eq!(*select_nth(&mut values, 4).1, sorted[4]);
    select_nth_many(&mut values, &[0, 2, 5]);
    for index in [0, 2, 5] {
        assert_eq!(values[index], sorted[index]);
    }
}
// reuse probe
