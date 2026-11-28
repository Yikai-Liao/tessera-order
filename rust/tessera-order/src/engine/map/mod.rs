// SPDX-License-Identifier: BSD-2-Clause

//! Unified adaptive ordered map and set built directly on Tessera's radix refinement engine.
//!
//! [`TesseraMap`] exposes one `Send + Sync` ordered-map API (`new`, `from`, `from_unique`,
//! `get`, `range`, `select_nth`, `first`, `last`, `insert`, `remove`, `extend_vec`) with
//! transparent on-demand refinement hidden behind an append-only stable-address [`SharedPool`]
//! and atomic [`AtomicNodeId`] slots:
//! - Small inputs (`< LAZY_THRESHOLD`) build bottom-up immediately.
//! - Large unsorted inputs (`From<Vec<(K, V)>>` / `FromIterator` / `from_unique`) adopt the
//!   caller's `Vec` allocation in `O(1)` time without shrinking or copying, refining only
//!   touched buckets during `&self` reads using Tessera's `block::partition` and `tile::Scratch`.
//! - Published leaves never move or mutate during `&self`, so returned `&V` references remain
//!   valid across concurrent reads and lazy refinement of disjoint buckets.
//! - Overwritten duplicate values discovered during lazy refinement are dropped only after the
//!   refined subtree is published and internal locks are released.
//!
//! [`TesseraSet`] delegates directly to `TesseraMap<K, ()>`: zero-sized `()` values shrink each
//! 64-slot leaf from 1088B to 576B while reusing every refinement and query path unchanged.

mod internal;
mod leaf;
mod pool;
mod simd;
mod slab;
#[cfg(test)]
mod tests;

use alloc::vec::Vec;
use core::marker::PhantomData;
use core::mem::{self, MaybeUninit};
use core::ops::{Bound, RangeBounds};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use crate::engine::block::{self, MAX_RADIX_BITS};
use crate::engine::key::{ByKey, Digit, KeyDigit, RadixKey, Unsigned, bit_len};
use crate::engine::order::scan_varying_bits;
use crate::engine::refine::sort as engine_sort;
use crate::engine::thresholds::TILE_BYTES;
use crate::engine::tile::capacity as tile_capacity;
use crate::engine::workspace::Workspace;
use fearless_simd::{Level, Simd, dispatch};

use internal::{INTERNAL_CAP, Internal};
use leaf::{BULK_LEAF_FILL, LEAF_CAP, Leaf, LeafInsert};
use pool::{
    AtomicNodeId, INDEX_MASK, NodeId, PoolAlloc, SharedPool, TAG_INTERNAL, TAG_LEAF, TAG_RADIX,
    TAG_UNREFINED,
};
use slab::{RadixNode, RawSlab, UnrefinedNode};

/// Inputs below this length build eagerly in `From<Vec>`; larger unsorted inputs adopt their
/// buffer in `O(1)` and refine transparently on demand during `&self` reads.
const LAZY_THRESHOLD: usize = 16_384;
/// Maximum records in an unrefined bucket before materializing directly into leaves.
const TILE_REFINE_THRESHOLD: usize = 1024;
/// Maximum radix width per lazy crack pass (8 bits = 256 buckets).
const CRACK_MAX_BITS: usize = 8;
/// Minimum slab capacity before adaptive slab compaction fires below 25% utilization.
const COMPACT_MIN_SLAB_LEN: usize = 16_384;
/// Inline stack capacity for descent paths before spilling to heap (handles arbitrary `u128` depth).
const INLINE_PATH_CAP: usize = 16;

/// Provisional structural diagnostics for [`TesseraMap`] and [`TesseraSet`].
///
/// Inspecting `stats()` does not refine unrefined buckets. When the container holds unresolved
/// duplicates from `From<Vec<_>>` or `extend_vec`, `len` reflects the current provisional count
/// until [`TesseraMap::len`] or lazy refinement deduplicates the remaining buckets.
#[derive(Clone, Copy, Debug, Default)]
pub struct MapStats {
    /// Provisional entry count (exact once `unrefined == 0` or after calling `len()`).
    pub len: usize,
    /// Number of live `Leaf` nodes.
    pub leaves: usize,
    /// Number of live `Internal` nodes.
    pub internals: usize,
    /// Number of live `Radix` directory nodes.
    pub radixes: usize,
    /// Number of remaining unrefined buckets.
    pub unrefined: usize,
    /// Number of backing raw slabs still alive.
    pub live_slabs: usize,
    /// Live unrefined records remaining across all slabs.
    pub unrefined_records: usize,
}

/// One parent slot step recorded during iterative descent.
#[derive(Clone, Copy)]
struct PathStep {
    node: NodeId,
    slot: usize,
}

/// Stack-backed descent path with heap overflow for arbitrarily deep trees (e.g. skewed `u128`).
struct PathStack {
    inline: [PathStep; INLINE_PATH_CAP],
    inline_len: usize,
    overflow: Vec<PathStep>,
}

impl PathStack {
    #[inline(always)]
    fn new() -> Self {
        Self {
            inline: [PathStep {
                node: NodeId::EMPTY,
                slot: 0,
            }; INLINE_PATH_CAP],
            inline_len: 0,
            overflow: Vec::new(),
        }
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.inline_len == 0
    }

    #[inline(always)]
    fn push(&mut self, step: PathStep) {
        if self.inline_len < INLINE_PATH_CAP {
            self.inline[self.inline_len] = step;
            self.inline_len += 1;
        } else {
            self.overflow.push(step);
        }
    }

    #[inline(always)]
    fn pop(&mut self) {
        if self.overflow.pop().is_none() && self.inline_len > 0 {
            self.inline_len -= 1;
        }
    }

    #[inline(always)]
    fn last(&self) -> Option<PathStep> {
        self.overflow
            .last()
            .copied()
            .or_else(|| (self.inline_len > 0).then(|| self.inline[self.inline_len - 1]))
    }

    #[inline(always)]
    fn get(&self, index: usize) -> Option<PathStep> {
        if index < self.inline_len {
            Some(self.inline[index])
        } else {
            self.overflow.get(index - self.inline_len).copied()
        }
    }

    #[inline(always)]
    fn for_each(&self, mut f: impl FnMut(PathStep)) {
        for &step in self.inline[..self.inline_len].iter().chain(&self.overflow) {
            f(step);
        }
    }

    #[inline(always)]
    fn iter_rev(&self) -> impl Iterator<Item = (usize, PathStep)> + '_ {
        let base = self.inline_len;
        self.overflow
            .iter()
            .enumerate()
            .rev()
            .map(move |(i, &s)| (base + i, s))
            .chain(
                self.inline[..self.inline_len]
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(i, &s)| (i, s)),
            )
    }
}

/// Runs Tessera's stable in-place `block::partition` on a single key digit window.
fn partition_by_digit<K: RadixKey, V>(
    slice: &mut [(K, V)],
    shift: usize,
    width: usize,
    workspace: &mut Workspace<(K, V)>,
) -> Vec<usize> {
    let mut ext = ByKey(|r: &(K, V)| r.0);
    let classifier = KeyDigit::new(&mut ext, shift);
    let mut noop = |_: usize,
                    _: core::ops::Range<usize>,
                    _: &mut [(K, V)],
                    _: &mut KeyDigit<&mut ByKey<_>, K::Encoded>| {};
    block::partition(slice, width, classifier, &mut workspace.block, &mut noop)
}

/// Sorts `slice` using Tessera's reusable tile scratch when tile-sized, else full `engine_sort`.
fn sort_slice<K: RadixKey, V>(
    slice: &mut [(K, V)],
    candidate_mask: u128,
    workspace: &mut Workspace<(K, V)>,
) {
    if slice.len() <= 1 {
        return;
    }
    if slice.len() <= tile_capacity::<(K, V)>(TILE_BYTES) {
        let mut ext = ByKey(|r: &(K, V)| r.0);
        let varying = scan_varying_bits(slice, candidate_mask, &mut ext);
        workspace.tile.sort(slice, varying, &mut ext);
    } else {
        engine_sort(slice, ByKey(|r: &(K, V)| r.0));
    }
}

/// Partitions a sorted slice in place so `0..kept` holds unique keys (last value wins) and
/// `kept..` holds overwritten duplicate entries. Does not drop any values.
fn dedup_sorted_last_wins_partition<K: RadixKey, V>(records: &mut [(K, V)]) -> usize {
    let n = records.len();
    if n < 2 {
        return n;
    }
    let mut write = 0usize;
    for read in 1..n {
        if records[read].0.encode() == records[write].0.encode() {
            records.swap(write, read);
        } else {
            write += 1;
            if write != read {
                records.swap(write, read);
            }
        }
    }
    write + 1
}

/// Converts a `RangeBounds<K>` into an inclusive encoded range `(lo, hi)`, or `None` if empty.
fn encoded_inclusive_bounds<K: RadixKey>(
    bounds: &impl RangeBounds<K>,
) -> Option<(K::Encoded, K::Encoded)> {
    let lo = match bounds.start_bound() {
        Bound::Unbounded => K::Encoded::from_usize(0),
        Bound::Included(&k) => k.encode(),
        Bound::Excluded(&k) => {
            let ek = k.encode();
            if ek == K::Encoded::MAX {
                return None;
            }
            K::Encoded::from_u128(ek.to_u128() + 1)
        }
    };
    let hi = match bounds.end_bound() {
        Bound::Unbounded => K::Encoded::MAX,
        Bound::Included(&k) => k.encode(),
        Bound::Excluded(&k) => {
            let ek = k.encode();
            if ek == K::Encoded::from_usize(0) {
                return None;
            }
            K::Encoded::from_u128(ek.to_u128() - 1)
        }
    };
    (lo <= hi).then_some((lo, hi))
}

