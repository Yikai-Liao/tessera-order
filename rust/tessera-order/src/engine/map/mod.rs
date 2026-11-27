// SPDX-License-Identifier: BSD-2-Clause

//! Grand-unified ordered map and set built directly on Tessera's radix refinement engine.
//!
//! [`TesseraMap`] presents ONE standard, `Send + Sync` ordered-map API (`new`, `from`,
//! `get(&self)`, `range(&self)`, `select_nth(&self)`, `insert(&mut self)`, `remove(&mut self)`,
//! `extend`) with zero user-facing mode switches (`lazy`/`eager`/`adaptive`/`finish`).
//!
//! Internally, it hides a single monotonic refinement lifecycle behind an append-only
//! stable-address [`SharedPool`] and atomic [`AtomicNodeId`] slots:
//! - Small inputs (`< LAZY_THRESHOLD`) build bottom-up immediately.
//! - Large unsorted inputs (`From<Vec<(K, V)>>` / `FromIterator`) adopt the buffer in `O(1)`
//!   time and lazily refine ONLY touched buckets during `&self` reads using Tessera's
//!   in-place `block::partition` and `tile::Scratch`.
//! - Once published into [`SharedPool`], leaves never move or mutate during `&self`, so
//!   returned `&V` references remain sound under concurrent reads and lazy refinement of
//!   disjoint buckets. Already-refined nodes are read lock-free via `Acquire` loads.
//! - Adaptive slab compaction frees sparse slabs (<25% live), ensuring memory converges cleanly.
//!
//! [`TesseraSet`] derives ordered sets from `TesseraMap<K, ()>` with zero duplicated code:
//! zero-sized `()` values shrink each leaf from 1088B to 576B (~10.5 B/entry for `u64`).

mod internal;
mod leaf;
mod pool;
mod simd;
mod slab;
#[cfg(test)]
mod tests;

use alloc::vec::Vec;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
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

/// Inputs below this length build eagerly in `From<Vec>` (takes <0.2 ms); larger unsorted
/// inputs adopt their buffer in `O(1)` and refine transparently on demand during `&self` reads.
const LAZY_THRESHOLD: usize = 16_384;
/// Maximum records in an unrefined bucket before materializing directly into leaves.
const TILE_REFINE_THRESHOLD: usize = 1024;
/// Maximum radix width per lazy crack pass (8 bits = 256 buckets).
const CRACK_MAX_BITS: usize = 8;
/// Minimum slab capacity before adaptive slab compaction fires below 25% utilization.
const COMPACT_MIN_SLAB_LEN: usize = 16_384;
/// Inline stack capacity for descent paths before spilling to heap (handles arbitrary `u128` depth).
const INLINE_PATH_CAP: usize = 16;

/// Diagnostic snapshot of internal structure and memory convergence.
#[derive(Clone, Copy, Debug, Default)]
pub struct MapStats {
    /// Number of live entries reported by the map.
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
        if !self.overflow.is_empty() {
            self.overflow.pop();
        } else if self.inline_len > 0 {
            self.inline_len -= 1;
        }
    }

    #[inline(always)]
    fn last(&self) -> Option<PathStep> {
        if let Some(&step) = self.overflow.last() {
            Some(step)
        } else if self.inline_len > 0 {
            Some(self.inline[self.inline_len - 1])
        } else {
            None
        }
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
        for &step in &self.inline[..self.inline_len] {
            f(step);
        }
        for &step in &self.overflow {
            f(step);
        }
    }

    #[inline(always)]
    fn iter_rev(&self) -> impl Iterator<Item = (usize, PathStep)> + '_ {
        let base = self.inline_len;
        let tail = self
            .overflow
            .iter()
            .enumerate()
            .rev()
            .map(move |(i, &s)| (base + i, s));
        let head = self.inline[..self.inline_len]
            .iter()
            .enumerate()
            .rev()
            .map(|(i, &s)| (i, s));
        tail.chain(head)
    }
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
            workspace: Workspace::new(),
        }
    }

    fn alloc_slab(&mut self, slab: RawSlab<K, V>) -> u32 {
        let id = u32::try_from(self.slabs.len()).expect("slab index overflow");
        self.slabs.push(Some(slab));
        id
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

    fn has_unrefined(&self) -> bool {
        self.unrefineds.iter().any(Option::is_some)
    }

    fn drain_slab_range(&mut self, slab_id: u32, start: u32, end: u32, out: &mut Vec<(K, V)>) {
        let (s, e) = (start as usize, end as usize);
        let should_free = {
            let slab = self.slabs[slab_id as usize].as_mut().unwrap();
            for slot in &mut slab.data[s..e] {
                // SAFETY: caller exclusively owns initialized range `s..e`.
                out.push(unsafe { slot.assume_init_read() });
            }
            slab.live_count -= e - s;
            slab.live_count == 0
        };
        if should_free {
            self.slabs[slab_id as usize] = None;
        }
    }

    /// Adaptive slab compaction: frees sparse slabs once utilization drops below 25%.
    fn maybe_compact_slab(&mut self, slab_id: u32) {
        let (total_len, live_count) = match self.slabs[slab_id as usize].as_ref() {
            Some(s) => (s.data.len(), s.live_count),
            None => return,
        };
        if live_count == 0 {
            self.slabs[slab_id as usize] = None;
            return;
        }
        if total_len < COMPACT_MIN_SLAB_LEN || live_count * 4 >= total_len {
            return;
        }

        // Pre-reserve both buffers before mutating any slab or `UnrefinedNode` descriptor so
        // an allocation failure leaves `RefineState` completely intact.
        let mut packed: Vec<(K, V)> = Vec::with_capacity(live_count);
        self.slabs.reserve(1);
        let new_slab_id = u32::try_from(self.slabs.len()).expect("slab index overflow");

        {
            let old_slab = self.slabs[slab_id as usize].as_mut().unwrap();
            for u in self.unrefineds.iter_mut().flatten() {
                if u.slab_id == slab_id {
                    let new_start = packed.len() as u32;
                    for slot in &mut old_slab.data[u.start as usize..u.end as usize] {
                        // SAFETY: live `UnrefinedNode` exclusively owns initialized `u.start..u.end`.
                        packed.push(unsafe { slot.assume_init_read() });
                    }
                    u.slab_id = new_slab_id;
                    u.start = new_start;
                    u.end = packed.len() as u32;
                }
            }
        }
        debug_assert_eq!(packed.len(), live_count);
        self.slabs[slab_id as usize] = None;
        // `packed.len() == packed.capacity() == live_count` and `self.slabs` has reserved space,
        // so creating and pushing the new slab performs zero allocations.
        self.slabs.push(Some(RawSlab::from_vec(packed)));
    }
}

