// SPDX-License-Identifier: BSD-2-Clause

//! Per-call scratch allocation and release, measured with a counting allocator.

// Miri keeps its own allocator; the other suites cover record handling there.
#![cfg(not(miri))]

use tessera_sort::{stable_sort, stable_sort_by_key};

#[path = "support/allocations.rs"]
mod allocations;
use allocations::measure_peak;

fn measure(f: impl FnOnce()) -> (usize, isize) {
    measure_peak(f).0
}

#[test]
fn scratch_released_after_sort() {
    let mut input: Vec<_> = (0..20000u64)
        .map(|id| (id.wrapping_mul(0x9e3779b97f4a7c15), id))
        .collect();
    let (allocations, bytes) = measure(|| stable_sort_by_key(&mut input, |x| x.0));
    assert!(allocations > 0);
    assert_eq!(bytes, 0);
    assert!(input.is_sorted_by_key(|x| x.0));

    let mut keys: Vec<_> = (0..20000u64)
        .map(|id| id.wrapping_mul(0x9e3779b97f4a7c15))
        .collect();
    let (allocations, bytes) = measure(|| stable_sort(&mut keys));
    assert!(allocations > 0);
    assert_eq!(bytes, 0);
    assert!(keys.is_sorted());
}

#[test]
fn bucket_ordered_input_keeps_scratch_in_one_tile() {
    // A sorted body with a run of keys above it is already in bucket order at
    // the top bit, so splitting there needs no block partition. The run sits
    // either side of the body: behind it after a rotation. The wide digit the
    // partition would choose reserves 2 * 2^12 blocks of records, which is
    // 8 MiB for these keys.
    let len = 65_536usize;
    let run = len / 128;
    let above: Vec<u64> = (0..run as u64).rev().map(|id| 1 << 63 | id).collect();
    let body: Vec<u64> = (0..(len - run) as u64).collect();
    for keys in [
        body.iter().chain(&above).copied().collect::<Vec<_>>(),
        above.iter().chain(&body).copied().collect::<Vec<_>>(),
    ] {
        let mut keys = keys;
        let ((allocations, net), peak) = measure_peak(|| stable_sort(&mut keys));
        assert!(keys.is_sorted());
        assert!(allocations > 0);
        assert_eq!(net, 0);
        assert!(peak <= 128 * 1024, "peak scratch {peak}");
    }
}

#[test]
fn ordered_inputs_do_not_allocate() {
    for len in [0, 1, 48, 49, 8193, 20000] {
        for constant in [false, true] {
            let mut keys: Vec<_> = (0..len as u64)
                .map(|id| if constant { 7 } else { id })
                .collect();
            assert_eq!(measure(|| stable_sort(&mut keys)), (0, 0));
            let mut records: Vec<_> = keys.iter().copied().enumerate().collect();
            assert_eq!(
                measure(|| stable_sort_by_key(&mut records, |x| x.1)),
                (0, 0)
            );
        }
    }
}
