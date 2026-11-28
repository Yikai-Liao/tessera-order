// SPDX-License-Identifier: BSD-2-Clause

//! Tagged 32-bit node handles, atomic node slots, and append-only stable-address pools.
//!
//! [`SharedPool`] allows lock-free `&self` reads of published nodes while a serialized
//! cold-path writer (holding the map's refinement mutex) allocates and initializes new
//! nodes in previously unused slots. Because chunks never move or reallocate once
//! created, references `&T` returned from `&self` methods remain valid even as other
//! buckets lazily refine.

use alloc::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};

/// Tagged 32-bit node handle (top 2 bits = variant, low 30 bits = pool index).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(transparent)]
pub(crate) struct NodeId(u32);

const TAG_SHIFT: u32 = 30;
pub(crate) const INDEX_MASK: u32 = (1u32 << TAG_SHIFT) - 1;

pub(crate) const TAG_LEAF: u32 = 0;
pub(crate) const TAG_INTERNAL: u32 = 1;
pub(crate) const TAG_RADIX: u32 = 2;
pub(crate) const TAG_UNREFINED: u32 = 3;

impl NodeId {
    /// Sentinel representing an empty root or empty radix slot.
    pub(crate) const EMPTY: Self = Self(u32::MAX);

    #[inline(always)]
    pub(crate) const fn leaf(index: u32) -> Self {
        debug_assert!(index < INDEX_MASK);
        Self((TAG_LEAF << TAG_SHIFT) | index)
    }

    #[inline(always)]
    pub(crate) const fn internal(index: u32) -> Self {
        debug_assert!(index < INDEX_MASK);
        Self((TAG_INTERNAL << TAG_SHIFT) | index)
    }

    #[inline(always)]
    pub(crate) const fn radix(index: u32) -> Self {
        debug_assert!(index < INDEX_MASK);
        Self((TAG_RADIX << TAG_SHIFT) | index)
    }

    #[inline(always)]
    pub(crate) const fn unrefined(index: u32) -> Self {
        debug_assert!(index < INDEX_MASK);
        Self((TAG_UNREFINED << TAG_SHIFT) | index)
    }

    #[inline(always)]
    pub(crate) const fn is_empty(self) -> bool {
        self.0 == u32::MAX
    }

    #[inline(always)]
    pub(crate) const fn tag(self) -> u32 {
        self.0 >> TAG_SHIFT
    }

    #[inline(always)]
    pub(crate) const fn index(self) -> u32 {
        self.0 & INDEX_MASK
    }
}

/// Atomic 32-bit slot holding a [`NodeId`] for lock-free `&self` reads and
/// monotonic `Unrefined -> Refined` publication during lazy refinement.
#[repr(transparent)]
pub(crate) struct AtomicNodeId(AtomicU32);

impl AtomicNodeId {
    #[inline(always)]
    pub(crate) const fn new(id: NodeId) -> Self {
        Self(AtomicU32::new(id.0))
    }

    #[inline(always)]
    pub(crate) fn load_acquire(&self) -> NodeId {
        NodeId(self.0.load(Ordering::Acquire))
    }

    #[inline(always)]
    pub(crate) fn load_relaxed(&self) -> NodeId {
        NodeId(self.0.load(Ordering::Relaxed))
    }

    #[inline(always)]
    pub(crate) fn store_release(&self, id: NodeId) {
        self.0.store(id.0, Ordering::Release);
    }

    #[inline(always)]
    pub(crate) fn get_mut(&mut self) -> NodeId {
        NodeId(*self.0.get_mut())
    }

    #[inline(always)]
    pub(crate) fn set_mut(&mut self, id: NodeId) {
        *self.0.get_mut() = id.0;
    }
}

// ---------------------------------------------------------------------------
// 2-level append-only stable-address pool (`SharedPool<T>`)
// ---------------------------------------------------------------------------

const SLOTS_PER_CHUNK: usize = 64;
const CHUNKS_PER_BLOCK: usize = 256;
const DIR_LEN: usize = 1024; // 1024 * 256 * 64 = 16,777,216 nodes (~1B leaf entries)

struct Chunk<T> {
    live: AtomicU64,
    slots: [UnsafeCell<MaybeUninit<T>>; SLOTS_PER_CHUNK],
}

impl<T> Chunk<T> {
    fn alloc_raw() -> *mut Self {
        let layout = Layout::new::<Self>();
        // SAFETY: `Self` is non-ZST (contains `live: AtomicU64`).
        let ptr = unsafe { alloc(layout).cast::<Self>() };
        if ptr.is_null() {
            handle_alloc_error(layout);
        }
        // SAFETY: `ptr` is valid, aligned, uniquely owned; only `live` requires init.
        unsafe {
            ptr::addr_of_mut!((*ptr).live).write(AtomicU64::new(0));
        }
        ptr
    }