/// Cold-path refinement and allocation bookkeeping protected by `Mutex` during `&self`
/// lazy refinement and bypassed without locking under `&mut self`.
struct RefineState<K, V> {
    leaf_alloc: PoolAlloc,
    internal_alloc: PoolAlloc,
    radix_alloc: PoolAlloc,
    unrefineds: Vec<Option<UnrefinedNode>>,
    free_unrefineds: Vec<u32>,
    slabs: Vec<Option<RawSlab<K, V>>>,
    free_slabs: Vec<u32>,
    workspace: Workspace<(K, V)>,
}

impl<K, V> RefineState<K, V> {
    fn new() -> Self {
        Self {
            leaf_alloc: PoolAlloc::new(),
            internal_alloc: PoolAlloc::new(),
            radix_alloc: PoolAlloc::new(),
            unrefineds: Vec::new(),
            free_unrefineds: Vec::new(),
            slabs: Vec::new(),
            free_slabs: Vec::new(),
            workspace: Workspace::new(),
        }
    }

    fn alloc_slab(&mut self, slab: RawSlab<K, V>) -> u32 {
        if let Some(id) = self.free_slabs.pop() {
            self.slabs[id as usize] = Some(slab);
            id
        } else {
            let id = u32::try_from(self.slabs.len()).expect("slab index overflow");
            self.slabs.push(Some(slab));
            id
        }
    }

    fn take_slab(&mut self, id: u32) -> RawSlab<K, V> {
        let slab = self.slabs[id as usize].take().expect("live slab");
        self.free_slabs.push(id);
        slab
    }

    fn alloc_unrefined(&mut self, node: UnrefinedNode) -> u32 {
        if let Some(id) = self.free_unrefineds.pop() {
            self.unrefineds[id as usize] = Some(node);
            id
        } else {
            let id = u32::try_from(self.unrefineds.len()).expect("unrefined handle overflow");
            assert!(id < INDEX_MASK, "unrefined handle capacity exceeded");
            self.unrefineds.push(Some(node));
            id
        }
    }

    fn take_unrefined(&mut self, id: u32) -> UnrefinedNode {
        let u = self.unrefineds[id as usize].take().expect("live unrefined");
        self.free_unrefineds.push(id);
        u
    }

    fn alloc_slab_unrefined(&mut self, records: Vec<(K, V)>, candidate_mask: u128) -> u32 {
        let end = records.len();
        let slab_id = self.alloc_slab(RawSlab::from_vec(records));
        self.alloc_unrefined(UnrefinedNode {
            slab_id,
            start: 0,
            end,
            candidate_mask,
        })
    }

    fn merge_unrefined(
        &mut self,
        old_uid: u32,
        extra: impl ExactSizeIterator<Item = (K, V)>,
        candidate_mask: u128,
    ) -> (u32, usize) {
        let old_len = self.unrefineds[old_uid as usize]
            .as_ref()
            .expect("live unrefined")
            .len();
        let mut combined = Vec::with_capacity(old_len + extra.len());
        let old_u = self.take_unrefined(old_uid);
        self.drain_slab_range(old_u.slab_id, old_u.start, old_u.end, &mut combined);
        combined.extend(extra);
        let combined_len = combined.len();
        let uid = self.alloc_slab_unrefined(combined, candidate_mask);
        (uid, combined_len)
    }

    fn has_unrefined(&self) -> bool {
        self.unrefineds.iter().any(Option::is_some)
    }

    fn clear_if_converged(&mut self, has_unverified_dups: &AtomicBool) {
        if !self.has_unrefined() {
            has_unverified_dups.store(false, Ordering::Release);
            self.unrefineds.clear();
            self.free_unrefineds.clear();
            self.workspace = Workspace::new();
            if self.slabs.iter().all(Option::is_none) {
                self.slabs.clear();
                self.free_slabs.clear();
            }
        }
    }

    fn drain_slab_range(&mut self, slab_id: u32, start: usize, end: usize, out: &mut Vec<(K, V)>) {
        let should_free = {
            let slab = self.slabs[slab_id as usize].as_mut().unwrap();
            for slot in &mut slab.data[start..end] {
                // SAFETY: caller exclusively owns initialized range `start..end`.
                out.push(unsafe { slot.assume_init_read() });
            }
            slab.live_count -= end - start;
            slab.live_count == 0
        };
        if should_free {
            self.take_slab(slab_id);
        }
    }

    /// Adaptive slab compaction: frees sparse slabs once utilization drops below 25%.
    fn maybe_compact_slab(&mut self, slab_id: u32) {
        let (total_cap, live_count) = match self.slabs[slab_id as usize].as_ref() {
            Some(s) => (s.data.capacity(), s.live_count),
            None => return,
        };
        if live_count == 0 {
            self.take_slab(slab_id);
            return;
        }
        if total_cap < COMPACT_MIN_SLAB_LEN || live_count * 4 >= total_cap {
            return;
        }

        // Pre-reserve before mutating any slab or descriptor so OOM leaves state intact.
        let mut packed: Vec<(K, V)> = Vec::with_capacity(live_count);
        {
            let old_slab = self.slabs[slab_id as usize].as_mut().unwrap();
            for u in self.unrefineds.iter_mut().flatten() {
                if u.slab_id == slab_id {
                    let new_start = packed.len();
                    for slot in &mut old_slab.data[u.start..u.end] {
                        // SAFETY: live `UnrefinedNode` exclusively owns initialized `u.start..u.end`.
                        packed.push(unsafe { slot.assume_init_read() });
                    }
                    u.start = new_start;
                    u.end = packed.len();
                }
            }
        }
        debug_assert_eq!(packed.len(), live_count);
        self.slabs[slab_id as usize] = Some(RawSlab::from_vec(packed));
    }
}

impl<K, V> Drop for RefineState<K, V> {
    fn drop(&mut self) {
        fn drop_remaining<K, V>(state: &mut RefineState<K, V>) {
            for u in state.unrefineds.iter_mut().flatten() {
                if let Some(slab) = state
                    .slabs
                    .get_mut(u.slab_id as usize)
                    .and_then(Option::as_mut)
                {
                    while u.start < u.end {
                        let idx = u.start;
                        u.start += 1;
                        // SAFETY: live `UnrefinedNode` exclusively owns initialized slot `idx`.
                        unsafe { slab.data[idx].assume_init_drop() };
                    }
                }
            }
        }

        struct Guard<'a, K, V>(&'a mut RefineState<K, V>);
        impl<K, V> Drop for Guard<'_, K, V> {
            fn drop(&mut self) {
                drop_remaining(self.0);
            }
        }

        let guard = Guard(self);
        drop_remaining(guard.0);
        mem::forget(guard);
    }
}

/// Stable-address pools holding published leaves, internal nodes, and radix directories.
///
/// Separated from `Mutex<RefineState>` so `&mut self` paths can borrow `pools` and
/// `refine.get_mut()` disjointly without acquiring the mutex.
struct NodePools<E: Unsigned, V> {
    leaves: SharedPool<Leaf<E, V>>,
    internals: SharedPool<Internal<E>>,
    radixes: SharedPool<RadixNode<E>>,
}

impl<E: Unsigned, V> NodePools<E, V> {
    fn new() -> Self {
        Self {
            leaves: SharedPool::new(),
            internals: SharedPool::new(),
            radixes: SharedPool::new(),
        }
    }

    #[inline(always)]
    fn mask_below(bits: usize) -> u128 {
        if bits >= 128 {
            u128::MAX
        } else if bits == 0 {
            0
        } else {
            (1u128 << bits) - 1
        }
    }

    fn alloc_radix_node<K>(
        &self,
        state: &mut RefineState<K, V>,
        sample_key_u128: u128,
        shift: usize,
        width: usize,
        children: Vec<NodeId>,
        counts: Vec<usize>,
    ) -> NodeId {
        let prefix_shift = (shift + width) as u8;
        let prefix_base = if (prefix_shift as usize) >= E::BITS {
            E::from_usize(0)
        } else {
            E::from_u128(sample_key_u128 & !Self::mask_below(prefix_shift as usize))
        };
        let rx = RadixNode::new(
            prefix_base,
            prefix_shift,
            shift as u8,
            width as u8,
            children,
            counts,
        );
        // SAFETY: `state` serializes all allocations into `self.radixes`.
        let rx_id = unsafe {
            self.radixes.alloc_with(&mut state.radix_alloc, |slot| {
                slot.write(rx);
            })
        };
        NodeId::radix(rx_id)
    }

    #[inline(always)]
    fn load_slot_acquire(&self, root: &AtomicNodeId, parent: Option<PathStep>) -> NodeId {
        match parent {
            None => root.load_acquire(),
            Some(step) => match step.node.tag() {
                TAG_INTERNAL => {
                    // SAFETY: `step.node` is a published internal handle obtained via `Acquire`.
                    unsafe { self.internals.get(step.node.index()) }.children[step.slot]
                        .load_acquire()
                }
                TAG_RADIX => {
                    // SAFETY: `step.node` is a published radix handle obtained via `Acquire`.
                    unsafe { self.radixes.get(step.node.index()) }.children[step.slot]
                        .load_acquire()
                }
                _ => unreachable!(),
            },
        }
    }

