// SPDX-License-Identifier: BSD-2-Clause
use std::sync::atomic::AtomicUsize;
#[cfg(feature = "memory")]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::Ordering,
};
#[cfg(feature = "memory")]
pub struct Count;
pub static LIVE: AtomicUsize = AtomicUsize::new(0);
pub static PEAK: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "memory")]
fn add(n: usize) {
    let x = LIVE.fetch_add(n, Ordering::Relaxed) + n;
    PEAK.fetch_max(x, Ordering::Relaxed);
}
#[cfg(feature = "memory")]
// SAFETY: Every allocation operation forwards its original pointer and layout
// to System. Accounting uses only nonallocating atomics and preserves failed
// allocation/reallocation behavior; it never changes the returned allocation.
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // SAFETY: The caller supplies the valid layout required by GlobalAlloc.
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            add(l.size())
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        // SAFETY: The caller supplies a live allocation and its original layout;
        // alloc/realloc above obtain all allocations from the same System owner.
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        // SAFETY: The caller supplies a live allocation, its original layout and
        // a valid new size. Passing them through preserves System's contract.
        let q = unsafe { System.realloc(p, l, n) };
        if !q.is_null() {
            if n >= l.size() {
                add(n - l.size())
            } else {
                LIVE.fetch_sub(l.size() - n, Ordering::Relaxed);
            }
        }
        q
    }
}
#[cfg(feature = "memory")]
#[global_allocator]
static ALLOC: Count = Count;