impl<K, V> Drop for RefineState<K, V> {
    fn drop(&mut self) {
        for u in self.unrefineds.iter_mut().flatten() {
            if let Some(slab) = self
                .slabs
                .get_mut(u.slab_id as usize)
                .and_then(Option::as_mut)
            {
                for slot in &mut slab.data[u.start as usize..u.end as usize] {
                    // SAFETY: live `UnrefinedNode` exclusively owns initialized `u.start..u.end`.
                    unsafe { slot.assume_init_drop() };
                }
            }
        }
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

    #[inline(always)]
    fn load_slot_acquire(&self, root: &AtomicNodeId, parent: Option<PathStep>) -> NodeId {
        match parent {
            None => root.load_acquire(),
            Some(step) => match step.node.tag() {
                TAG_INTERNAL => {
                    // SAFETY: `step.node` is a published internal handle obtained via `Acquire`.
                    let internal = unsafe { self.internals.get(step.node.index()) };
                    internal.children[step.slot].load_acquire()
                }
                TAG_RADIX => {
                    // SAFETY: `step.node` is a published radix handle obtained via `Acquire`.
                    let rx = unsafe { self.radixes.get(step.node.index()) };
                    rx.children[step.slot].load_acquire()
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
                    rx.counts[step.slot]
                        .store(prev.saturating_sub(dropped as u32), Ordering::Release);
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
                rx.counts
                    .iter()
                    .map(|c| c.load(Ordering::Relaxed) as usize)
                    .sum()
            }
            TAG_INTERNAL => {
                // SAFETY: `node` is a live published internal handle under `state`.
                let internal = unsafe { self.internals.get(node.index()) };
                (0..internal.len())
                    .map(|i| self.subtree_count_locked(state, internal.children[i].load_relaxed()))
                    .sum()
            }
            _ => unreachable!(),
        }
    }

    fn finish_subtree_locked<K: RadixKey<Encoded = E>>(
        &self,
        state: &mut RefineState<K, V>,
        len_atom: &AtomicUsize,
        node: NodeId,
    ) -> NodeId {
        if node.is_empty() {
            return node;
        }
        match node.tag() {
            TAG_LEAF => node,
            TAG_UNREFINED => {
                let u = state.take_unrefined(node.index());
                let (nid, dropped) = self.materialize_unrefined_locked(state, u);
                if dropped > 0 {
                    len_atom.fetch_sub(dropped, Ordering::Release);
                }
                nid
            }
            TAG_RADIX => {
                // SAFETY: `node` is a live published radix handle under `state`.
                let rx = unsafe { self.radixes.get(node.index()) };
                for b in 0..rx.children.len() {
                    let c = rx.children[b].load_acquire();
                    if !c.is_empty() && c.tag() != TAG_LEAF {
                        let nc = self.finish_subtree_locked(state, len_atom, c);
                        let new_cnt = self.subtree_count_locked(state, nc) as u32;
                        rx.counts[b].store(new_cnt, Ordering::Release);
                        rx.children[b].store_release(nc);
                    }
                }
                node
            }
            TAG_INTERNAL => {
                // SAFETY: `node` is a live published internal handle under `state`.
                let internal = unsafe { self.internals.get(node.index()) };
                for i in 0..internal.len() {
                    let c = internal.children[i].load_acquire();
                    if !c.is_empty() && c.tag() != TAG_LEAF {
                        let nc = self.finish_subtree_locked(state, len_atom, c);
                        internal.children[i].store_release(nc);
                    }
                }
                node
            }
            _ => unreachable!(),
        }
    }

    /// Performs ONE step of Tessera refinement while holding `state`.
    fn crack_unrefined_locked<K: RadixKey<Encoded = E>>(
        &self,
        state: &mut RefineState<K, V>,
        uid: u32,
    ) -> (NodeId, usize) {
        let u = state.take_unrefined(uid);
        let len = u.len();
        if len == 0 {
            return (NodeId::EMPTY, 0);
        }
        if len <= TILE_REFINE_THRESHOLD {
            return self.materialize_unrefined_locked(state, u);
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
            return self.materialize_unrefined_locked(state, u);
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
            let mut ext = ByKey(|r: &(K, V)| r.0);
            let classifier = KeyDigit::new(&mut ext, digit.shift);
            let mut noop = |_: usize,
                            _: core::ops::Range<usize>,
                            _: &mut [(K, V)],
                            _: &mut KeyDigit<&mut ByKey<_>, E>| {};
            block::partition(
                slice,
                digit.width,
                classifier,
                &mut state.workspace.block,
                &mut noop,
            )
        };

        let num_buckets = 1usize << digit.width;
        let mut children = alloc::vec![NodeId::EMPTY; num_buckets];
        let mut counts = alloc::vec![0u32; num_buckets];
        let child_candidate = varying & !digit.mask::<E>().to_u128();

        let mut prev = 0usize;
        for (b, &end) in ends.iter().enumerate() {
            if end > prev {
                counts[b] = (end - prev) as u32;
                let cid = state.alloc_unrefined(UnrefinedNode {
                    slab_id: u.slab_id,
                    start: u.start + prev as u32,
                    end: u.start + end as u32,
                    candidate_mask: child_candidate,
                });
                children[b] = NodeId::unrefined(cid);
            }
            prev = end;
        }

        let prefix_shift = (digit.shift + digit.width) as u8;
        let prefix_base = if (prefix_shift as usize) >= E::BITS {
            E::from_usize(0)
        } else {
            E::from_u128(sample_key.to_u128() & !Self::mask_below(prefix_shift as usize))
        };

        let rx = RadixNode::new(
            prefix_base,
            prefix_shift,
            digit.shift as u8,
            digit.width as u8,
            children,
            counts,
        );
        // SAFETY: `state` serializes all allocations into `self.radixes`.
        let rx_id = unsafe {
            self.radixes.alloc_with(&mut state.radix_alloc, |slot| {
                slot.write(rx);
            })
        };
        (NodeId::radix(rx_id), 0)
    }

    fn materialize_unrefined_locked<K: RadixKey<Encoded = E>>(
        &self,
        state: &mut RefineState<K, V>,
        u: UnrefinedNode,
    ) -> (NodeId, usize) {
        let (len, s, e, slab_id) = (u.len(), u.start as usize, u.end as usize, u.slab_id);

        // Zero-copy fast path when `u` owns the entire backing slab (e.g. full convergence).
        if s == 0 && len == state.slabs[slab_id as usize].as_ref().unwrap().data.len() {
            let slab = state.slabs[slab_id as usize].take().unwrap();
            // SAFETY: `u` exclusively owns every initialized slot `0..len` in `slab`.
            let mut records = unsafe { slab.into_vec() };
            if len > 1 {
                engine_sort(&mut records, ByKey(|r: &(K, V)| r.0));
                dedup_sorted_last_wins(&mut records);
            }
            let dropped = len - records.len();
            let node = self.build_bottom_up_sorted(state, records);
            return (node, dropped);
        }

        {
            let slab = state.slabs[slab_id as usize].as_mut().unwrap();
            // SAFETY: `u` exclusively owns initialized `u.start..u.end`.
            let slice = unsafe { slab.slice_mut(u.start, u.end) };
            let mut ext = ByKey(|r: &(K, V)| r.0);
            if len > 1 {
                if len <= tile_capacity::<(K, V)>(TILE_BYTES) {
                    let varying = scan_varying_bits(slice, u.candidate_mask, &mut ext);
                    state.workspace.tile.sort(slice, varying, &mut ext);
                } else {
                    engine_sort(slice, ByKey(|r: &(K, V)| r.0));
                }
            }
        }

        // Drain all `len` records before dropping any overwritten duplicate values so a
        // panicking `Drop` on `V` never leaves unconsumed initialized slots in `slab`.
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

        dedup_sorted_last_wins(&mut drained);
        let dropped = len - drained.len();
        let node = self.build_bottom_up_sorted(state, drained);
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
        if n < 4096 {
            return self.build_btree_from_iter(
                state,
                n,
                records.into_iter().map(|(k, v)| (k.encode(), v)),
            );
        }

        let min_k = records[0].0.encode().to_u128();
        let max_k = records[n - 1].0.encode().to_u128();
        let diff = min_k ^ max_k;
        if diff == 0 {
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

        let mut counts = alloc::vec![0u32; num_buckets];
        for (k, _) in &records {
            counts[((k.encode().to_u128() >> shift) & mask) as usize] += 1;
        }

        let mut children = alloc::vec![NodeId::EMPTY; num_buckets];
        let mut iter = records.into_iter().map(|(k, v)| (k.encode(), v));
        for b in 0..num_buckets {
            let cnt = counts[b] as usize;
            if cnt > 0 {
                children[b] = self.build_btree_from_iter(state, cnt, iter.by_ref().take(cnt));
            }
        }

        let prefix_shift = (shift + width) as u8;
        let prefix_base = if (prefix_shift as usize) >= E::BITS {
            E::from_usize(0)
        } else {
            E::from_u128(min_k & !Self::mask_below(prefix_shift as usize))
        };

        let rx = RadixNode::new(
            prefix_base,
            prefix_shift,
            shift as u8,
            width as u8,
            children,
            counts,
        );
        // SAFETY: `state` serializes allocations into `self.radixes`.
        let rx_id = unsafe {
            self.radixes.alloc_with(&mut state.radix_alloc, |slot| {
                slot.write(rx);
            })
        };
        NodeId::radix(rx_id)
    }
}

/// Ordered map keyed by any Tessera [`RadixKey`].
///
/// Combines `O(1)` lazy adoption of large unsorted batches, transparent on-demand radix
/// refinement during `&self` reads, gapped `fearless_simd` leaves, and streaming batch
/// ingestion behind one standard, thread-safe (`Send + Sync`) ordered-map interface.
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
        Self::from_vec_inner(records, true)
    }
}

impl<K: RadixKey, V> FromIterator<(K, V)> for TesseraMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self::from(iter.into_iter().collect::<Vec<_>>())
    }
}