    #[inline(always)]
    fn publish_slot_release(
        &self,
        root: &AtomicNodeId,
        path: &PathStack,
        child: NodeId,
        dropped: usize,
    ) {
        if dropped > 0 {
            path.for_each(|step| {
                if step.node.tag() == TAG_RADIX {
                    // SAFETY: `step.node` is a published radix handle on the active descent path.
                    let rx = unsafe { self.radixes.get(step.node.index()) };
                    let prev = rx.counts[step.slot].load(Ordering::Relaxed);
                    rx.counts[step.slot].store(prev.saturating_sub(dropped), Ordering::Release);
                }
            });
        }
        match path.last() {
            None => root.store_release(child),
            Some(step) => match step.node.tag() {
                TAG_INTERNAL => {
                    // SAFETY: `step.node` is a published internal handle on the active descent path.
                    unsafe { self.internals.get(step.node.index()) }.children[step.slot]
                        .store_release(child);
                }
                TAG_RADIX => {
                    // SAFETY: `step.node` is a published radix handle on the active descent path.
                    unsafe { self.radixes.get(step.node.index()) }.children[step.slot]
                        .store_release(child);
                }
                _ => unreachable!(),
            },
        }
    }

    fn subtree_count_locked<K>(&self, state: &RefineState<K, V>, node: NodeId) -> usize {
        if node.is_empty() {
            return 0;
        }
        match node.tag() {
            // SAFETY: `node` is a live published leaf handle under `state`.
            TAG_LEAF => unsafe { self.leaves.get(node.index()) }.len(),
            TAG_UNREFINED => state.unrefineds[node.index() as usize]
                .as_ref()
                .map_or(0, UnrefinedNode::len),
            TAG_RADIX => {
                // SAFETY: `node` is a live published radix handle under `state`.
                let rx = unsafe { self.radixes.get(node.index()) };
                rx.counts.iter().map(|c| c.load(Ordering::Relaxed)).sum()
            }
            TAG_INTERNAL => {
                // SAFETY: `node` is a live published internal handle under `state`.
                let internal = unsafe { self.internals.get(node.index()) };
                internal.children[..internal.len()]
                    .iter()
                    .map(|c| self.subtree_count_locked(state, c.load_relaxed()))
                    .sum()
            }
            _ => unreachable!(),
        }
    }

    /// Resolves all unrefined buckets under `node`, publishing every refined child immediately
    /// and appending overwritten duplicate entries to `discarded` so callers drop them after
    /// releasing the refinement mutex.
    fn finish_subtree_locked<K: RadixKey<Encoded = E>>(
        &self,
        state: &mut RefineState<K, V>,
        len_atom: &AtomicUsize,
        node: NodeId,
        discarded: &mut Vec<(K, V)>,
    ) -> NodeId {
        if node.is_empty() {
            return node;
        }
        match node.tag() {
            TAG_LEAF => node,
            TAG_UNREFINED => {
                let u = state.take_unrefined(node.index());
                let (nid, dropped) = self.materialize_unrefined_locked(state, u, discarded);
                if dropped > 0 {
                    len_atom.fetch_sub(dropped, Ordering::Release);
                }
                nid
            }
            TAG_RADIX => {
                // SAFETY: `node` is a live published radix handle under `state`.
                let rx = unsafe { self.radixes.get(node.index()) };
                for (child_slot, count_slot) in rx.children.iter().zip(rx.counts.iter()) {
                    let c = child_slot.load_acquire();
                    if !c.is_empty() && c.tag() != TAG_LEAF {
                        let nc = self.finish_subtree_locked(state, len_atom, c, discarded);
                        let new_cnt = self.subtree_count_locked(state, nc);
                        count_slot.store(new_cnt, Ordering::Release);
                        child_slot.store_release(nc);
                    }
                }
                node
            }
            TAG_INTERNAL => {
                // SAFETY: `node` is a live published internal handle under `state`.
                let internal = unsafe { self.internals.get(node.index()) };
                for child_slot in &internal.children[..internal.len()] {
                    let c = child_slot.load_acquire();
                    if !c.is_empty() && c.tag() != TAG_LEAF {
                        let nc = self.finish_subtree_locked(state, len_atom, c, discarded);
                        child_slot.store_release(nc);
                    }
                }
                node
            }
            _ => unreachable!(),
        }
    }

    /// Performs one step of Tessera refinement while holding `state`.
    /// Never runs user `V::drop`; any overwritten duplicate entries are appended to `discarded`.
    fn crack_unrefined_locked<K: RadixKey<Encoded = E>>(
        &self,
        state: &mut RefineState<K, V>,
        uid: u32,
        discarded: &mut Vec<(K, V)>,
    ) -> (NodeId, usize) {
        let u = state.take_unrefined(uid);
        let len = u.len();
        if len == 0 {
            return (NodeId::EMPTY, 0);
        }
        if len <= TILE_REFINE_THRESHOLD {
            return self.materialize_unrefined_locked(state, u, discarded);
        }

        let (varying, sample_key) = {
            let slab = state.slabs[u.slab_id as usize].as_ref().unwrap();
            // SAFETY: `u` exclusively owns initialized `u.start..u.end` inside `state`.
            let slice = unsafe { slab.slice(u.start, u.end) };
            let mut ext = ByKey(|r: &(K, V)| r.0);
            (
                scan_varying_bits(slice, u.candidate_mask, &mut ext),
                slice[0].0.encode(),
            )
        };
        if varying == 0 {
            return self.materialize_unrefined_locked(state, u, discarded);
        }

        let top = bit_len(varying);
        let desired_bits = (((len / 256).max(2) - 1).ilog2() + 1) as usize;
        let width = desired_bits.clamp(4, CRACK_MAX_BITS).min(top).max(1);
        let digit = Digit {
            shift: top - width,
            width,
        };

        let ends = {
            let slab = state.slabs[u.slab_id as usize].as_mut().unwrap();
            // SAFETY: `u` exclusively owns initialized `u.start..u.end`.
            let slice = unsafe { slab.slice_mut(u.start, u.end) };
            partition_by_digit(slice, digit.shift, digit.width, &mut state.workspace)
        };

        let num_buckets = 1usize << digit.width;
        let mut children = alloc::vec![NodeId::EMPTY; num_buckets];
        let mut counts = alloc::vec![0usize; num_buckets];
        let child_candidate = varying & !digit.mask::<E>().to_u128();

        let mut prev = 0usize;
        for (b, &end) in ends.iter().enumerate() {
            if end > prev {
                counts[b] = end - prev;
                let cid = state.alloc_unrefined(UnrefinedNode {
                    slab_id: u.slab_id,
                    start: u.start + prev,
                    end: u.start + end,
                    candidate_mask: child_candidate,
                });
                children[b] = NodeId::unrefined(cid);
            }
            prev = end;
        }

        let rx_id = self.alloc_radix_node(
            state,
            sample_key.to_u128(),
            digit.shift,
            digit.width,
            children,
            counts,
        );
        (rx_id, 0)
    }

    #[allow(clippy::too_many_arguments)]
    fn crack_and_publish<K: RadixKey<Encoded = E>>(
        &self,
        root: &AtomicNodeId,
        len_atom: &AtomicUsize,
        has_unverified_dups: &AtomicBool,
        state: &mut RefineState<K, V>,
        path: &PathStack,
        uid: u32,
        discarded: &mut Vec<(K, V)>,
    ) -> NodeId {
        let (refined, dropped) = self.crack_unrefined_locked(state, uid, discarded);
        if dropped > 0 {
            len_atom.fetch_sub(dropped, Ordering::Release);
        }
        self.publish_slot_release(root, path, refined, dropped);
        state.clear_if_converged(has_unverified_dups);
        refined
    }

    fn materialize_unrefined_locked<K: RadixKey<Encoded = E>>(
        &self,
        state: &mut RefineState<K, V>,
        u: UnrefinedNode,
        discarded: &mut Vec<(K, V)>,
    ) -> (NodeId, usize) {
        let (len, s, e, slab_id) = (u.len(), u.start, u.end, u.slab_id);

        // Zero-copy fast path when `u` owns the entire backing slab.
        let mut records =
            if s == 0 && len == state.slabs[slab_id as usize].as_ref().unwrap().data.len() {
                let slab = state.take_slab(slab_id);
                // SAFETY: `u` exclusively owns every initialized slot `0..len` in `slab`.
                let mut records = unsafe { slab.into_vec() };
                if len > 1 {
                    engine_sort(&mut records, ByKey(|r: &(K, V)| r.0));
                }
                records
            } else {
                {
                    let slab = state.slabs[slab_id as usize].as_mut().unwrap();
                    // SAFETY: `u` exclusively owns initialized `u.start..u.end`.
                    let slice = unsafe { slab.slice_mut(u.start, u.end) };
                    sort_slice(slice, u.candidate_mask, &mut state.workspace);
                }
                let mut drained: Vec<(K, V)> = Vec::with_capacity(len);
                {
                    let slab = state.slabs[slab_id as usize].as_mut().unwrap();
                    for slot in &mut slab.data[s..e] {
                        // SAFETY: `u` exclusively owns initialized `s..e`, moving ownership out once.
                        drained.push(unsafe { slot.assume_init_read() });
                    }
                    slab.live_count -= len;
                }
                state.maybe_compact_slab(slab_id);
                drained
            };

        let kept = dedup_sorted_last_wins_partition(&mut records);
        let dropped = len - kept;
        if dropped > 0 {
            discarded.reserve(dropped);
            discarded.extend(records.drain(kept..));
        }
        let node = self.build_bottom_up_sorted(state, records);
        (node, dropped)
    }

