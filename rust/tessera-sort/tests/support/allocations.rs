// SPDX-License-Identifier: BSD-2-Clause

//! Calling-thread allocation accounting shared by engine and public tests.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

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

/// Measures allocation count, net bytes and the peak live size of one call.
pub(crate) fn measure_peak(f: impl FnOnce()) -> ((usize, isize), isize) {
    ALLOCS.with(|x| x.set(0));
    BYTES.with(|x| x.set(0));
    PEAK.with(|x| x.set(0));
    struct Disable;
    impl Drop for Disable {
        fn drop(&mut self) {
            ENABLED.with(|x| x.set(false));
        }
    }
    ENABLED.with(|x| x.set(true));
    let disable = Disable;
    f();
    drop(disable);
    (
        (ALLOCS.with(Cell::get), BYTES.with(Cell::get)),
        PEAK.with(Cell::get),
    )
}
