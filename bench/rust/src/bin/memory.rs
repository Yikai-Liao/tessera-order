// SPDX-License-Identifier: BSD-2-Clause

//! Measures sorting allocations in a process separate from timing.

use std::alloc::{GlobalAlloc, Layout, System};
struct Counting;
// SAFETY: All allocation operations delegate to System with the original layout.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller supplies a valid allocation layout.
        let p = unsafe { System.alloc(layout) };
        #[cfg(not(feature = "compare-native"))]
        if !p.is_null() {
            tessera_bench::allocation::bench_allocation_change(layout.size() as isize);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        #[cfg(not(feature = "compare-native"))]
        tessera_bench::allocation::bench_allocation_change(-(layout.size() as isize));
        // SAFETY: The pointer and layout are the pair returned by System.
        unsafe { System.dealloc(p, layout) };
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tessera_bench::main(true)
}

#[cfg(test)]
mod tests {
    use tessera_bench::allocation::{begin, end};
    #[test]
    fn rust_and_native_allocations_are_counted_once() {
        begin();
        let values = Vec::<u8>::with_capacity(12345);
        std::hint::black_box(&values);
        let rust_peak = end();
        assert!((12345..12400).contains(&rust_peak), "Rust peak {rust_peak}");
        drop(values);
        #[cfg(feature = "compare-native")]
        {
            unsafe extern "C" {
                fn bench_native_memory_probe() -> usize;
            }
            begin();
            // SAFETY: The probe allocates and frees its own two buffers and returns their usable size.
            let expected = unsafe { bench_native_memory_probe() };
            let native_peak = end();
            assert_eq!(native_peak, expected);
            assert!(native_peak >= 2 * 12345);
        }
    }
}