    fn build_btree_from_iter<K, I>(
        &self,
        state: &mut RefineState<K, V>,
        total: usize,
        mut iter: I,
    ) -> NodeId
    where
        I: Iterator<Item = (E, V)>,
    {
        if total == 0 {
            return NodeId::EMPTY;
        }
        if total <= LEAF_CAP {
            // SAFETY: `state` serializes allocations into `self.leaves`.
            let leaf_id = unsafe {
                self.leaves.alloc_with(&mut state.leaf_alloc, |slot| {
                    Leaf::init_from_sorted(slot, total, iter)
                })
            };
            return NodeId::leaf(leaf_id);
        }

        let num_leaves = total.div_ceil(BULK_LEAF_FILL);
        let (base_per, extra) = (total / num_leaves, total % num_leaves);
        let mut level: Vec<(NodeId, E)> = Vec::with_capacity(num_leaves);

        for i in 0..num_leaves {
            let n = base_per + usize::from(i < extra);
            // SAFETY: `state` serializes allocations into `self.leaves`.
            let leaf_id = unsafe {
                self.leaves.alloc_with(&mut state.leaf_alloc, |slot| {
                    Leaf::init_from_sorted(slot, n, iter.by_ref().take(n))
                })
            };
            // SAFETY: `leaf_id` was just initialized in `self.leaves` by this thread.
            let max_k = unsafe { self.leaves.get(leaf_id) }.max_key();
            level.push((NodeId::leaf(leaf_id), max_k));
        }

        while level.len() > 1 {
            let next_count = level.len().div_ceil(INTERNAL_CAP);
            let mut next_level = Vec::with_capacity(next_count);
            let (base, rem) = (level.len() / next_count, level.len() % next_count);
            let mut offset = 0;
            for i in 0..next_count {
                let n = base + usize::from(i < rem);
                let chunk = &level[offset..offset + n];
                // SAFETY: `state` serializes allocations into `self.internals`.
                let int_id = unsafe {
                    self.internals
                        .alloc_with(&mut state.internal_alloc, |slot| {
                            Internal::init_from_children(slot, chunk)
                        })
                };
                next_level.push((NodeId::internal(int_id), chunk[n - 1].1));
                offset += n;
            }
            level = next_level;
        }

        level[0].0
    }

    fn build_bottom_up_sorted<K: RadixKey<Encoded = E>>(
        &self,
        state: &mut RefineState<K, V>,
        records: Vec<(K, V)>,
    ) -> NodeId {
        let n = records.len();
        if n == 0 {
            return NodeId::EMPTY;
        }
        let min_k = records[0].0.encode().to_u128();
        let diff = min_k ^ records[n - 1].0.encode().to_u128();
        if n < 4096 || diff == 0 {
            return self.build_btree_from_iter(
                state,
                n,
                records.into_iter().map(|(k, v)| (k.encode(), v)),
            );
        }

        let top = bit_len(diff);
        let desired_buckets = (n / 1000).clamp(16, 4096);
        let width = (((desired_buckets - 1).ilog2() + 1) as usize)
            .clamp(4, 12)
            .min(top);
        let shift = top - width;
        let num_buckets = 1usize << width;
        let mask = (1u128 << width) - 1;

        let mut counts = alloc::vec![0usize; num_buckets];
        for (k, _) in &records {
            counts[((k.encode().to_u128() >> shift) & mask) as usize] += 1;
        }

        let mut children = alloc::vec![NodeId::EMPTY; num_buckets];
        let mut iter = records.into_iter().map(|(k, v)| (k.encode(), v));
        for (child_slot, &cnt) in children.iter_mut().zip(&counts) {
            if cnt > 0 {
                *child_slot = self.build_btree_from_iter(state, cnt, iter.by_ref().take(cnt));
            }
        }

        self.alloc_radix_node(state, min_k, shift, width, children, counts)
    }
}

/// Ordered map keyed by any Tessera [`RadixKey`].
///
/// Combines `O(1)` lazy adoption of large unsorted batches, transparent on-demand radix
/// refinement during `&self` reads, gapped `fearless_simd` leaves, and streaming batch
/// ingestion behind one `Send + Sync` ordered-map interface.
///
/// Ordering, duplicate resolution, lazy refinement, reference stability, and unwind guarantees
/// are specified in `conformance/semantics.md`; internal pool and slab ownership boundaries are
/// documented in `docs/design/engine.md`.
///
/// # Examples
///
/// ```
/// use tessera_order::TesseraMap;
///
/// let mut map = TesseraMap::from(vec![(30u64, "c"), (10, "a"), (20, "b")]);
/// assert_eq!(map.get(20), Some(&"b"));
/// assert_eq!(map.select_nth(0), Some((10, &"a")));
/// map.insert(15, "ab");
/// let mut items = Vec::new();
/// map.range(10..=20, |k, &v| items.push((k, v)));
/// assert_eq!(items, vec![(10, "a"), (15, "ab"), (20, "b")]);
/// ```
pub struct TesseraMap<K: RadixKey, V> {
    root: AtomicNodeId,
    len: AtomicUsize,
    has_unverified_dups: AtomicBool,
    level: Level,
    pools: NodePools<K::Encoded, V>,
    refine: Mutex<RefineState<K, V>>,
    _marker: PhantomData<K>,
}

// SAFETY: `TesseraMap<K, V>` owns all nodes and slabs exclusively; `K: RadixKey` is `Send + Sync`,
// so transferring ownership across threads is sound whenever `V: Send`.
unsafe impl<K: RadixKey, V: Send> Send for TesseraMap<K, V> {}
// SAFETY: Concurrent `&self` readers access published immutable nodes lock-free via `Acquire`
// loads (`V: Sync`) and serialize lazy refinement of unpublished slabs under `self.refine`
// (`V: Send`). Published leaves never move or mutate during `&self`.
unsafe impl<K: RadixKey, V: Send + Sync> Sync for TesseraMap<K, V> {}

impl<K: RadixKey, V> Default for TesseraMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: RadixKey, V> From<Vec<(K, V)>> for TesseraMap<K, V> {
    fn from(records: Vec<(K, V)>) -> Self {
        Self::from_vec_with_threshold(records, true, LAZY_THRESHOLD)
    }
}

impl<K: RadixKey, V> FromIterator<(K, V)> for TesseraMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self::from(iter.into_iter().collect::<Vec<_>>())
    }
}

impl<K: RadixKey, V> Extend<(K, V)> for TesseraMap<K, V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        self.extend_vec(iter.into_iter().collect());
    }
}

impl<K: RadixKey, V> TesseraMap<K, V> {
    /// Creates an empty map without allocating.
    pub fn new() -> Self {
        Self {
            root: AtomicNodeId::new(NodeId::EMPTY),
            len: AtomicUsize::new(0),
            has_unverified_dups: AtomicBool::new(false),
            level: Level::new(),
            pools: NodePools::new(),
            refine: Mutex::new(RefineState::new()),
            _marker: PhantomData,
        }
    }

    /// Creates a map from unsorted `(K, V)` pairs whose keys are expected to be distinct.
    ///
    /// Unlike [`From<Vec<(K, V)>>`], assuming distinct keys allows [`Self::len`] and
    /// [`Self::select_nth`] to answer lazily without scanning untouched buckets. If duplicate
    /// keys are present anyway, refinement still deduplicates them safely (last value wins),
    /// though untouched buckets report raw counts until refined.
    pub fn from_unique(records: Vec<(K, V)>) -> Self {
        Self::from_vec_with_threshold(records, false, LAZY_THRESHOLD)
    }

    #[cfg(test)]
    pub(crate) fn from_vec_lazy_for_test(records: Vec<(K, V)>, may_have_dups: bool) -> Self {
        Self::from_vec_with_threshold(records, may_have_dups, 1)
    }

    fn from_vec_with_threshold(
        mut records: Vec<(K, V)>,
        may_have_dups: bool,
        lazy_threshold: usize,
    ) -> Self {
        let len = records.len();
        if len == 0 {
            return Self::new();
        }
        let mut map = Self::new();
        if len < lazy_threshold {
            engine_sort(&mut records, ByKey(|r: &(K, V)| r.0));
            let kept = dedup_sorted_last_wins_partition(&mut records);
            records.truncate(kept);
            let n = records.len();
            let (pools, state) = map.parts_mut();
            let root = pools.build_bottom_up_sorted(state, records);
            map.root.set_mut(root);
            *map.len.get_mut() = n;
            return map;
        }

        let uid = map
            .refine_mut()
            .alloc_slab_unrefined(records, K::VARYING_MASK);
        map.root.set_mut(NodeId::unrefined(uid));
        *map.len.get_mut() = len;
        *map.has_unverified_dups.get_mut() = may_have_dups;
        map
    }

    /// Number of entries in the map.
    ///
    /// If constructed from an unsorted batch with unverified duplicate keys, resolves
    /// remaining unrefined buckets once so the returned length is always exact.
    #[inline]
    pub fn len(&self) -> usize {
        if self.has_unverified_dups.load(Ordering::Acquire) {
            self.resolve_unverified_dups();
        }
        self.len.load(Ordering::Acquire)
    }