    /// Drops every initialized slot and frees this chunk allocation, continuing past a
    /// panicking element destructor so no sibling entries or heap allocations are skipped.
    ///
    /// # Safety
    /// Caller must have exclusive ownership of `ptr` (no concurrent readers).
    unsafe fn drop_and_free(ptr: *mut Self) {
        /// # Safety
        /// `ptr` must be an exclusively owned, live `Chunk<T>` allocation.
        unsafe fn finish<T>(ptr: *mut Chunk<T>) {
            // SAFETY: caller guarantees exclusive ownership of `ptr`.
            unsafe {
                loop {
                    let s = {
                        let live = (*ptr).live.get_mut();
                        if *live == 0 {
                            break;
                        }
                        let s = live.trailing_zeros() as usize;
                        *live &= !(1u64 << s);
                        s
                    };
                    (*(*ptr).slots[s].get()).assume_init_drop();
                }
                dealloc(ptr.cast::<u8>(), Layout::new::<Chunk<T>>());
            }
        }

        struct Guard<T>(*mut Chunk<T>);
        impl<T> Drop for Guard<T> {
            fn drop(&mut self) {
                // SAFETY: `Chunk::drop_and_free` owns `self.0` exclusively; each cleared bit in
                // `live` guarantees unwinding resumes at the next live slot.
                unsafe { finish(self.0) };
            }
        }

        let guard = Guard(ptr);
        // SAFETY: caller guarantees exclusive ownership of `ptr`.
        unsafe { finish(guard.0) };
        core::mem::forget(guard);
    }
}

fn alloc_zeroed_raw<T>() -> *mut T {
    let layout = Layout::new::<T>();
    // SAFETY: callers only instantiate non-ZST directory types whose all-zero bit pattern
    // represents null `AtomicPtr` entries.
    let ptr = unsafe { alloc::alloc::alloc_zeroed(layout).cast::<T>() };
    if ptr.is_null() {
        handle_alloc_error(layout);
    }
    ptr
}

struct DirBlock<T> {
    chunks: [AtomicPtr<Chunk<T>>; CHUNKS_PER_BLOCK],
}

struct DirTable<T> {
    blocks: [AtomicPtr<DirBlock<T>>; DIR_LEN],
}

/// Allocation cursor and free-list for one [`SharedPool`], owned either inside the
/// cold-path refinement mutex (during `&self` cracking) or accessed directly under `&mut self`.
pub(crate) struct PoolAlloc {
    pub(crate) free: Vec<u32>,
    pub(crate) next_fresh: u32,
    pub(crate) live_count: usize,
}

impl PoolAlloc {
    pub(crate) const fn new() -> Self {
        Self {
            free: Vec::new(),
            next_fresh: 0,
            live_count: 0,
        }
    }
}

/// Append-only, stable-address chunked arena supporting lock-free `&self` reads.
pub(crate) struct SharedPool<T> {
    dir: AtomicPtr<DirTable<T>>,
}

// SAFETY: `SharedPool<T>` owns its heap allocations exclusively; transferring ownership
// across threads is sound whenever `T: Send`.
unsafe impl<T: Send> Send for SharedPool<T> {}
// SAFETY: `&SharedPool<T>` allows concurrent lock-free reads of published immutable `T`
// entries (`T: Sync`) and serialized initialization of unpublished slots (`T: Send`).
unsafe impl<T: Send + Sync> Sync for SharedPool<T> {}

impl<T> Drop for SharedPool<T> {
    fn drop(&mut self) {
        /// # Safety
        /// `dir_ptr` must be an exclusively owned, live `DirTable<T>` allocation.
        unsafe fn drop_dir<T>(dir_ptr: *mut DirTable<T>) {
            // SAFETY: caller guarantees exclusive ownership of `dir_ptr` and its subtree.
            unsafe {
                let dir = &mut *dir_ptr;
                for block_atom in &mut dir.blocks {
                    let block_ptr = *block_atom.get_mut();
                    if block_ptr.is_null() {
                        continue;
                    }
                    let block = &mut *block_ptr;
                    for chunk_atom in &mut block.chunks {
                        let chunk_ptr = core::mem::replace(chunk_atom.get_mut(), ptr::null_mut());
                        if !chunk_ptr.is_null() {
                            Chunk::drop_and_free(chunk_ptr);
                        }
                    }
                    *block_atom.get_mut() = ptr::null_mut();
                    dealloc(block_ptr.cast::<u8>(), Layout::new::<DirBlock<T>>());
                }
                dealloc(dir_ptr.cast::<u8>(), Layout::new::<DirTable<T>>());
            }
        }

        struct Guard<T>(*mut DirTable<T>);
        impl<T> Drop for Guard<T> {
            fn drop(&mut self) {
                // SAFETY: `SharedPool::drop` owns `self.0` exclusively; nulling consumed slots
                // ensures unwinding resumes from the first remaining chunk/block.
                unsafe { drop_dir(self.0) };
            }
        }

        let dir_ptr = core::mem::replace(self.dir.get_mut(), ptr::null_mut());
        if dir_ptr.is_null() {
            return;
        }
        let guard = Guard(dir_ptr);
        // SAFETY: `&mut self` in `Drop` guarantees exclusive ownership of all directory levels.
        unsafe { drop_dir(guard.0) };
        core::mem::forget(guard);
    }
}

