// SPDX-License-Identifier: BSD-2-Clause

//! Backing slabs, atomic prefix-compressed radix directories, and unrefined slice descriptors
//! shared by transparent lazy refinement and streaming batch ingestion.

use super::pool::{AtomicNodeId, NodeId};
use crate::engine::key::Unsigned;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use core::sync::atomic::AtomicU32;

/// Owned contiguous slab holding unrefined `(K, V)` records.
///
/// Accessed exclusively inside the refinement mutex or under `&mut self`; no `&V` returned
/// from `&self` methods ever borrows into a `RawSlab`.
pub(crate) struct RawSlab<K, V> {
    pub(crate) data: Box<[MaybeUninit<(K, V)>]>,
    pub(crate) live_count: usize,
}

impl<K, V> RawSlab<K, V> {
    pub(crate) fn from_vec(vec: Vec<(K, V)>) -> Self {
        let len = vec.len();
        let raw = Box::into_raw(vec.into_boxed_slice()) as *mut [MaybeUninit<(K, V)>];
        // SAFETY: `MaybeUninit<T>` has identical layout to `T`.
        let data = unsafe { Box::from_raw(raw) };
        Self {
            data,
            live_count: len,
        }
    }

    /// Reclaims the whole slab back into a `Vec<(K, V)>` in O(1) without copying.
    ///
    /// # Safety
    /// Every slot in `0..self.data.len()` must be initialized (`self.live_count == self.data.len()`).
    pub(crate) unsafe fn into_vec(self) -> Vec<(K, V)> {
        debug_assert_eq!(self.live_count, self.data.len());
        let raw = Box::into_raw(self.data) as *mut [(K, V)];
        // SAFETY: `MaybeUninit<T>` has identical layout to `T`, and all slots are initialized.
        unsafe { Box::from_raw(raw) }.into_vec()
    }

    /// Borrows an exclusively owned initialized subrange mutably.
    ///
    /// # Safety
    /// Caller must exclusively own live initialized records in `start..end`.
    #[inline]
    pub(crate) unsafe fn slice_mut(&mut self, start: u32, end: u32) -> &mut [(K, V)] {
        let (s, e) = (start as usize, end as usize);
        debug_assert!(s <= e && e <= self.data.len());
        // SAFETY: forwarded from caller contract.
        unsafe {
            core::slice::from_raw_parts_mut(self.data.as_mut_ptr().add(s).cast::<(K, V)>(), e - s)
        }
    }

    /// Borrows an initialized subrange immutably.
    ///
    /// # Safety
    /// Caller must guarantee `start..end` holds initialized records.
    #[inline]
    pub(crate) unsafe fn slice(&self, start: u32, end: u32) -> &[(K, V)] {
        let (s, e) = (start as usize, end as usize);
        debug_assert!(s <= e && e <= self.data.len());
        // SAFETY: forwarded from caller contract.
        unsafe { core::slice::from_raw_parts(self.data.as_ptr().add(s).cast::<(K, V)>(), e - s) }
    }
}

/// Prefix-compressed multi-way radix directory node produced by Tessera's `block::partition`.
///
/// Stored in [`SharedPool`] so `&self` readers never race with reallocations; child handles
/// and bucket counts use atomics for lock-free reads and safe `Unrefined -> Refined` swaps.
pub(crate) struct RadixNode<E: Unsigned> {
    pub(crate) prefix_base: E,
    pub(crate) prefix_shift: u8,
    pub(crate) shift: u8,
    pub(crate) width: u8,
    pub(crate) children: Box<[AtomicNodeId]>,
    pub(crate) counts: Box<[AtomicU32]>,
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
        counts: Vec<u32>,
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
                .map(AtomicU32::new)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        }
    }
}

/// Unrefined subslice `[start..end)` owned inside `slabs[slab_id]`.
pub(crate) struct UnrefinedNode {
    pub(crate) slab_id: u32,
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) candidate_mask: u128,
}

impl UnrefinedNode {
    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        (self.end - self.start) as usize
    }
}