impl<K: RadixKey, V> Extend<(K, V)> for TesseraMap<K, V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        let batch: Vec<(K, V)> = iter.into_iter().collect();
        self.extend_vec(batch);
    }
}

/// Deduplicates a sorted `Vec<(K, V)>` in place, keeping the LAST value for equal keys.
fn dedup_sorted_last_wins<K: RadixKey, V>(records: &mut Vec<(K, V)>) {
    let n = records.len();
    if n < 2 {
        return;
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
    records.truncate(write + 1);
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
    /// keys are accidentally present, refinement still deduplicates them safely (last-wins)
    /// without violating invariants, though unrefined buckets reflect raw counts until touched.
    pub fn from_unique(records: Vec<(K, V)>) -> Self {
        Self::from_vec_inner(records, false)
    }

    fn from_vec_inner(mut records: Vec<(K, V)>, may_have_dups: bool) -> Self {
        let len = records.len();
        if len == 0 {
            return Self::new();
        }
        let mut map = Self::new();
        if len < LAZY_THRESHOLD {
            engine_sort(&mut records, ByKey(|r: &(K, V)| r.0));
            dedup_sorted_last_wins(&mut records);
            let n = records.len();
            let state = map.refine.get_mut().unwrap_or_else(PoisonError::into_inner);
            let root = map.pools.build_bottom_up_sorted(state, records);
            map.root.set_mut(root);
            *map.len.get_mut() = n;
            return map;
        }

        let state = map.refine.get_mut().unwrap_or_else(PoisonError::into_inner);
        let slab_id = state.alloc_slab(RawSlab::from_vec(records));
        let uid = state.alloc_unrefined(UnrefinedNode {
            slab_id,
            start: 0,
            end: len as u32,
            candidate_mask: K::VARYING_MASK,
        });
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

    /// Returns `true` if the map contains no entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.root.load_acquire().is_empty() || self.len() == 0
    }

    /// Removes all entries and releases all internal storage.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Returns internal structural diagnostics (leaf/internal/radix/unrefined counts).
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

    /// Cold path for `get`: tracks the full ancestor `PathStack` so any duplicates dropped
    /// during cracking propagate cleanly up every ancestor `RadixNode`.
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
                TAG_UNREFINED => {
                    cur = self.crack_slot_shared(&path, cur);
                }
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

    /// Returns the smallest `(K, &V)` pair in the map, if non-empty.
    pub fn first(&self) -> Option<(K, &V)> {
        let mut cur = self.root.load_acquire();
        let mut path = PathStack::new();
        loop {
            if cur.is_empty() {
                return None;
            }
            match cur.tag() {
                TAG_LEAF => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    return unsafe { self.pools.leaves.get(cur.index()) }
                        .first()
                        .map(|(k, v)| (K::decode(k), v));
                }
                TAG_INTERNAL => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let internal = unsafe { self.pools.internals.get(cur.index()) };
                    path.push(PathStep { node: cur, slot: 0 });
                    cur = internal.children[0].load_acquire();
                }
                TAG_RADIX => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let rx = unsafe { self.pools.radixes.get(cur.index()) };
                    let (slot, next) = rx.children.iter().enumerate().find_map(|(i, c)| {
                        let id = c.load_acquire();
                        (!id.is_empty()).then_some((i, id))
                    })?;
                    path.push(PathStep { node: cur, slot });
                    cur = next;
                }
                TAG_UNREFINED => cur = self.crack_slot_shared(&path, cur),
                _ => unreachable!(),
            }
        }
    }

    /// Returns the largest `(K, &V)` pair in the map, if non-empty.
    pub fn last(&self) -> Option<(K, &V)> {
        let mut cur = self.root.load_acquire();
        let mut path = PathStack::new();
        loop {
            if cur.is_empty() {
                return None;
            }
            match cur.tag() {
                TAG_LEAF => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    return unsafe { self.pools.leaves.get(cur.index()) }
                        .last()
                        .map(|(k, v)| (K::decode(k), v));
                }
                TAG_INTERNAL => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let internal = unsafe { self.pools.internals.get(cur.index()) };
                    let slot = internal.len() - 1;
                    path.push(PathStep { node: cur, slot });
                    cur = internal.children[slot].load_acquire();
                }
                TAG_RADIX => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let rx = unsafe { self.pools.radixes.get(cur.index()) };
                    let (slot, next) =
                        rx.children.iter().enumerate().rev().find_map(|(i, c)| {
                            let id = c.load_acquire();
                            (!id.is_empty()).then_some((i, id))
                        })?;
                    path.push(PathStep { node: cur, slot });
                    cur = next;
                }
                TAG_UNREFINED => cur = self.crack_slot_shared(&path, cur),
                _ => unreachable!(),
            }
        }
    }

    /// Returns the `rank`-th smallest `(K, &V)` pair (`0 <= rank < self.len()`).
    ///
    /// On maps created via [`Self::from_unique`] (or already deduplicated), lazily refines
    /// ONLY the buckets containing `rank` and persists all cracked structure for future queries.
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
                TAG_UNREFINED => {
                    cur = self.crack_slot_shared(&path, cur);
                }
                TAG_RADIX => {
                    // SAFETY: `cur` was loaded with `Acquire` from a published slot.
                    let rx = unsafe { self.pools.radixes.get(cur.index()) };
                    let mut chosen = None;
                    for (slot, cnt) in rx.counts.iter().enumerate() {
                        let c = cnt.load(Ordering::Acquire) as usize;
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
                    for slot in 0..internal.len() {
                        let child = internal.children[slot].load_acquire();
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
                let ends = {
                    let state = self
                        .refine
                        .get_mut()
                        .unwrap_or_else(PoisonError::into_inner);
                    let mut ext = ByKey(|r: &(K, V)| r.0);
                    let classifier = KeyDigit::new(&mut ext, shift as usize);
                    let mut noop =
                        |_: usize,
                         _: core::ops::Range<usize>,
                         _: &mut [(K, V)],
                         _: &mut KeyDigit<&mut ByKey<_>, K::Encoded>| {};
                    block::partition(
                        &mut batch,
                        width as usize,
                        classifier,
                        &mut state.workspace.block,
                        &mut noop,
                    )
                };

                let tile_len = tile_capacity::<(K, V)>(TILE_BYTES);
                let mut needs_slab = false;
                let mut start = 0usize;
                for (b, &end) in ends.iter().enumerate() {
                    if end > start {
                        // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
                        let child =
                            unsafe { self.pools.radixes.get(rx_idx) }.children[b].load_relaxed();
                        if child.is_empty() || child.tag() == TAG_UNREFINED {
                            needs_slab = true;
                        } else if end - start > 16 {
                            let slice = &mut batch[start..end];
                            if slice.len() <= tile_len {
                                let state = self
                                    .refine
                                    .get_mut()
                                    .unwrap_or_else(PoisonError::into_inner);
                                let mut ext = ByKey(|r: &(K, V)| r.0);
                                let cand = NodePools::<K::Encoded, V>::mask_below(shift as usize)
                                    & K::VARYING_MASK;
                                let varying = scan_varying_bits(slice, cand, &mut ext);
                                state.workspace.tile.sort(slice, varying, &mut ext);
                            } else {
                                engine_sort(slice, ByKey(|r: &(K, V)| r.0));
                            }
                        }
                    }
                    start = end;
                }

                dispatch!(self.level, simd => {
                    self.merge_partitioned_batch(simd, rx_idx, shift, batch, &ends, needs_slab);
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

    #[inline(always)]
    fn insert_into<S: Simd>(
        &mut self,
        simd: S,
        start_node: NodeId,
        start_parent: Option<PathStep>,
        ek: K::Encoded,
        val: V,
    ) -> (NodeId, Option<V>) {
        if start_node.is_empty() {
            let leaf = NodeId::leaf(self.alloc_leaf_mut(|slot| Leaf::init_single(slot, ek, val)));
            self.set_child_mut(start_parent, leaf);
            if let Some(p) = start_parent
                && p.node.tag() == TAG_RADIX
            {
                // SAFETY: `p.node` is a live radix handle under `&mut self`.
                *unsafe { self.pools.radixes.get_mut(p.node.index()) }.counts[p.slot].get_mut() +=
                    1;
            }
            *self.len.get_mut() += 1;
            return (leaf, None);
        }

        let mut path = PathStack::new();
        if let Some(p) = start_parent {
            path.push(p);
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
                            self.alloc_internal_mut(|slot| {
                                Internal::init_pair(slot, new_leaf, ek, cur)
                            })
                        } else {
                            self.alloc_internal_mut(|slot| {
                                Internal::init_pair(slot, cur, max_cov, new_leaf)
                            })
                        });
                        let parent = path.last();
                        self.set_child_mut(parent, wrapped);
                        if path.is_empty() {
                            top_node = wrapped;
                        }
                        self.bump_radix_counts_mut(&path, 1);
                        *self.len.get_mut() += 1;
                        return (top_node, None);
                    }
                    path.push(PathStep { node: cur, slot: b });
                    if next.is_empty() {
                        let leaf = NodeId::leaf(
                            self.alloc_leaf_mut(|slot| Leaf::init_single(slot, ek, val)),
                        );
                        // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
                        unsafe { self.pools.radixes.get_mut(rx_idx) }.children[b].set_mut(leaf);
                        self.bump_radix_counts_mut(&path, 1);
                        *self.len.get_mut() += 1;
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
                        let leaf = NodeId::leaf(
                            self.alloc_leaf_mut(|slot| Leaf::init_single(slot, ek, val)),
                        );
                        self.set_child_mut(path.last(), leaf);
                        if path.is_empty() {
                            top_node = leaf;
                        }
                        self.bump_radix_counts_mut(&path, 1);
                        *self.len.get_mut() += 1;
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
        mut top_node: NodeId,
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
        top_node = new_root;
        top_node
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
                        let state = self
                            .refine
                            .get_mut()
                            .unwrap_or_else(PoisonError::into_inner);
                        self.pools.leaves.free(&mut state.leaf_alloc, leaf_idx);
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
                TAG_UNREFINED => {
                    cur = self.crack_slot_mut(&path, cur);
                }
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
                    let len = unsafe { self.pools.internals.get(int_idx) }.len();
                    if len == 1 {
                        let state = self
                            .refine
                            .get_mut()
                            .unwrap_or_else(PoisonError::into_inner);
                        self.pools
                            .internals
                            .free(&mut state.internal_alloc, int_idx);
                        continue;
                    }
                    // SAFETY: `int_idx` is a live internal handle under `&mut self`.
                    unsafe { self.pools.internals.get_mut(int_idx) }.remove_child(step.slot);
                    // SAFETY: `int_idx` is a live internal handle under `&mut self`.
                    if unsafe { self.pools.internals.get(int_idx) }.len() == 1 {
                        // SAFETY: `int_idx` is a live internal handle under `&mut self`.
                        let sole =
                            unsafe { self.pools.internals.get(int_idx) }.children[0].load_relaxed();
                        let state = self
                            .refine
                            .get_mut()
                            .unwrap_or_else(PoisonError::into_inner);
                        self.pools
                            .internals
                            .free(&mut state.internal_alloc, int_idx);
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
                        let state = self
                            .refine
                            .get_mut()
                            .unwrap_or_else(PoisonError::into_inner);
                        self.pools.radixes.free(&mut state.radix_alloc, rx_idx);
                        continue;
                    }
                    return;
                }
                _ => unreachable!(),
            }
        }
        self.root.set_mut(NodeId::EMPTY);
    }

    #[inline(always)]
    fn merge_partitioned_batch<S: Simd>(
        &mut self,
        simd: S,
        rx_idx: u32,
        shift: u8,
        batch: Vec<(K, V)>,
        ends: &[usize],
        needs_slab: bool,
    ) {
        let rx_node = NodeId::radix(rx_idx);
        if !needs_slab {
            let mut iter = batch.into_iter();
            let mut start = 0usize;
            for (b, &end) in ends.iter().enumerate() {
                let count = end - start;
                if count == 0 {
                    continue;
                }
                let step = Some(PathStep {
                    node: rx_node,
                    slot: b,
                });
                for (k, v) in iter.by_ref().take(count) {
                    // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
                    let child =
                        unsafe { self.pools.radixes.get(rx_idx) }.children[b].load_relaxed();
                    self.insert_into(simd, child, step, k.encode(), v);
                }
                start = end;
            }
            return;
        }

        let slab_id = self
            .refine
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .alloc_slab(RawSlab::from_vec(batch));
        let full_bucket_cand =
            NodePools::<K::Encoded, V>::mask_below(shift as usize) & K::VARYING_MASK;
        let mut start = 0usize;
        for (b, &end) in ends.iter().enumerate() {
            let count = end - start;
            if count == 0 {
                continue;
            }
            // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
            let child = unsafe { self.pools.radixes.get(rx_idx) }.children[b].load_relaxed();
            if child.is_empty() {
                let state = self
                    .refine
                    .get_mut()
                    .unwrap_or_else(PoisonError::into_inner);
                let uid = state.alloc_unrefined(UnrefinedNode {
                    slab_id,
                    start: start as u32,
                    end: end as u32,
                    candidate_mask: full_bucket_cand,
                });
                // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
                let rx = unsafe { self.pools.radixes.get_mut(rx_idx) };
                rx.children[b].set_mut(NodeId::unrefined(uid));
                *rx.counts[b].get_mut() += count as u32;
                *self.len.get_mut() += count;
                *self.has_unverified_dups.get_mut() = true;
            } else if child.tag() == TAG_UNREFINED {
                let (uid, combined_len) = {
                    let state = self
                        .refine
                        .get_mut()
                        .unwrap_or_else(PoisonError::into_inner);
                    let old_len = state.unrefineds[child.index() as usize]
                        .as_ref()
                        .expect("live unrefined")
                        .len();
                    // Allocate before `take_unrefined` so OOM never leaks `old_u`.
                    let mut combined = Vec::with_capacity(old_len + count);
                    let old_u = state.take_unrefined(child.index());
                    state.drain_slab_range(old_u.slab_id, old_u.start, old_u.end, &mut combined);
                    state.drain_slab_range(slab_id, start as u32, end as u32, &mut combined);
                    let combined_len = combined.len();
                    let new_slab = state.alloc_slab(RawSlab::from_vec(combined));
                    let uid = state.alloc_unrefined(UnrefinedNode {
                        slab_id: new_slab,
                        start: 0,
                        end: combined_len as u32,
                        // Newly merged keys can vary in bits that were constant in the old batch.
                        candidate_mask: full_bucket_cand,
                    });
                    (uid, combined_len)
                };
                // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
                let rx = unsafe { self.pools.radixes.get_mut(rx_idx) };
                rx.children[b].set_mut(NodeId::unrefined(uid));
                *rx.counts[b].get_mut() = combined_len as u32;
                *self.len.get_mut() += count;
                *self.has_unverified_dups.get_mut() = true;
            } else {
                let step = Some(PathStep {
                    node: rx_node,
                    slot: b,
                });
                for idx in start..end {
                    let (k, v) = {
                        let state = self
                            .refine
                            .get_mut()
                            .unwrap_or_else(PoisonError::into_inner);
                        let slab = state.slabs[slab_id as usize].as_mut().unwrap();
                        // SAFETY: exclusively consuming initialized `start..end` once.
                        unsafe { slab.data[idx].assume_init_read() }
                    };
                    // SAFETY: `rx_idx` is a live radix handle under `&mut self`.
                    let cur_child =
                        unsafe { self.pools.radixes.get(rx_idx) }.children[b].load_relaxed();
                    self.insert_into(simd, cur_child, step, k.encode(), v);
                }
                let state = self
                    .refine
                    .get_mut()
                    .unwrap_or_else(PoisonError::into_inner);
                let should_free = {
                    let slab = state.slabs[slab_id as usize].as_mut().unwrap();
                    slab.live_count -= count;
                    slab.live_count == 0
                };
                if should_free {
                    state.slabs[slab_id as usize] = None;
                }
            }
            start = end;
        }
    }

    // -----------------------------------------------------------------------
    // Shared `&self` transparent cracking, range, and exclusive pool helpers
    // -----------------------------------------------------------------------

    #[inline]
    fn alloc_leaf_mut(&mut self, init: impl FnOnce(&mut MaybeUninit<Leaf<K::Encoded, V>>)) -> u32 {
        let state = self
            .refine
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner);
        // SAFETY: `&mut self` guarantees exclusive access to `leaves` and `leaf_alloc`.
        unsafe { self.pools.leaves.alloc_with(&mut state.leaf_alloc, init) }
    }

    #[inline]
    fn alloc_internal_mut(
        &mut self,
        init: impl FnOnce(&mut MaybeUninit<Internal<K::Encoded>>),
    ) -> u32 {
        let state = self
            .refine
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner);
        // SAFETY: `&mut self` guarantees exclusive access to `internals` and `internal_alloc`.
        unsafe {
            self.pools
                .internals
                .alloc_with(&mut state.internal_alloc, init)
        }
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
    fn bump_radix_counts_mut(&mut self, path: &PathStack, delta: u32) {
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

    /// Cold path during `&self`: locks `self.refine`, re-checks `path`'s tail slot (in case
    /// another thread already cracked `expected`), cracks once, and publishes via `Release`.
    #[cold]
    #[inline(never)]
    fn crack_slot_shared(&self, path: &PathStack, expected: NodeId) -> NodeId {
        let mut state = self.lock_refine();
        let current = self.pools.load_slot_acquire(&self.root, path.last());
        if current != expected {
            return current;
        }
        let (refined, dropped) = self
            .pools
            .crack_unrefined_locked(&mut state, expected.index());
        if dropped > 0 {
            self.len.fetch_sub(dropped, Ordering::Release);
        }
        self.pools
            .publish_slot_release(&self.root, path, refined, dropped);
        if !state.has_unrefined() {
            self.has_unverified_dups.store(false, Ordering::Release);
            state.slabs.clear();
            state.unrefineds.clear();
            state.free_unrefineds.clear();
            state.workspace = Workspace::new();
        }
        refined
    }

    /// Exclusive `&mut self` variant of cracking with zero mutex locking.
    #[cold]
    #[inline(never)]
    fn crack_slot_mut(&mut self, path: &PathStack, expected: NodeId) -> NodeId {
        let state = self
            .refine
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner);
        let (refined, dropped) = self.pools.crack_unrefined_locked(state, expected.index());
        if dropped > 0 {
            *self.len.get_mut() -= dropped;
        }
        self.pools
            .publish_slot_release(&self.root, path, refined, dropped);
        if !state.has_unrefined() {
            *self.has_unverified_dups.get_mut() = false;
            state.slabs.clear();
            state.unrefineds.clear();
            state.free_unrefineds.clear();
            state.workspace = Workspace::new();
        }
        refined
    }

    /// Resolves all remaining unrefined buckets under `self.refine` when exact deduplicated
    /// `len()` or `select_nth()` is requested on a map that may contain duplicates.
    #[cold]
    #[inline(never)]
    fn resolve_unverified_dups(&self) {
        let mut state = self.lock_refine();
        if !self.has_unverified_dups.load(Ordering::Acquire) {
            return;
        }
        let root = self.root.load_acquire();
        if !root.is_empty() {
            let new_root = self
                .pools
                .finish_subtree_locked(&mut state, &self.len, root);
            self.root.store_release(new_root);
        }
        debug_assert!(!state.has_unrefined());
        self.has_unverified_dups.store(false, Ordering::Release);
        state.slabs.clear();
        state.unrefineds.clear();
        state.free_unrefineds.clear();
        state.workspace = Workspace::new();
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
                    for i in lo..=hi {
                        let c = internal.children[i].load_acquire();
                        path.push(PathStep { node, slot: i });
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
                    for b in lo..=hi {
                        let c = rx.children[b].load_acquire();
                        if !c.is_empty() {
                            path.push(PathStep { node, slot: b });
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
                TAG_UNREFINED => {
                    node = self.crack_slot_shared(path, node);
                }
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
                rx.counts
                    .iter()
                    .map(|c| c.load(Ordering::Acquire) as usize)
                    .sum()
            }
            TAG_INTERNAL => {
                // SAFETY: `node` was loaded with `Acquire` from a published slot.
                let internal = unsafe { self.pools.internals.get(node.index()) };
                (0..internal.len())
                    .map(|slot| {
                        let child = internal.children[slot].load_acquire();
                        self.subtree_count_shared(Some(PathStep { node, slot }), child)
                    })
                    .sum()
            }
            _ => unreachable!(),
        }
    }
}

/// Ordered set derived directly from `TesseraMap<K, ()>`.
///
/// Because `()` is zero-sized, each leaf automatically omits value storage (576B per 64-slot
/// leaf, ~10.5 bytes per `u64` key) while inheriting lazy batch cracking, range queries,
/// `select_nth`, and streaming batch ingestion.
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

    /// Returns `true` if the set is empty.
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

    /// Returns internal structural diagnostics.
    pub fn stats(&self) -> MapStats {
        self.0.stats()
    }
}
