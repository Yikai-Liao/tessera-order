// SPDX-License-Identifier: BSD-2-Clause

//! Per-call scratch allocation and release, measured with a counting allocator.

// Miri keeps its own allocator; the other suites cover record handling there.
#![cfg(not(miri))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use tessera_order::{sort, sort_by_key};

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
    static BYTES: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
}
struct Counting;
#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn record(bytes: isize, allocation: bool) {
    if ENABLED.try_with(Cell::get).unwrap_or(false) {
        BYTES.with(|total| {
            let live = total.get() + bytes;
            total.set(live);
            PEAK.with(|peak| peak.set(peak.get().max(live)));
        });
        if allocation {
            ALLOCS.with(|total| total.set(total.get() + 1));
        }
    }
}

// SAFETY: all allocation operations forward the same pointer and Layout to
// System. Thread-local bookkeeping neither allocates nor changes storage.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies a valid allocation layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record(layout.size() as isize, true);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(-(layout.size() as isize), false);
        // SAFETY: ptr/layout came from this forwarding allocator.
        unsafe {
            System.dealloc(ptr, layout);
        }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: the original pointer/layout and requested size come from
        // GlobalAlloc's caller and are passed unchanged to System.
        let next = unsafe { System.realloc(ptr, layout, size) };
        if !next.is_null() {
            record(size as isize - layout.size() as isize, true);
        }
        next
    }
}

fn measure(f: impl FnOnce()) -> (usize, isize) {
    measure_peak(f).0
}

/// Measures allocation count, net bytes and the peak live size of one call.
fn measure_peak(f: impl FnOnce()) -> ((usize, isize), isize) {
    ALLOCS.with(|x| x.set(0));
    BYTES.with(|x| x.set(0));
    PEAK.with(|x| x.set(0));
    ENABLED.with(|x| x.set(true));
    f();
    ENABLED.with(|x| x.set(false));
    (
        (ALLOCS.with(Cell::get), BYTES.with(Cell::get)),
        PEAK.with(Cell::get),
    )
}

#[test]
fn scratch_released_after_sort() {
    let mut input: Vec<_> = (0..20000u64)
        .map(|id| (id.wrapping_mul(0x9e3779b97f4a7c15), id))
        .collect();
    let (allocations, bytes) = measure(|| sort_by_key(&mut input, |x| x.0));
    assert!(allocations > 0);
    assert_eq!(bytes, 0);
    assert!(input.is_sorted_by_key(|x| x.0));

    let mut keys: Vec<_> = (0..20000u64)
        .map(|id| id.wrapping_mul(0x9e3779b97f4a7c15))
        .collect();
    let (allocations, bytes) = measure(|| sort(&mut keys));
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
        let ((allocations, net), peak) = measure_peak(|| sort(&mut keys));
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
            assert_eq!(measure(|| sort(&mut keys)), (0, 0));
            let mut records: Vec<_> = keys.iter().copied().enumerate().collect();
            assert_eq!(measure(|| sort_by_key(&mut records, |x| x.1)), (0, 0));
        }
    }
}

#[test]
fn wide_serial_partition_avoids_per_bucket_free_cache_allocations() {
    // 4,096 distinct 12-bit categories trigger a full 4,096-bucket serial block
    // distribution; with one shared free stack, total heap allocations stay O(1)
    // rather than scaling with the 4,096 nonempty buckets.
    let len = 65_536usize;
    let mut keys: Vec<u64> = (0..len as u64)
        .map(|id| id.wrapping_mul(0x9e37_79b9_7f4a_7c15) & 0xfff)
        .collect();
    let (allocations, net) = measure(|| tessera_order::sort_by_cached_key(&mut keys, |&k| k));
    assert!(keys.is_sorted());
    assert_eq!(net, 0);
    assert!(
        allocations < 64,
        "expected O(1) allocations for 4096-bucket serial distribution, got {allocations}"
    );
}

#[test]
fn boundary_selection_does_not_allocate() {
    let len = 65_536usize;
    let mut constant: Vec<_> = (0..len).map(|id| (id, 7u64)).collect();
    let (allocations, net) = measure(|| {
        let (_, selected, _) = tessera_order::select_nth_by_key(&mut constant, 4, |r| r.1);
        assert_eq!(selected.0, 4);
    });
    assert_eq!((allocations, net), (0, 0));

    // Over-budget all-minimum prefix on 128-byte records: `rank + 1` exceeds
    // the sparse-order budget on both 32-bit and 64-bit targets, yet the zero
    // pivot skips scanning the suffix.
    let over_budget_rank = 4_000usize;
    assert_eq!(core::mem::size_of::<(u64, u64, [u8; 112])>(), 128);
    let mut wide: Vec<_> = (0..len as u64)
        .map(|id| {
            let key = if id <= over_budget_rank as u64 {
                0
            } else {
                1 + (id % 97)
            };
            (id, key, [0u8; 112])
        })
        .collect();
    let (allocations, net) = measure(|| {
        let (_, selected, _) = tessera_order::select_nth_by_key(&mut wide, over_budget_rank, |r| {
            assert!(
                r.0 <= over_budget_rank as u64,
                "all-minimum prefix scanned suffix at {}",
                r.0
            );
            r.1
        });
        assert_eq!(selected.0, over_budget_rank as u64);
    });
    assert_eq!((allocations, net), (0, 0));
}

#[test]
fn one_sided_cut_reads_only_prefix_and_outlier() {
    let len = 65_536usize;
    let mut skewed: Vec<_> = (0..len)
        .map(|id| (id, if id == 2 { 7u64 } else { 0 }))
        .collect();
    let ((_, net), peak) = measure_peak(|| {
        let (before, selected, after) = tessera_order::select_nth_by_key(&mut skewed, 4, |r| {
            assert!(r.0 <= 5, "one-sided cut scanned past outlier at {}", r.0);
            r.1
        });
        assert_eq!(selected.0, 5);
        assert!(before.iter().map(|r| r.0).eq([0, 1, 3, 4]));
        assert_eq!(after.len(), len - 5);
        assert_eq!(after.iter().filter(|r| *r == &(2, 7)).count(), 1);
        assert!(after.iter().filter(|r| r.1 == 0).map(|r| r.0).eq(6..len));
    });
    assert_eq!(net, 0);
    assert!(peak <= 1024, "peak scratch {peak}");
}
