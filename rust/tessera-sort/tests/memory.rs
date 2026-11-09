// SPDX-License-Identifier: BSD-2-Clause

//! Per-call scratch allocation and release, measured with a counting allocator.

// Miri keeps its own allocator; the other suites cover record handling there.
#![cfg(not(miri))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use tessera_sort::{stable_sort, stable_sort_by_key};

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
    // A sorted body followed by a run of keys above it is already in bucket
    // order at the top bit, so splitting there needs no block partition. The
    // wide digit that pass would choose reserves 2 * 2^12 blocks of records,
    // which is 8 MiB for these keys.
    let len = 65_536usize;
    let tail = len / 128;
    let mut keys: Vec<u64> = (0..(len - tail) as u64).collect();
    keys.extend((0..tail as u64).rev().map(|id| 1 << 63 | id));
    let ((allocations, net), peak) = measure_peak(|| stable_sort(&mut keys));
    assert!(keys.is_sorted());
    assert!(allocations > 0);
    assert_eq!(net, 0);
    assert!(peak <= 128 * 1024, "peak scratch {peak}");
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