    /// Returns `true` if the map contains no entries, in `O(1)` without refining unrefined buckets.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.root.load_acquire().is_empty()
    }

    /// Removes all entries and releases all internal storage.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Returns provisional internal structural diagnostics without refining unrefined buckets.
    pub fn stats(&self) -> MapStats {
        let state = self.lock_refine();
        let mut unrefined = 0usize;
        let mut unrefined_records = 0usize;
        for u in state.unrefineds.iter().flatten() {
            unrefined += 1;
            unrefined_records += u.len();
        }
        MapStats {
            len: self.len.load(Ordering::Acquire),
            leaves: state.leaf_alloc.live_count,
            internals: state.internal_alloc.live_count,
            radixes: state.radix_alloc.live_count,
            unrefined,
            live_slabs: state.slabs.iter().filter(|s| s.is_some()).count(),
            unrefined_records,
        }
    }

    #[inline]
    fn lock_refine(&self) -> std::sync::MutexGuard<'_, RefineState<K, V>> {
        self.refine.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[inline]
    fn refine_mut(&mut self) -> &mut RefineState<K, V> {
        self.refine
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
    }

    #[inline]
    fn parts_mut(&mut self) -> (&mut NodePools<K::Encoded, V>, &mut RefineState<K, V>) {
        (
            &mut self.pools,
            self.refine
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }

    // -----------------------------------------------------------------------
    // Concurrent lock-free `&self` reads with transparent lazy refinement
    // -----------------------------------------------------------------------

    #[inline(always)]
    fn get_with<S: Simd>(&self, simd: S, ek: K::Encoded) -> Option<&V> {
        let mut cur = self.root.load_acquire();
        loop {
            if cur.is_empty() {
                return None;
            }
            match cur.tag() {
                TAG_LEAF => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    return unsafe { self.pools.leaves.get(cur.index()) }.get(simd, ek);
                }
                TAG_INTERNAL => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let internal = unsafe { self.pools.internals.get(cur.index()) };
                    let slot = internal.find_child_idx(simd, ek);
                    cur = internal.children[slot].load_acquire();
                }
                TAG_RADIX => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let rx = unsafe { self.pools.radixes.get(cur.index()) };
                    if !rx.contains_prefix(ek) {
                        return None;
                    }
                    let slot = rx.bucket_of(ek);
                    // SAFETY: `slot < (1 << rx.width) == rx.children.len()`.
                    cur = unsafe { rx.children.get_unchecked(slot).load_acquire() };
                }
                TAG_UNREFINED => return self.get_cold_crack(simd, ek),
                _ => unreachable!(),
            }
        }
    }

    /// Cold path for `get`: tracks the full ancestor `PathStack` so duplicate drops during
    /// cracking propagate cleanly up every ancestor `RadixNode`.
    #[cold]
    #[inline(never)]
    fn get_cold_crack<S: Simd>(&self, simd: S, ek: K::Encoded) -> Option<&V> {
        let mut cur = self.root.load_acquire();
        let mut path = PathStack::new();
        loop {
            if cur.is_empty() {
                return None;
            }
            match cur.tag() {
                TAG_LEAF => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    return unsafe { self.pools.leaves.get(cur.index()) }.get(simd, ek);
                }
                TAG_INTERNAL => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let internal = unsafe { self.pools.internals.get(cur.index()) };
                    let slot = internal.find_child_idx(simd, ek);
                    path.push(PathStep { node: cur, slot });
                    cur = internal.children[slot].load_acquire();
                }
                TAG_RADIX => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let rx = unsafe { self.pools.radixes.get(cur.index()) };
                    if !rx.contains_prefix(ek) {
                        return None;
                    }
                    let slot = rx.bucket_of(ek);
                    path.push(PathStep { node: cur, slot });
                    // SAFETY: `slot < (1 << rx.width) == rx.children.len()`.
                    cur = unsafe { rx.children.get_unchecked(slot).load_acquire() };
                }
                TAG_UNREFINED => cur = self.crack_slot_shared(&path, cur),
                _ => unreachable!(),
            }
        }
    }

    /// Returns a reference to the value corresponding to `key`, if present.
    ///
    /// Lock-free on already-refined paths; transparently refines untouched buckets on demand.
    #[inline]
    pub fn get(&self, key: K) -> Option<&V> {
        let ek = key.encode();
        dispatch!(self.level, simd => self.get_with(simd, ek))
    }

    /// Returns `true` if the map contains an entry for `key`.
    #[inline]
    pub fn contains_key(&self, key: K) -> bool {
        self.get(key).is_some()
    }

    /// Visits all `(K, &V)` pairs within `bounds` in ascending key order.
    ///
    /// Lazily refines only the unrefined buckets whose key range overlaps `bounds`.
    pub fn range(&self, bounds: impl RangeBounds<K>, mut f: impl FnMut(K, &V)) {
        let Some((lo, hi)) = encoded_inclusive_bounds(&bounds) else {
            return;
        };
        let root = self.root.load_acquire();
        if root.is_empty() {
            return;
        }
        let mut path = PathStack::new();
        let mut visitor = |ek: K::Encoded, v: &V| f(K::decode(ek), v);
        dispatch!(self.level, simd => {
            self.range_shared(simd, &mut path, root, lo, hi, &mut visitor);
        });
    }

    fn endpoint(&self, last: bool) -> Option<(K, &V)> {
        let mut cur = self.root.load_acquire();
        let mut path = PathStack::new();
        loop {
            if cur.is_empty() {
                return None;
            }
            match cur.tag() {
                TAG_LEAF => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let leaf = unsafe { self.pools.leaves.get(cur.index()) };
                    let pair = if last { leaf.last() } else { leaf.first() };
                    return pair.map(|(k, v)| (K::decode(k), v));
                }
                TAG_INTERNAL => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let internal = unsafe { self.pools.internals.get(cur.index()) };
                    let slot = if last { internal.len() - 1 } else { 0 };
                    path.push(PathStep { node: cur, slot });
                    cur = internal.children[slot].load_acquire();
                }
                TAG_RADIX => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let rx = unsafe { self.pools.radixes.get(cur.index()) };
                    let next = if last {
                        rx.children.iter().enumerate().rev().find_map(|(i, c)| {
                            let id = c.load_acquire();
                            (!id.is_empty()).then_some((i, id))
                        })
                    } else {
                        rx.children.iter().enumerate().find_map(|(i, c)| {
                            let id = c.load_acquire();
                            (!id.is_empty()).then_some((i, id))
                        })
                    };
                    let (slot, id) = next?;
                    path.push(PathStep { node: cur, slot });
                    cur = id;
                }
                TAG_UNREFINED => cur = self.crack_slot_shared(&path, cur),
                _ => unreachable!(),
            }
        }
    }

    /// Returns the smallest `(K, &V)` pair in the map, if non-empty.
    pub fn first(&self) -> Option<(K, &V)> {
        self.endpoint(false)
    }

    /// Returns the largest `(K, &V)` pair in the map, if non-empty.
    pub fn last(&self) -> Option<(K, &V)> {
        self.endpoint(true)
    }

    /// Returns the `rank`-th smallest `(K, &V)` pair (`0 <= rank < self.len()`).
    ///
    /// On maps created via [`Self::from_unique`] (or already deduplicated), lazily refines
    /// only the buckets containing `rank` and retains cracked structure for future queries.
    pub fn select_nth(&self, rank: usize) -> Option<(K, &V)> {
        if self.has_unverified_dups.load(Ordering::Acquire) {
            self.resolve_unverified_dups();
        }
        if rank >= self.len.load(Ordering::Acquire) {
            return None;
        }
        let mut cur = self.root.load_acquire();
        let mut path = PathStack::new();
        let mut rem = rank;

        loop {
            if cur.is_empty() {
                return None;
            }
            match cur.tag() {
                TAG_LEAF => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let (ek, v) = unsafe { self.pools.leaves.get(cur.index()) }.nth(rem)?;
                    return Some((K::decode(ek), v));
                }
                TAG_UNREFINED => cur = self.crack_slot_shared(&path, cur),
                TAG_RADIX => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let rx = unsafe { self.pools.radixes.get(cur.index()) };
                    let mut chosen = None;
                    for (slot, cnt) in rx.counts.iter().enumerate() {
                        let c = cnt.load(Ordering::Acquire);
                        if rem < c {
                            chosen = Some((slot, rx.children[slot].load_acquire()));
                            break;
                        }
                        rem -= c;
                    }
                    let (slot, next) = chosen?;
                    path.push(PathStep { node: cur, slot });
                    cur = next;
                }
                TAG_INTERNAL => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let internal = unsafe { self.pools.internals.get(cur.index()) };
                    let mut chosen = None;
                    for (slot, child_slot) in internal.children[..internal.len()].iter().enumerate()
                    {
                        let child = child_slot.load_acquire();
                        let c =
                            self.subtree_count_shared(Some(PathStep { node: cur, slot }), child);
                        if rem < c {
                            chosen = Some((slot, child));
                            break;
                        }
                        rem -= c;
                    }
                    let (slot, next) = chosen?;
                    path.push(PathStep { node: cur, slot });
                    cur = next;
                }
                _ => unreachable!(),
            }
        }
    }

    // -----------------------------------------------------------------------
    // Exclusive `&mut self` mutations (`insert`, `remove`, `extend_vec`)
    // -----------------------------------------------------------------------

    /// Inserts `(key, val)` into the map, returning the previous value if `key` existed.
    #[inline]
    pub fn insert(&mut self, key: K, val: V) -> Option<V> {
        let ek = key.encode();
        let root = self.root.get_mut();
        let (new_root, old) =
            dispatch!(self.level, simd => self.insert_into(simd, root, None, ek, val));
        self.root.set_mut(new_root);
        old
    }

    /// Removes `key` from the map, returning its value if present.
    #[inline]
    pub fn remove(&mut self, key: K) -> Option<V> {
        let ek = key.encode();
        dispatch!(self.level, simd => self.remove_with(simd, ek))
    }

    /// Merges an unsorted batch of `(K, V)` pairs using Tessera's `block::partition` for
    /// single-pass top-digit routing before cache-local sorted insertion.
    pub fn extend_vec(&mut self, mut batch: Vec<(K, V)>) {
        if batch.is_empty() {
            return;
        }
        let root = self.root.get_mut();
        if root.is_empty() {
            *self = Self::from(batch);
            return;
        }

        if root.tag() == TAG_RADIX {
            let rx_idx = root.index();
            let (prefix_base, prefix_shift, shift, width) = {
                // SAFETY: `root` is a live radix handle under `&mut self`.
                let rx = unsafe { self.pools.radixes.get(rx_idx) };
                (rx.prefix_base, rx.prefix_shift, rx.shift, rx.width)
            };

            let all_in_prefix = prefix_shift as usize >= K::Encoded::BITS
                || batch.iter().all(|(k, _)| {
                    ((k.encode().to_u128() ^ prefix_base.to_u128()) >> prefix_shift) == 0
                });

            if all_in_prefix && batch.len() >= 256 && (width as usize) <= MAX_RADIX_BITS {
                let ends = partition_by_digit(
                    &mut batch,
                    shift as usize,
                    width as usize,
                    &mut self.refine_mut().workspace,
                );
                let full_bucket_cand =
                    NodePools::<K::Encoded, V>::mask_below(shift as usize) & K::VARYING_MASK;
                let rx_node = NodeId::radix(rx_idx);

                // Consume buckets from the tail of `batch` via `Vec::drain` / `split_off` so the
                // active batch stays locally owned (never inside `state.slabs` while `insert_into`
                // may refine and reclaim existing slabs) and `Vec` handles unwind cleanup safely.
                dispatch!(self.level, simd => {
                    for b in (0..ends.len()).rev() {
                        let start = if b == 0 { 0 } else { ends[b - 1] };
                        let end = ends[b];
                        let count = end - start;
                        debug_assert_eq!(batch.len(), end);
                        if count == 0 {
                            continue;
                        }
                        // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
                        let child =
                            unsafe { self.pools.radixes.get(rx_idx) }.children[b].load_relaxed();
                        if child.is_empty() {
                            let bucket_vec = if start == 0 {
                                mem::take(&mut batch)
                            } else {
                                batch.split_off(start)
                            };
                            let uid = self
                                .refine_mut()
                                .alloc_slab_unrefined(bucket_vec, full_bucket_cand);
                            // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
                            let rx = unsafe { self.pools.radixes.get_mut(rx_idx) };
                            rx.children[b].set_mut(NodeId::unrefined(uid));
                            *rx.counts[b].get_mut() += count;
                            *self.len.get_mut() += count;
                            *self.has_unverified_dups.get_mut() = true;
                        } else if child.tag() == TAG_UNREFINED {
                            let (uid, combined_len) = self.refine_mut().merge_unrefined(
                                child.index(),
                                batch.drain(start..),
                                full_bucket_cand,
                            );
                            // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
                            let rx = unsafe { self.pools.radixes.get_mut(rx_idx) };
                            rx.children[b].set_mut(NodeId::unrefined(uid));
                            *rx.counts[b].get_mut() = combined_len;
                            *self.len.get_mut() += count;
                            *self.has_unverified_dups.get_mut() = true;
                        } else {
                            if count > 16 {
                                sort_slice(
                                    &mut batch[start..end],
                                    full_bucket_cand,
                                    &mut self.refine_mut().workspace,
                                );
                            }
                            let step = Some(PathStep {
                                node: rx_node,
                                slot: b,
                            });
                            for (k, v) in batch.drain(start..) {
                                // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
                                let cur_child = unsafe { self.pools.radixes.get(rx_idx) }
                                    .children[b]
                                    .load_relaxed();
                                self.insert_into(simd, cur_child, step, k.encode(), v);
                            }
                        }
                    }
                });
                return;
            }
        }

        if batch.len() > 64 {
            engine_sort(&mut batch, ByKey(|r: &(K, V)| r.0));
        }
        dispatch!(self.level, simd => {
            for (k, v) in batch {
                let r = self.root.get_mut();
                let (new_r, _) = self.insert_into(simd, r, None, k.encode(), v);
                self.root.set_mut(new_r);
            }
        });
    }

    // -----------------------------------------------------------------------
    // Unified iterative `&mut self` insert / remove / batch-merge internals
    // -----------------------------------------------------------------------

    #[inline]
    fn insert_new_leaf_at(&mut self, path: &PathStack, ek: K::Encoded, val: V) -> NodeId {
        let leaf = NodeId::leaf(self.alloc_leaf_mut(|slot| Leaf::init_single(slot, ek, val)));
        self.set_child_mut(path.last(), leaf);
        self.bump_radix_counts_mut(path, 1);
        *self.len.get_mut() += 1;
        leaf
    }

    #[inline(always)]
    fn insert_into<S: Simd>(
        &mut self,
        simd: S,
        start_node: NodeId,
        start_parent: Option<PathStep>,
        ek: K::Encoded,
        val: V,
    ) -> (NodeId, Option<V>) {
        let mut path = PathStack::new();
        if let Some(p) = start_parent {
            path.push(p);
        }
        if start_node.is_empty() {
            let leaf = self.insert_new_leaf_at(&path, ek, val);
            return (leaf, None);
        }

        let mut top_node = start_node;
        let mut cur = start_node;

        loop {
            match cur.tag() {
                TAG_LEAF => {
                    let leaf_idx = cur.index();
                    // SAFETY: `leaf_idx` is a live leaf handle under `&mut self`.
                    let res = unsafe { self.pools.leaves.get_mut(leaf_idx) }.insert(simd, ek, val);
                    match res {
                        LeafInsert::Replaced(old) => return (top_node, Some(old)),
                        LeafInsert::Inserted => {
                            self.bump_radix_counts_mut(&path, 1);
                            *self.len.get_mut() += 1;
                            return (top_node, None);
                        }
                        LeafInsert::Full(val) => {
                            let new_top = self
                                .split_leaf_and_propagate(simd, leaf_idx, &path, ek, val, top_node);
                            *self.len.get_mut() += 1;
                            return (new_top, None);
                        }
                    }
                }
                TAG_INTERNAL => {
                    // SAFETY: `cur` is a live internal handle under `&mut self`.
                    let internal = unsafe { self.pools.internals.get(cur.index()) };
                    let slot = internal.find_child_idx(simd, ek);
                    path.push(PathStep { node: cur, slot });
                    cur = internal.children[slot].load_relaxed();
                }
                TAG_RADIX => {
                    let rx_idx = cur.index();
                    let (in_prefix, min_cov, max_cov, b, next) = {
                        // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
                        let rx = unsafe { self.pools.radixes.get(rx_idx) };
                        if rx.contains_prefix(ek) {
                            let b = rx.bucket_of(ek);
                            (
                                true,
                                rx.min_covered(),
                                rx.max_covered(),
                                b,
                                rx.children[b].load_relaxed(),
                            )
                        } else {
                            (false, rx.min_covered(), rx.max_covered(), 0, NodeId::EMPTY)
                        }
                    };
                    if !in_prefix {
                        let new_leaf = NodeId::leaf(
                            self.alloc_leaf_mut(|slot| Leaf::init_single(slot, ek, val)),
                        );
                        let wrapped = NodeId::internal(if ek < min_cov {
                            let sep = K::Encoded::from_u128(min_cov.to_u128() - 1);
                            self.alloc_internal_mut(|slot| {
                                Internal::init_pair(slot, new_leaf, sep, cur)
                            })
                        } else {
                            self.alloc_internal_mut(|slot| {
                                Internal::init_pair(slot, cur, max_cov, new_leaf)
                            })
                        });
                        self.set_child_mut(path.last(), wrapped);
                        if path.is_empty() {
                            top_node = wrapped;
                        }
                        self.bump_radix_counts_mut(&path, 1);
                        *self.len.get_mut() += 1;
                        return (top_node, None);
                    }
                    path.push(PathStep { node: cur, slot: b });
                    if next.is_empty() {
                        self.insert_new_leaf_at(&path, ek, val);
                        return (top_node, None);
                    }
                    cur = next;
                }
                TAG_UNREFINED => {
                    let refined = self.crack_slot_mut(&path, cur);
                    if path.is_empty() {
                        top_node = refined;
                    }
                    if refined.is_empty() {
                        let leaf = self.insert_new_leaf_at(&path, ek, val);
                        if path.is_empty() {
                            top_node = leaf;
                        }
                        return (top_node, None);
                    }
                    cur = refined;
                }
                _ => unreachable!(),
            }
        }
    }

    #[cold]
    #[inline(never)]
    fn split_leaf_and_propagate<S: Simd>(
        &mut self,
        simd: S,
        leaf_idx: u32,
        path: &PathStack,
        ek: K::Encoded,
        val: V,
        top_node: NodeId,
    ) -> NodeId {
        let right_idx = self.alloc_leaf_mut(Leaf::init_empty);
        // SAFETY: `leaf_idx` and `right_idx` are distinct live leaves under `&mut self`.
        let (left, right) = unsafe { self.pools.leaves.get_two_mut(leaf_idx, right_idx) };
        let mut sep = left.split_and_insert(simd, right, ek, val);
        let mut left_child = NodeId::leaf(leaf_idx);
        let mut right_child = NodeId::leaf(right_idx);

        self.bump_radix_counts_mut(path, 1);

        for (_, step) in path.iter_rev() {
            match step.node.tag() {
                TAG_INTERNAL => {
                    let int_idx = step.node.index();
                    // SAFETY: `int_idx` is a live internal handle under `&mut self`.
                    if !unsafe { self.pools.internals.get(int_idx) }.is_full() {
                        // SAFETY: `int_idx` is a live internal handle under `&mut self`.
                        unsafe { self.pools.internals.get_mut(int_idx) }.insert_after(
                            step.slot,
                            sep,
                            right_child,
                        );
                        return top_node;
                    }
                    let right_int = self.alloc_internal_mut(|slot| {
                        Internal::init_pair(slot, NodeId::EMPTY, K::Encoded::MAX, NodeId::EMPTY);
                    });
                    // SAFETY: `int_idx` and `right_int` are distinct live internals under `&mut self`.
                    let (left_n, right_n) =
                        unsafe { self.pools.internals.get_two_mut(int_idx, right_int) };
                    sep = left_n.split_and_insert(right_n, step.slot, sep, right_child);
                    left_child = step.node;
                    right_child = NodeId::internal(right_int);
                }
                TAG_RADIX => {
                    let new_int = NodeId::internal(self.alloc_internal_mut(|slot| {
                        Internal::init_pair(slot, left_child, sep, right_child)
                    }));
                    // SAFETY: `step.node` is a live radix handle under `&mut self`.
                    unsafe { self.pools.radixes.get_mut(step.node.index()) }.children[step.slot]
                        .set_mut(new_int);
                    return top_node;
                }
                _ => unreachable!(),
            }
        }

        let new_root =
            NodeId::internal(self.alloc_internal_mut(|slot| {
                Internal::init_pair(slot, left_child, sep, right_child)
            }));
        self.root.set_mut(new_root);
        new_root
    }

    #[inline(always)]
    fn remove_with<S: Simd>(&mut self, simd: S, ek: K::Encoded) -> Option<V> {
        let mut path = PathStack::new();
        let mut cur = self.root.get_mut();

        loop {
            if cur.is_empty() {
                return None;
            }
            match cur.tag() {
                TAG_LEAF => {
                    let leaf_idx = cur.index();
                    // SAFETY: `leaf_idx` is a live leaf handle under `&mut self`.
                    let removed =
                        unsafe { self.pools.leaves.get_mut(leaf_idx) }.remove(simd, ek)?;
                    *self.len.get_mut() -= 1;
                    self.dec_radix_counts_mut(&path);
                    // SAFETY: `leaf_idx` is a live leaf handle under `&mut self`.
                    if unsafe { self.pools.leaves.get(leaf_idx) }.is_empty() {
                        let (pools, state) = self.parts_mut();
                        pools.leaves.free(&mut state.leaf_alloc, leaf_idx);
                        self.unlink_empty(&path);
                    }
                    return Some(removed);
                }
                TAG_INTERNAL => {
                    // SAFETY: `cur` is a live internal handle under `&mut self`.
                    let internal = unsafe { self.pools.internals.get(cur.index()) };
                    let slot = internal.find_child_idx(simd, ek);
                    path.push(PathStep { node: cur, slot });
                    cur = internal.children[slot].load_relaxed();
                }
                TAG_RADIX => {
                    // SAFETY: `cur` is a live radix handle under `&mut self`.
                    let rx = unsafe { self.pools.radixes.get(cur.index()) };
                    if !rx.contains_prefix(ek) {
                        return None;
                    }
                    let slot = rx.bucket_of(ek);
                    path.push(PathStep { node: cur, slot });
                    cur = rx.children[slot].load_relaxed();
                }
                TAG_UNREFINED => cur = self.crack_slot_mut(&path, cur),
                _ => unreachable!(),
            }
        }
    }

    #[cold]
    #[inline(never)]
    fn unlink_empty(&mut self, path: &PathStack) {
        for (level, step) in path.iter_rev() {
            match step.node.tag() {
                TAG_INTERNAL => {
                    let int_idx = step.node.index();
                    // SAFETY: `int_idx` is a live internal handle under `&mut self`.
                    if unsafe { self.pools.internals.get(int_idx) }.len() == 1 {
                        let (pools, state) = self.parts_mut();
                        pools.internals.free(&mut state.internal_alloc, int_idx);
                        continue;
                    }
                    // SAFETY: `int_idx` is a live internal handle under `&mut self`.
                    unsafe { self.pools.internals.get_mut(int_idx) }.remove_child(step.slot);
                    // SAFETY: `int_idx` is a live internal handle under `&mut self`.
                    if unsafe { self.pools.internals.get(int_idx) }.len() == 1 {
                        // SAFETY: `int_idx` is a live internal handle under `&mut self`.
                        let sole =
                            unsafe { self.pools.internals.get(int_idx) }.children[0].load_relaxed();
                        let (pools, state) = self.parts_mut();
                        pools.internals.free(&mut state.internal_alloc, int_idx);
                        let parent = level.checked_sub(1).and_then(|i| path.get(i));
                        self.set_child_mut(parent, sole);
                    }
                    return;
                }
                TAG_RADIX => {
                    let rx_idx = step.node.index();
                    let all_empty = {
                        // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
                        let rx = unsafe { self.pools.radixes.get_mut(rx_idx) };
                        rx.children[step.slot].set_mut(NodeId::EMPTY);
                        *rx.counts[step.slot].get_mut() = 0;
                        rx.children.iter_mut().all(|c| c.get_mut().is_empty())
                    };
                    if all_empty {
                        let (pools, state) = self.parts_mut();
                        pools.radixes.free(&mut state.radix_alloc, rx_idx);
                        continue;
                    }
                    return;
                }
                _ => unreachable!(),
            }
        }
        self.root.set_mut(NodeId::EMPTY);
    }

    // -----------------------------------------------------------------------
    // Shared `&self` transparent cracking, range, and exclusive pool helpers
    // -----------------------------------------------------------------------

    #[inline]
    fn alloc_leaf_mut(&mut self, init: impl FnOnce(&mut MaybeUninit<Leaf<K::Encoded, V>>)) -> u32 {
        let (pools, state) = self.parts_mut();
        // SAFETY: `&mut self` guarantees exclusive access to `leaves` and `leaf_alloc`.
        unsafe { pools.leaves.alloc_with(&mut state.leaf_alloc, init) }
    }

    #[inline]
    fn alloc_internal_mut(
        &mut self,
        init: impl FnOnce(&mut MaybeUninit<Internal<K::Encoded>>),
    ) -> u32 {
        let (pools, state) = self.parts_mut();
        // SAFETY: `&mut self` guarantees exclusive access to `internals` and `internal_alloc`.
        unsafe { pools.internals.alloc_with(&mut state.internal_alloc, init) }
    }

    #[inline]
    fn set_child_mut(&mut self, parent: Option<PathStep>, child: NodeId) {
        match parent {
            None => self.root.set_mut(child),
            Some(step) => match step.node.tag() {
                TAG_INTERNAL => {
                    // SAFETY: `step.node` is a live internal handle under `&mut self`.
                    unsafe { self.pools.internals.get_mut(step.node.index()) }.children[step.slot]
                        .set_mut(child);
                }
                TAG_RADIX => {
                    // SAFETY: `step.node` is a live radix handle under `&mut self`.
                    unsafe { self.pools.radixes.get_mut(step.node.index()) }.children[step.slot]
                        .set_mut(child);
                }
                _ => unreachable!(),
            },
        }
    }

    #[inline(always)]
    fn bump_radix_counts_mut(&mut self, path: &PathStack, delta: usize) {
        path.for_each(|step| {
            if step.node.tag() == TAG_RADIX {
                // SAFETY: `step.node` is a live radix handle under `&mut self`.
                *unsafe { self.pools.radixes.get_mut(step.node.index()) }.counts[step.slot]
                    .get_mut() += delta;
            }
        });
    }

    #[inline(always)]
    fn dec_radix_counts_mut(&mut self, path: &PathStack) {
        path.for_each(|step| {
            if step.node.tag() == TAG_RADIX {
                // SAFETY: `step.node` is a live radix handle under `&mut self`.
                let c = unsafe { self.pools.radixes.get_mut(step.node.index()) }.counts[step.slot]
                    .get_mut();
                *c = c.saturating_sub(1);
            }
        });
    }

    /// Cold path during `&self`: locks `self.refine`, re-checks `path`'s tail slot, cracks once,
    /// publishes via `Release`, releases the mutex, and only then drops overwritten duplicates.
    #[cold]
    #[inline(never)]
    fn crack_slot_shared(&self, path: &PathStack, expected: NodeId) -> NodeId {
        let mut discarded = Vec::new();
        let refined = {
            let mut state = self.lock_refine();
            let current = self.pools.load_slot_acquire(&self.root, path.last());
            if current != expected {
                return current;
            }
            self.pools.crack_and_publish(
                &self.root,
                &self.len,
                &self.has_unverified_dups,
                &mut state,
                path,
                expected.index(),
                &mut discarded,
            )
        };
        drop(discarded);
        refined
    }

    /// Exclusive `&mut self` variant of cracking with zero mutex locking.
    #[cold]
    #[inline(never)]
    fn crack_slot_mut(&mut self, path: &PathStack, expected: NodeId) -> NodeId {
        let mut discarded = Vec::new();
        let refined = {
            let state = self
                .refine
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner);
            self.pools.crack_and_publish(
                &self.root,
                &self.len,
                &self.has_unverified_dups,
                state,
                path,
                expected.index(),
                &mut discarded,
            )
        };
        drop(discarded);
        refined
    }

    /// Resolves all remaining unrefined buckets under `self.refine` when exact deduplicated
    /// `len()` or `select_nth()` is requested on a map that may contain duplicates.
    /// Drops overwritten duplicate values only after publishing and releasing `self.refine`.
    #[cold]
    #[inline(never)]
    fn resolve_unverified_dups(&self) {
        let mut discarded = Vec::new();
        {
            let mut state = self.lock_refine();
            if !self.has_unverified_dups.load(Ordering::Acquire) {
                return;
            }
            let root = self.root.load_acquire();
            if !root.is_empty() {
                let new_root =
                    self.pools
                        .finish_subtree_locked(&mut state, &self.len, root, &mut discarded);
                self.root.store_release(new_root);
            }
            debug_assert!(!state.has_unrefined());
            state.clear_if_converged(&self.has_unverified_dups);
        }
        drop(discarded);
    }

    #[inline(always)]
    fn range_shared<S: Simd>(
        &self,
        simd: S,
        path: &mut PathStack,
        mut node: NodeId,
        start: K::Encoded,
        end_inclusive: K::Encoded,
        f: &mut impl FnMut(K::Encoded, &V),
    ) -> bool {
        loop {
            if node.is_empty() {
                return true;
            }
            match node.tag() {
                TAG_LEAF => {
                    // SAFETY: `node` was loaded with `Acquire` from a published slot.
                    return unsafe { self.pools.leaves.get(node.index()) }.range_visit(
                        simd,
                        start,
                        end_inclusive,
                        f,
                    );
                }
                TAG_INTERNAL => {
                    // SAFETY: `node` was loaded with `Acquire` from a published slot.
                    let internal = unsafe { self.pools.internals.get(node.index()) };
                    let lo = internal.find_child_idx(simd, start);
                    let hi = internal.find_child_idx(simd, end_inclusive);
                    for (offset, child_slot) in internal.children[lo..=hi].iter().enumerate() {
                        let c = child_slot.load_acquire();
                        path.push(PathStep {
                            node,
                            slot: lo + offset,
                        });
                        let keep_going = self.range_shared(simd, path, c, start, end_inclusive, f);
                        path.pop();
                        if !keep_going {
                            return false;
                        }
                    }
                    return true;
                }
                TAG_RADIX => {
                    // SAFETY: `node` was loaded with `Acquire` from a published slot.
                    let rx = unsafe { self.pools.radixes.get(node.index()) };
                    let Some((lo, hi)) = rx.bucket_range(start, end_inclusive) else {
                        return true;
                    };
                    for (offset, child_slot) in rx.children[lo..=hi].iter().enumerate() {
                        let c = child_slot.load_acquire();
                        if !c.is_empty() {
                            path.push(PathStep {
                                node,
                                slot: lo + offset,
                            });
                            let keep_going =
                                self.range_shared(simd, path, c, start, end_inclusive, f);
                            path.pop();
                            if !keep_going {
                                return false;
                            }
                        }
                    }
                    return true;
                }
                TAG_UNREFINED => node = self.crack_slot_shared(path, node),
                _ => unreachable!(),
            }
        }
    }

    fn subtree_count_shared(&self, parent: Option<PathStep>, node: NodeId) -> usize {
        if node.is_empty() {
            return 0;
        }
        match node.tag() {
            // SAFETY: `node` was loaded with `Acquire` from a published slot.
            TAG_LEAF => unsafe { self.pools.leaves.get(node.index()) }.len(),
            TAG_UNREFINED => {
                // Re-read the parent slot under `self.refine` so a concurrent crack cannot cause
                // stale indexing, `None` return, or handle ABA.
                let state = self.lock_refine();
                let current = self.pools.load_slot_acquire(&self.root, parent);
                self.pools.subtree_count_locked(&state, current)
            }
            TAG_RADIX => {
                // SAFETY: `node` was loaded with `Acquire` from a published slot.
                let rx = unsafe { self.pools.radixes.get(node.index()) };
                rx.counts.iter().map(|c| c.load(Ordering::Acquire)).sum()
            }
            TAG_INTERNAL => {
                // SAFETY: `node` was loaded with `Acquire` from a published slot.
                let internal = unsafe { self.pools.internals.get(node.index()) };
                internal.children[..internal.len()]
                    .iter()
                    .enumerate()
                    .map(|(slot, child_slot)| {
                        self.subtree_count_shared(
                            Some(PathStep { node, slot }),
                            child_slot.load_acquire(),
                        )
                    })
                    .sum()
            }
            _ => unreachable!(),
        }
    }
}