impl<T> SharedPool<T> {
    pub(crate) const fn new() -> Self {
        Self {
            dir: AtomicPtr::new(ptr::null_mut()),
        }
    }

    #[inline(always)]
    fn split_idx(idx: u32) -> (usize, usize, usize) {
        let slot = (idx & 63) as usize;
        let chunk = ((idx >> 6) & 255) as usize;
        let block = (idx >> 14) as usize;
        (block, chunk, slot)
    }

    /// Ensures the chunk for `idx` exists and returns its pointer.
    ///
    /// # Safety
    /// Must be serialized with respect to all other allocations on this pool (either via
    /// `&mut self` or while holding the map's refinement mutex).
    unsafe fn ensure_chunk(&self, block_i: usize, chunk_i: usize) -> *mut Chunk<T> {
        assert!(block_i < DIR_LEN, "TesseraMap pool capacity exceeded");
        let mut dir_ptr = self.dir.load(Ordering::Relaxed);
        if dir_ptr.is_null() {
            dir_ptr = alloc_zeroed_raw::<DirTable<T>>();
            self.dir.store(dir_ptr, Ordering::Release);
        }
        // SAFETY: `dir_ptr` is initialized and never freed until `Drop`.
        let block_atom = unsafe { &(*dir_ptr).blocks[block_i] };
        let mut block_ptr = block_atom.load(Ordering::Relaxed);
        if block_ptr.is_null() {
            block_ptr = alloc_zeroed_raw::<DirBlock<T>>();
            block_atom.store(block_ptr, Ordering::Release);
        }
        // SAFETY: `block_ptr` is initialized and never freed until `Drop`.
        let chunk_atom = unsafe { &(*block_ptr).chunks[chunk_i] };
        let mut chunk_ptr = chunk_atom.load(Ordering::Relaxed);
        if chunk_ptr.is_null() {
            chunk_ptr = Chunk::<T>::alloc_raw();
            chunk_atom.store(chunk_ptr, Ordering::Release);
        }
        chunk_ptr
    }

    /// Allocates and initializes a slot in place while serialized by `alloc` (either under
    /// `&mut self` or under the map's refinement mutex).
    ///
    /// # Safety
    /// `alloc` must belong exclusively to this `SharedPool`, and concurrent callers must
    /// serialize allocations through `&mut PoolAlloc`.
    #[inline]
    pub(crate) unsafe fn alloc_with(
        &self,
        alloc: &mut PoolAlloc,
        init: impl FnOnce(&mut MaybeUninit<T>),
    ) -> u32 {
        let idx = if let Some(reused) = alloc.free.pop() {
            reused
        } else {
            let idx = alloc.next_fresh;
            alloc.next_fresh = idx.checked_add(1).expect("node index overflow");
            idx
        };
        let (block_i, chunk_i, slot_i) = Self::split_idx(idx);
        // SAFETY: caller serializes allocations; `idx` is not currently live or published.
        unsafe {
            let chunk_ptr = self.ensure_chunk(block_i, chunk_i);
            let slot_ref = &mut *(*chunk_ptr).slots[slot_i].get();
            init(slot_ref);
            // Publish liveness AFTER `init` completes so unwinding never drops uninit data.
            (*chunk_ptr)
                .live
                .fetch_or(1u64 << slot_i, Ordering::Release);
        }
        alloc.live_count += 1;
        idx
    }

