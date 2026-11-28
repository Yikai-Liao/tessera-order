// SPDX-License-Identifier: BSD-2-Clause

//! Backing slabs, atomic prefix-compressed radix directories, and unrefined slice descriptors
//! shared by transparent lazy refinement and streaming batch ingestion.

use super::pool::{AtomicNodeId, NodeId};
use crate::engine::key::Unsigned;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::sync::atomic::AtomicUsize;

/// Owned contiguous slab holding unrefined `(K, V)` records.
///
/// Retains the incoming `Vec`'s allocation without shrinking so adopting a batch with spare
/// capacity stays strictly `O(1)`. Accessed exclusively inside the refinement mutex or under
/// `&mut self`; no `&V` returned from `&self` methods ever borrows into a `RawSlab`.
pub(crate) struct RawSlab<K, V> {
    pub(crate) data: Vec<MaybeUninit<(K, V)>>,
    pub(crate) live_count: usize,
}

impl<K, V> RawSlab<K, V> {
    pub(crate) fn from_vec(vec: Vec<(K, V)>) -> Self {
        let mut vec = ManuallyDrop::new(vec);
        let (ptr, len, cap) = (
            vec.as_mut_ptr().cast::<MaybeUninit<(K, V)>>(),
            vec.len(),
            vec.capacity(),
        );
        // SAFETY: `MaybeUninit<T>` has identical size and alignment to `T`, and
        // `(ptr, len, cap)` came directly from `vec`.
        let data = unsafe { Vec::from_raw_parts(ptr, len, cap) };
        Self {
            data,
            live_count: len,
        }
    }

    /// Reclaims the whole slab back into a `Vec<(K, V)>` in `O(1)` without copying.
    ///
    /// # Safety
    /// Every slot in `0..self.data.len()` must be initialized (`self.live_count == self.data.len()`).
    pub(crate) unsafe fn into_vec(self) -> Vec<(K, V)> {
        debug_assert_eq!(self.live_count, self.data.len());
        let mut data = ManuallyDrop::new(self.data);
        let (ptr, len, cap) = (
            data.as_mut_ptr().cast::<(K, V)>(),
            data.len(),
            data.capacity(),
        );
        // SAFETY: `MaybeUninit<T>` has identical layout to `T`, and all `len` slots are initialized.
        unsafe { Vec::from_raw_parts(ptr, len, cap) }
    }

    /// Borrows an exclusively owned initialized subrange mutably.
    ///
    /// # Safety
    /// Caller must exclusively own live initialized records in `start..end`.
    #[inline]
    pub(crate) unsafe fn slice_mut(&mut self, start: usize, end: usize) -> &mut [(K, V)] {
        debug_assert!(start <= end && end <= self.data.len());
        // SAFETY: forwarded from caller contract.
        unsafe {
            core::slice::from_raw_parts_mut(
                self.data.as_mut_ptr().add(start).cast::<(K, V)>(),
                end - start,
            )
        }
    }

    /// Borrows an initialized subrange immutably.
    ///
    /// # Safety
    /// Caller must guarantee `start..end` holds initialized records.
    #[inline]
    pub(crate) unsafe fn slice(&self, start: usize, end: usize) -> &[(K, V)] {
        debug_assert!(start <= end && end <= self.data.len());
        // SAFETY: forwarded from caller contract.
        unsafe {
            core::slice::from_raw_parts(self.data.as_ptr().add(start).cast::<(K, V)>(), end - start)
        }
    }
}

/// Prefix-compressed multi-way radix directory node produced by Tessera's `block::partition`.
///
/// Stored in [`super::pool::SharedPool`] so `&self` readers never race with reallocations;
/// child handles and bucket counts use atomics for lock-free reads and safe
/// `Unrefined -> Refined` swaps.
pub(crate) struct RadixNode<E: Unsigned> {
    pub(crate) prefix_base: E,
    pub(crate) prefix_shift: u8,
    pub(crate) shift: u8,
    pub(crate) width: u8,
    pub(crate) children: Box<[AtomicNodeId]>,
    pub(crate) counts: Box<[AtomicUsize]>,
}

impl<E: Unsigned> RadixNode<E> {
    #[inline(always)]
    pub(crate) fn contains_prefix(&self, key: E) -> bool {
        let ps = self.prefix_shift as u32;
        ps >= E::BITS as u32 || ((key.to_u128() ^ self.prefix_base.to_u128()) >> ps) == 0
    }

    #[inline(always)]
    pub(crate) fn bucket_of(&self, key: E) -> usize {
        let mask = (1u128 << self.width) - 1;
        ((key.to_u128() >> self.shift) & mask) as usize
    }

    #[inline(always)]
    pub(crate) fn min_covered(&self) -> E {
        self.prefix_base
    }

    #[inline(always)]
    pub(crate) fn max_covered(&self) -> E {
        let ps = self.prefix_shift as u32;
        let low = if ps >= 128 {
            u128::MAX
        } else {
            (1u128 << ps) - 1
        };
        E::from_u128(self.prefix_base.to_u128() | low)
    }

    #[inline]
    pub(crate) fn bucket_range(&self, start: E, end_inclusive: E) -> Option<(usize, usize)> {
        if start > self.max_covered() || end_inclusive < self.min_covered() {
            return None;
        }
        let lo = if start <= self.min_covered() {
            0
        } else {
            self.bucket_of(start)
        };
        let hi = if end_inclusive >= self.max_covered() {
            self.children.len() - 1
        } else {
            self.bucket_of(end_inclusive)
        };
        (lo <= hi).then_some((lo, hi))
    }

    /// Initializes a `RadixNode` directly from plain child/count vectors.
    pub(crate) fn new(
        prefix_base: E,
        prefix_shift: u8,
        shift: u8,
        width: u8,
        children: Vec<NodeId>,
        counts: Vec<usize>,
    ) -> Self {
        Self {
            prefix_base,
            prefix_shift,
            shift,
            width,
            children: children
                .into_iter()
                .map(AtomicNodeId::new)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            counts: counts
                .into_iter()
                .map(AtomicUsize::new)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        }
    }
}

/// Unrefined subslice `[start..end)` owned inside `slabs[slab_id]`.
pub(crate) struct UnrefinedNode {
    pub(crate) slab_id: u32,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) candidate_mask: u128,
}

impl UnrefinedNode {
    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.end - self.start
    }
}
