// SPDX-License-Identifier: BSD-2-Clause

//! Measures sorting allocations in a process separate from timing.

use std::alloc::{GlobalAlloc, Layout, System};
/// Measures allocation sizes while delegating storage to the system allocator.
struct Counting;
// SAFETY: All allocation operations delegate to System with the original layout.
unsafe impl GlobalAlloc for Counting {
    /// Allocates storage and records the live allocation size.
    ///
    /// # Safety
    ///
    /// `layout` satisfies [`GlobalAlloc::alloc`]'s requirements.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller supplies a valid allocation layout.
        let p = unsafe { System.alloc(layout) };
        #[cfg(not(feature = "compare-native"))]
        if !p.is_null() {
            tessera_bench::allocation::bench_allocation_change(layout.size() as isize);
        }
        p
    }
    /// Resizes storage through the system allocator and records the size change.
    ///
    /// # Safety
    ///
    /// `allocation`, `layout` and `new_size_bytes` satisfy [`GlobalAlloc::realloc`].
    unsafe fn realloc(
        &self,
        allocation: *mut u8,
        layout: Layout,
        new_size_bytes: usize,
    ) -> *mut u8 {
        // SAFETY: The caller supplies a live allocation and a valid resized layout.
        let resized = unsafe { System.realloc(allocation, layout, new_size_bytes) };
        #[cfg(not(feature = "compare-native"))]
        if !resized.is_null() {
            tessera_bench::allocation::bench_allocation_change(
                new_size_bytes as isize - layout.size() as isize,
            );
        }
        resized
    }
    /// Releases storage and removes its live allocation size.
    ///
    /// # Safety
    ///
    /// `p` and `layout` identify a live allocation returned by this allocator.
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        #[cfg(not(feature = "compare-native"))]
        tessera_bench::allocation::bench_allocation_change(-(layout.size() as isize));
        // SAFETY: The pointer and layout are the pair returned by System.
        unsafe { System.dealloc(p, layout) };
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
/// Runs the memory command-line executable.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tessera_bench::main(true)
}

#[cfg(test)]
mod tests {
    use tessera_bench::allocation::{begin, end};
    #[test]
    fn allocations_count_once() {
        begin();
        let values = Vec::<u8>::with_capacity(12345);
        std::hint::black_box(&values);
        let rust_peak = end();
        assert!((12345..12400).contains(&rust_peak), "Rust peak {rust_peak}");
        drop(values);
        begin();
        let mut buffer = Vec::<u8>::with_capacity(12345);
        buffer.reserve_exact(20000);
        std::hint::black_box(&buffer);
        let resized_peak_bytes = end();
        assert!(
            (20000..20200).contains(&resized_peak_bytes),
            "resized peak {resized_peak_bytes}"
        );
        drop(buffer);
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