    /// Drops the element at `idx` and returns the slot to `alloc.free`.
    /// Requires `&mut self` so no concurrent reader can hold a reference to `idx`.
    #[inline]
    pub(crate) fn free(&mut self, alloc: &mut PoolAlloc, idx: u32) {
        let (block_i, chunk_i, slot_i) = Self::split_idx(idx);
        let dir_ptr = *self.dir.get_mut();
        debug_assert!(!dir_ptr.is_null());
        alloc.live_count -= 1;
        alloc.free.push(idx);
        // SAFETY: `&mut self` guarantees exclusive access and `idx` is live.
        unsafe {
            let block_ptr = *(*dir_ptr).blocks[block_i].get_mut();
            let chunk_ptr = *(*block_ptr).chunks[chunk_i].get_mut();
            let live = (*chunk_ptr).live.get_mut();
            debug_assert!((*live >> slot_i) & 1 == 1);
            *live &= !(1u64 << slot_i);
            (*(*chunk_ptr).slots[slot_i].get()).assume_init_drop();
        }
    }

    /// Lock-free `&self` read of a published slot `idx`.
    ///
    /// # Safety
    /// `idx` must refer to a currently live slot in this pool obtained through an `Acquire`
    /// load (or `&mut self`) that synchronizes with the `Release` publication after `alloc_with`.
    #[inline(always)]
    pub(crate) unsafe fn get(&self, idx: u32) -> &T {
        let (block_i, chunk_i, slot_i) = Self::split_idx(idx);
        // SAFETY: Caller guarantees `idx` is live and synchronized with `Release` publication
        // after `dir`, `blocks[block_i]`, `chunks[chunk_i]`, and `slots[slot_i]` were initialized.
        unsafe {
            let dir_ptr = self.dir.load(Ordering::Relaxed);
            let block_ptr = (*dir_ptr)
                .blocks
                .get_unchecked(block_i)
                .load(Ordering::Relaxed);
            let chunk_ptr = (*block_ptr)
                .chunks
                .get_unchecked(chunk_i)
                .load(Ordering::Relaxed);
            debug_assert!(((*chunk_ptr).live.load(Ordering::Relaxed) >> slot_i) & 1 == 1);
            (*(*chunk_ptr).slots.get_unchecked(slot_i).get()).assume_init_ref()
        }
    }

    /// Exclusive `&mut self` mutable borrow of live slot `idx`.
    ///
    /// # Safety
    /// `idx` must refer to a currently live slot in this pool.
    #[inline(always)]
    pub(crate) unsafe fn get_mut(&mut self, idx: u32) -> &mut T {
        let (block_i, chunk_i, slot_i) = Self::split_idx(idx);
        let dir_ptr = *self.dir.get_mut();
        debug_assert!(!dir_ptr.is_null());
        // SAFETY: `&mut self` guarantees exclusive access and caller guarantees `idx` is live.
        unsafe {
            let block_ptr = *(*dir_ptr).blocks.get_unchecked_mut(block_i).get_mut();
            let chunk_ptr = *(*block_ptr).chunks.get_unchecked_mut(chunk_i).get_mut();
            debug_assert!((*(*chunk_ptr).live.get_mut() >> slot_i) & 1 == 1);
            (*(*chunk_ptr).slots.get_unchecked_mut(slot_i).get()).assume_init_mut()
        }
    }

    /// Borrows two distinct live elements mutably under `&mut self` (used during split).
    ///
    /// # Safety
    /// `a` and `b` must both refer to currently live slots in this pool.
    #[inline(always)]
    pub(crate) unsafe fn get_two_mut(&mut self, a: u32, b: u32) -> (&mut T, &mut T) {
        assert_ne!(a, b);
        let (ba, ca, sa) = Self::split_idx(a);
        let (bb, cb, sb) = Self::split_idx(b);
        let dir_ptr = *self.dir.get_mut();
        // SAFETY: `a != b` implies `(ba, ca, sa) != (bb, cb, sb)`, so the two `UnsafeCell`s
        // are strictly disjoint and `&mut self` guarantees exclusive ownership of the pool.
        unsafe {
            let chunk_a =
                (*(*dir_ptr).blocks[ba].load(Ordering::Relaxed)).chunks[ca].load(Ordering::Relaxed);
            let chunk_b =
                (*(*dir_ptr).blocks[bb].load(Ordering::Relaxed)).chunks[cb].load(Ordering::Relaxed);
            debug_assert!(((*chunk_a).live.load(Ordering::Relaxed) >> sa) & 1 == 1);
            debug_assert!(((*chunk_b).live.load(Ordering::Relaxed) >> sb) & 1 == 1);
            let ptr_a = (*chunk_a).slots.get_unchecked(sa).get();
            let ptr_b = (*chunk_b).slots.get_unchecked(sb).get();
            ((*ptr_a).assume_init_mut(), (*ptr_b).assume_init_mut())
        }
    }
}