/// Ordered set derived directly from `TesseraMap<K, ()>`.
///
/// Because `()` is zero-sized, each leaf omits value storage (576B per 64-slot leaf, ~10.5 bytes
/// per `u64` key) while inheriting lazy batch cracking, range queries, `select_nth`, and
/// streaming batch ingestion. See `conformance/semantics.md` for contract details.
///
/// # Examples
///
/// ```
/// use tessera_order::TesseraSet;
///
/// let mut set = TesseraSet::from(vec![30u64, 10, 20, 10]);
/// assert_eq!(set.len(), 3);
/// assert!(set.contains(20));
/// assert_eq!(set.select_nth(1), Some(20));
/// set.insert(25);
/// assert_eq!(set.last(), Some(30));
/// ```
pub struct TesseraSet<K: RadixKey>(TesseraMap<K, ()>);

impl<K: RadixKey> Default for TesseraSet<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: RadixKey> From<Vec<K>> for TesseraSet<K> {
    fn from(keys: Vec<K>) -> Self {
        Self(TesseraMap::from(
            keys.into_iter().map(|k| (k, ())).collect::<Vec<_>>(),
        ))
    }
}

impl<K: RadixKey> FromIterator<K> for TesseraSet<K> {
    fn from_iter<I: IntoIterator<Item = K>>(iter: I) -> Self {
        Self(TesseraMap::from(
            iter.into_iter().map(|k| (k, ())).collect::<Vec<_>>(),
        ))
    }
}

