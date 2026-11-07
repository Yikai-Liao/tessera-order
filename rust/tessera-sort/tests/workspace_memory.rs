// SPDX-License-Identifier: BSD-2-Clause

//! Allocation counts of a reused `Workspace` and the accuracy of
//! `retained_bytes`, measured with a counting global allocator.

// Miri keeps its own allocator; the other suites cover record handling there.
#![cfg(not(miri))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use tessera_sort::Workspace;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
    static BYTES: Cell<isize> = const { Cell::new(0) };
}
struct Counting;
#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn record(bytes: isize, allocation: bool) {
    if ENABLED.try_with(Cell::get).unwrap_or(false) {
        BYTES.with(|total| total.set(total.get() + bytes));
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
    ALLOCS.with(|x| x.set(0));
    BYTES.with(|x| x.set(0));
    ENABLED.with(|x| x.set(true));
    f();
    ENABLED.with(|x| x.set(false));
    (ALLOCS.with(Cell::get), BYTES.with(Cell::get))
}

#[test]
fn warm_workspace_reuses_buffers() {
    let mut input: Vec<_> = (0..20000u64)
        .map(|id| (id.wrapping_mul(0x9e3779b97f4a7c15), id))
        .collect();
    let original = input.clone();
    let mut workspace = Workspace::new();
    assert_eq!(workspace.retained_bytes(), 0);
    let (_, bytes) = measure(|| workspace.stable_sort_by_key(&mut input, |x| x.0));
    assert_eq!(workspace.retained_bytes() as isize, bytes);
    input.clone_from_slice(&original);
    let (allocations, bytes) = measure(|| workspace.stable_sort_by_key(&mut input, |x| x.0));
    assert_eq!((allocations, bytes), (0, 0));
    let retained = workspace.retained_bytes();
    let (_, freed) = measure(|| workspace.shrink_to_fit());
    assert_eq!(freed, -(retained as isize));
    assert_eq!(workspace.retained_bytes(), 0);
}
