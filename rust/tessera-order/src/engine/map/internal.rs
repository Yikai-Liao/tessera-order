// SPDX-License-Identifier: BSD-2-Clause

//! Compact 31-way branchless SIMD internal node (384 bytes = 6 exact cache lines for `u64`).
//!
//! Layout for `E = u64`:
//! - Cache Lines 0..4 (256B): `keys: [u64; 32]` (upper separators; `keys[len - 1 .. 32] == MAX`).
//! - Cache Lines 4..6 (128B): `children: [AtomicNodeId; 31]` (124B) + `len: u32` (4B).

use super::pool::{AtomicNodeId, NodeId};
use super::simd::count_lt_16;
use crate::engine::key::Unsigned;
use core::mem::MaybeUninit;
use core::ptr;
use fearless_simd::Simd;

/// Maximum fanout of one internal B+-Tree node.
pub(crate) const INTERNAL_CAP: usize = 31;

/// 31-way branchless SIMD internal node (384B for `u64`, zero padding).
#[repr(C, align(64))]
pub(crate) struct Internal<E: Unsigned> {
    /// Upper-bound separator keys: `children[i]` holds keys `<= keys[i]`.
    /// Unused tail slots (`len - 1 .. 32`, including slot 31) always hold `E::MAX`.
    pub(crate) keys: [E; 32],
    /// Atomic child handles parallel to `keys[0..31]`.
    pub(crate) children: [AtomicNodeId; INTERNAL_CAP],
    /// Number of active children (`1..=31`).
    pub(crate) len: u32,
}

impl<E: Unsigned> Internal<E> {
    /// Initializes an internal node with two children separated by `sep`.
    #[inline]
    pub(crate) fn init_pair(slot: &mut MaybeUninit<Self>, left: NodeId, sep: E, right: NodeId) {
        let ptr = slot.as_mut_ptr();
        // SAFETY: writing every field via raw pointers before forming `&mut Self`.
        let node = unsafe {
            ptr::addr_of_mut!((*ptr).keys).write([E::MAX; 32]);
            ptr::addr_of_mut!((*ptr).children)
                .write([const { AtomicNodeId::new(NodeId::EMPTY) }; INTERNAL_CAP]);
            ptr::addr_of_mut!((*ptr).len).write(2);
            &mut *ptr
        };
        node.keys[0] = sep;
        node.children[0].set_mut(left);
        node.children[1].set_mut(right);
    }

    /// Initializes an internal node from a slice of `(child, max_key)` pairs (`1 <= n <= 31`).
    pub(crate) fn init_from_children(slot: &mut MaybeUninit<Self>, items: &[(NodeId, E)]) {
        let n = items.len();
        debug_assert!((1..=INTERNAL_CAP).contains(&n));
        let ptr = slot.as_mut_ptr();
        // SAFETY: writing every field via raw pointers before forming `&mut Self`.
        let node = unsafe {
            ptr::addr_of_mut!((*ptr).keys).write([E::MAX; 32]);
            ptr::addr_of_mut!((*ptr).children)
                .write([const { AtomicNodeId::new(NodeId::EMPTY) }; INTERNAL_CAP]);
            ptr::addr_of_mut!((*ptr).len).write(n as u32);
            &mut *ptr
        };
        for (i, &(child, max_k)) in items.iter().enumerate() {
            node.children[i].set_mut(child);
            if i + 1 < n {
                node.keys[i] = max_k;
            }
        }
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.len as usize
    }

    #[inline(always)]
    pub(crate) fn is_full(&self) -> bool {
        self.len as usize == INTERNAL_CAP
    }

    /// Branchless half-select + portable `fearless_simd` search returning child index `0..self.len`.
    #[inline(always)]
    pub(crate) fn find_child_idx<S: Simd>(&self, simd: S, target: E) -> usize {
        let base = usize::from(target > self.keys[15]) << 4;
        // SAFETY: `base` is `0` or `16`, and `self.keys` has 32 elements.
        let half: &[E; 16] = unsafe { &*(self.keys.as_ptr().add(base).cast::<[E; 16]>()) };
        let idx = base + count_lt_16(simd, half, target);
        debug_assert!(idx < self.len as usize);
        idx
    }

