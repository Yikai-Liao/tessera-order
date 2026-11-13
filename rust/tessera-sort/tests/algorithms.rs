// SPDX-License-Identifier: BSD-2-Clause

//! Public slice-operation contracts and adapter validation.

use std::{
    cell::Cell,
    cmp::Reverse,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};
use tessera_sort::*;

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
    for k in [0, 1, len / 3, len] {
        let mut values = original.clone();
        let (selected, remaining) = stable_top_k_by_key(&mut values, k, |r| r.0);
        assert_eq!((selected.len(), remaining.len()), (k, len - k));
        ranked(&values, &original, std::slice::from_ref(&(k..k)));
    }
    for prefix in [0, 1, len / 3, len] {
        let mut values = original.clone();
        stable_sort_prefix_by_key(&mut values, prefix, |r| r.0);
        let ranks = 0..prefix;
        let ranges = if prefix == 0 {
            &[][..]
        } else {
            std::slice::from_ref(&ranks)
        };
        ranked(&values, &original, ranges);
    }
    let mut values = original.clone();
    let index = len / 2;
    let (before, selected, after) = stable_select_nth_by_key(&mut values, index, |r| r.0);
    assert_eq!((before.len(), after.len()), (index, len - index - 1));
    let _ = selected;
    ranked(
        &values,
        &original,
        std::slice::from_ref(&(index..index + 1)),
    );
    let ranks = len / 5..len * 4 / 5;
    stable_sort_rank_range_by_key(&mut values, ranks.clone(), |r| r.0);
    ranked(&values, &original, &[ranks]);
    // Unordered duplicates and adjacent indices exercise one-time normalization.
    let indices = [len - 1, 1, index, 0, index + 1, 1, index];
    let mut values = original.clone();
    stable_select_nth_many_by_key(&mut values, &indices, |r| r.0);
    ranked(&values, &original, &[0..2, index..index + 2, len - 1..len]);
}

#[test]
fn argsort_keeps_source_and_float_bits() {
    let source = [
        (2u64, String::from("first")),
        (1, String::from("middle")),
        (2, String::from("last")),
    ];
    let indices = stable_argsort_by_key(&source, |r| r.0);
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
    assert_eq!(stable_argsort(&floats.map(Reverse)), expected);
    assert_eq!(floats.map(f64::to_bits), bits);
    assert!(stable_argsort::<u64>(&[]).is_empty());
    let failed = catch_unwind(AssertUnwindSafe(|| {
        stable_argsort_by_key(&source, |_| -> u64 {
            panic!("key");
        })
    }));
    assert!(failed.is_err());
    assert_eq!(source[2].1, "last");
}

#[test]
fn segments_keep_boundaries_empty_segments_and_fnmut() {
    let mut values: Vec<_> = (0..1000).map(|i| (((i * 137) % 19) as u64, i)).collect();
    let mut expected = values.clone();
    let offsets = [0, 0, 49, 49, 600, 1000];
    for ends in offsets.windows(2) {
        expected[ends[0]..ends[1]].sort_by_key(|r| r.0);
    }
    let calls = Rc::new(Cell::new(0)); // Serial extensions must not require Send or Sync.
    stable_sort_segments_by_key(&mut values, &offsets, |r| {
        calls.set(calls.get() + 1);
        r.0
    });
    assert_eq!(values, expected);
    assert!(calls.get() > 0);
    stable_sort_segments::<u64>(&mut [], &[0]);
    let mut direct = [3u64, 1, 9, 7, 8];
    stable_sort_segments(&mut direct, &[0, 2, 5]);
    assert_eq!(direct, [1, 3, 7, 8, 9]);
}

#[test]
fn invalid_arguments_and_empty_requests_do_not_evaluate_keys_or_mutate() {
    let original = [4u64, 1, 3, 2];
    for operation in 0..7 {
        let mut values = original;
        let calls = Cell::new(0);
        let mut key = |r: &u64| {
            calls.set(calls.get() + 1);
            *r
        };
        let result = catch_unwind(AssertUnwindSafe(|| match operation {
            0 => stable_sort_prefix_by_key(&mut values, 5, &mut key),
            1 => stable_sort_rank_range_by_key(&mut values, 5..5, &mut key),
            2 => stable_sort_rank_range_by_key(
                &mut values,
                std::ops::Range { start: 3, end: 2 },
                &mut key,
            ),
            3 => {
                stable_select_nth_by_key(&mut values, 4, &mut key);
            }
            4 => stable_select_nth_many_by_key(&mut values, &[0, 4, 1], &mut key),
            5 => {
                stable_top_k_by_key(&mut values, 5, &mut key);
            }
            _ => stable_sort_segments_by_key(&mut values, &[0, 3, 2, 4], &mut key),
        }));
        assert!(result.is_err());
        assert_eq!(calls.get(), 0);
        assert_eq!(values, original);
    }
    let mut values = original;
    let key = |_: &u64| -> u64 { panic!("empty request evaluated a key") };
    stable_top_k_by_key(&mut values, 0, key);
    stable_top_k_by_key(&mut values, original.len(), key);
    stable_sort_prefix_by_key(&mut values, 0, key);
    stable_sort_rank_range_by_key(&mut values, 2..2, key);
    stable_select_nth_many_by_key(&mut values, &[], key);
    assert_eq!(values, original);
}

#[test]
fn borrowed_records_and_segment_panics_retain_every_owner() {
    let mut source: Vec<_> = (0..97)
        .map(|i| (((i * 29) % 11) as u64, Box::new(i)))
        .collect();
    let mut records: Vec<_> = source.iter_mut().collect();
    stable_select_nth_many_by_key(&mut records, &[70, 10, 50], |r| r.0);
    let mut ids: Vec<_> = records.iter().map(|r| *r.1).collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..97).collect::<Vec<_>>());
    drop(records);
    let failed = catch_unwind(AssertUnwindSafe(|| {
        stable_sort_segments_by_key(&mut source, &[0, 31, 65, 97], |r| {
            if *r.1 == 80 {
                panic!("later segment");
            }
            r.0
        });
    }));
    assert!(failed.is_err());
    let mut ids: Vec<_> = source.iter().map(|r| *r.1).collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..97).collect::<Vec<_>>());
    assert!(source[..31].windows(2).all(|w| w[0].0 <= w[1].0));
}

#[test]
fn direct_entries_follow_encoded_rank_order() {
    let original = [u128::MAX, 0, 1 << 100, 1, (1 << 100) + 1, 0];
    let mut sorted = original;
    sorted.sort();
    let mut values = original;
    let (smallest, _) = stable_top_k(&mut values, 3);
    smallest.sort();
    assert_eq!(smallest, &sorted[..3]);
    let mut values = original;
    stable_sort_prefix(&mut values, 3);
    assert_eq!(values[..3], sorted[..3]);
    stable_sort_rank_range(&mut values, 2..5);
    assert_eq!(values[2..5], sorted[2..5]);
    assert_eq!(*stable_select_nth(&mut values, 4).1, sorted[4]);
    stable_select_nth_many(&mut values, &[0, 2, 5]);
    for index in [0, 2, 5] {
        assert_eq!(values[index], sorted[index]);
    }
}