impl<K: RadixKey> Extend<K> for TesseraSet<K> {
    fn extend<I: IntoIterator<Item = K>>(&mut self, iter: I) {
        self.0.extend(iter.into_iter().map(|k| (k, ())));
    }
}

impl<K: RadixKey> TesseraSet<K> {
    /// Creates an empty set without allocating.
    pub fn new() -> Self {
        Self(TesseraMap::new())
    }

    /// Creates a set from unsorted keys expected to be distinct.
    pub fn from_unique(keys: Vec<K>) -> Self {
        Self(TesseraMap::from_unique(
            keys.into_iter().map(|k| (k, ())).collect(),
        ))
    }

    /// Number of keys in the set.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the set is empty, in `O(1)` without refining unrefined buckets.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Removes all keys from the set.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Returns `true` if `key` is present.
    #[inline]
    pub fn contains(&self, key: K) -> bool {
        self.0.contains_key(key)
    }

    /// Adds `key` to the set; returns `true` if it was newly inserted.
    #[inline]
    pub fn insert(&mut self, key: K) -> bool {
        self.0.insert(key, ()).is_none()
    }

    /// Removes `key` from the set; returns `true` if it was present.
    #[inline]
    pub fn remove(&mut self, key: K) -> bool {
        self.0.remove(key).is_some()
    }

    /// Merges an unsorted batch of keys using Tessera's top-digit partitioning.
    pub fn extend_vec(&mut self, keys: Vec<K>) {
        self.0
            .extend_vec(keys.into_iter().map(|k| (k, ())).collect());
    }

    /// Visits all keys within `bounds` in ascending order.
    pub fn range(&self, bounds: impl RangeBounds<K>, mut f: impl FnMut(K)) {
        self.0.range(bounds, |k, &()| f(k));
    }

    /// Returns the `rank`-th smallest key (`0 <= rank < self.len()`).
    pub fn select_nth(&self, rank: usize) -> Option<K> {
        self.0.select_nth(rank).map(|(k, _)| k)
    }

    /// Returns the smallest key in the set.
    pub fn first(&self) -> Option<K> {
        self.0.first().map(|(k, _)| k)
    }

    /// Returns the largest key in the set.
    pub fn last(&self) -> Option<K> {
        self.0.last().map(|(k, _)| k)
    }

    /// Returns provisional internal structural diagnostics without refining unrefined buckets.
    pub fn stats(&self) -> MapStats {
        self.0.stats()
    }
}