    /// Inserts a split right sibling `(sep, right_child)` right after child `child_idx`.
    #[inline]
    pub(crate) fn insert_after(&mut self, child_idx: usize, sep: E, right_child: NodeId) {
        let len = self.len as usize;
        debug_assert!(len < INTERNAL_CAP && child_idx < len);

        for i in (child_idx + 1..len).rev() {
            let moved = self.children[i].get_mut();
            self.children[i + 1].set_mut(moved);
        }
        self.children[child_idx + 1].set_mut(right_child);

        self.keys.copy_within(child_idx..len - 1, child_idx + 1);
        self.keys[child_idx] = sep;
        self.keys[len] = E::MAX;

        self.len = (len + 1) as u32;
    }

    /// Removes child at `child_idx` under `&mut self`.
    #[inline]
    pub(crate) fn remove_child(&mut self, child_idx: usize) {
        let len = self.len as usize;
        debug_assert!(child_idx < len && len >= 2);
        for i in child_idx..len - 1 {
            let next = self.children[i + 1].get_mut();
            self.children[i].set_mut(next);
        }
        self.children[len - 1].set_mut(NodeId::EMPTY);
        if child_idx + 1 < len {
            self.keys.copy_within(child_idx + 1..len - 1, child_idx);
        }
        self.keys[len - 2] = E::MAX;
        self.len = (len - 1) as u32;
    }

    /// Splits a full internal node (`len == 31`), inserting `(sep, right_child)` after `child_idx`.
    pub(crate) fn split_and_insert(
        &mut self,
        right: &mut Self,
        child_idx: usize,
        sep: E,
        right_child: NodeId,
    ) -> E {
        debug_assert_eq!(self.len as usize, INTERNAL_CAP);
        let mut all_children = [NodeId::EMPTY; INTERNAL_CAP + 1];
        let mut all_keys = [E::MAX; INTERNAL_CAP];

        for (dst, src) in all_children[..=child_idx]
            .iter_mut()
            .zip(&mut self.children[..=child_idx])
        {
            *dst = src.get_mut();
        }
        all_children[child_idx + 1] = right_child;
        for (dst, src) in all_children[child_idx + 2..]
            .iter_mut()
            .zip(&mut self.children[child_idx + 1..])
        {
            *dst = src.get_mut();
        }

        all_keys[..child_idx].copy_from_slice(&self.keys[..child_idx]);
        all_keys[child_idx] = sep;
        all_keys[child_idx + 1..].copy_from_slice(&self.keys[child_idx..INTERNAL_CAP - 1]);

        const LEFT_N: usize = 16;
        const RIGHT_N: usize = 16;
        let promoted_sep = all_keys[LEFT_N - 1];

        for (i, &c) in all_children[..LEFT_N].iter().enumerate() {
            self.children[i].set_mut(c);
        }
        for i in LEFT_N..INTERNAL_CAP {
            self.children[i].set_mut(NodeId::EMPTY);
        }
        self.keys[..LEFT_N - 1].copy_from_slice(&all_keys[..LEFT_N - 1]);
        self.keys[LEFT_N - 1..].fill(E::MAX);
        self.len = LEFT_N as u32;

        right.keys.fill(E::MAX);
        for (i, &c) in all_children[LEFT_N..].iter().enumerate() {
            right.children[i].set_mut(c);
        }
        for i in RIGHT_N..INTERNAL_CAP {
            right.children[i].set_mut(NodeId::EMPTY);
        }
        right.keys[..RIGHT_N - 1].copy_from_slice(&all_keys[LEFT_N..]);
        right.len = RIGHT_N as u32;
        debug_assert_eq!(LEFT_N + RIGHT_N, INTERNAL_CAP + 1);

        promoted_sep
    }
}
