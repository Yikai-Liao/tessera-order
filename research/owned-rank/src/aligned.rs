// SPDX-License-Identifier: BSD-2-Clause
//! Fixed-length, cache-line aligned metadata allocated before record movement.
use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

pub(super) struct HotVec<T: Copy> {
    ptr: NonNull<T>,
    len: usize,
    layout: Layout,
}
impl<T: Copy> From<Vec<T>> for HotVec<T> {
    fn from(values: Vec<T>) -> Self {
        assert!(std::mem::size_of::<T>() > 0);
        let bytes = values.len().checked_mul(std::mem::size_of::<T>()).unwrap();
        // Round the allocation up so a neighboring allocation cannot share the
        // last cache line, even for P=1/2/4 hot arrays shorter than 64 bytes.
        let size = bytes.max(1).checked_add(63).unwrap() / 64 * 64;
        let layout = Layout::from_size_align(size, std::mem::align_of::<T>().max(64)).unwrap();
        // SAFETY: layout is nonzero and valid; allocation checked before use.
        let raw = unsafe { alloc(layout) };
        let ptr = NonNull::new(raw.cast::<T>()).unwrap_or_else(|| handle_alloc_error(layout));
        // SAFETY: destination is aligned and allocated for all source cells;
        // source is initialized and disjoint. T is Copy and has no drop glue.
        unsafe {
            std::ptr::copy_nonoverlapping(values.as_ptr(), ptr.as_ptr(), values.len());
        }
        Self {
            ptr,
            len: values.len(),
            layout,
        }
    }
}
impl<T: Copy> HotVec<T> {
    pub(super) fn capacity(&self) -> usize {
        self.len
    }
}
impl<T: Copy> Deref for HotVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        // SAFETY: the full len prefix is initialized and owned for self's lifetime.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}
impl<T: Copy> DerefMut for HotVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        // SAFETY: exclusive self borrow grants exclusive access to owned cells.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}
impl<T: Copy> Drop for HotVec<T> {
    fn drop(&mut self) {
        // SAFETY: allocation was created with this exact layout. Copy T has no
        // destructor; no borrowed slice outlives the owned collection.
        unsafe {
            dealloc(self.ptr.as_ptr().cast(), self.layout);
        }
    }
}
// SAFETY: ownership of the allocation transfers with self, just as with Vec<T>.
unsafe impl<T: Copy + Send> Send for HotVec<T> {}
// SAFETY: shared access exposes only shared references to initialized T.
unsafe impl<T: Copy + Sync> Sync for HotVec<T> {}
