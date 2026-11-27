// SPDX-License-Identifier: BSD-2-Clause

//! Interleaved two-stage gapped-SIMD leaf node (`B^S-Tree` + `u64` liveness mask).
//!
//! Layout for `E = u64`, `V = u64` (1088 bytes total, 64B aligned):
//! - Cache Line 0 (64B): `sub_max: [u64; 7]` (56B) + `valid_mask: u64` (8B).
//! - 8 interleaved `SubBlock`s (128B each = 2 adjacent cache lines!):
//!   - CL A (64B): `keys: [u64; 8]`
//!   - CL B (64B): `vals: [MaybeUninit<V>; 8]`
//!
//! Interleaving puts `keys[b]` and `vals[b]` on adjacent cache lines so hardware spatial
//! prefetching + software `prefetch_read(&subs[b].vals)` overlaps the value load with the
//! SIMD key search.
//!
//! Invariants (whenever `valid_mask != 0`, letting `last = 63 - valid_mask.leading_zeros()`):
//! 1. Slot 0 is always live (`valid_mask & 1 == 1`).
//! 2. Live keys are strictly ascending (`key(i) < key(j)` for live `i < j`).
//! 3. Interior gaps `0 < g < last` duplicate their left neighbor (`key(g) == key(g - 1)`).
//! 4. Trailing gaps `g > last` hold `E::MAX` (so sequential appends land at `last + 1` in 1 write!).
//! 5. `sub_max[b] == subs[b].keys[7]` for `b in 0..7`.

use super::simd::{count_lt_7, count_lt_8};
use crate::engine::key::Unsigned;
use core::mem::{self, MaybeUninit};
use core::ptr;
use fearless_simd::Simd;

/// Total slot capacity of one leaf node.
pub(crate) const LEAF_CAP: usize = 64;

/// Target entries per leaf when bulk-packing sorted runs (7 live + 1 gap per 8-slot sub-block).
pub(crate) const BULK_LEAF_FILL: usize = 56;

/// Result of attempting to insert into a leaf node.
pub(crate) enum LeafInsert<V> {
    /// Key was updated in place; returns previous value.
    Replaced(V),
    /// Key was inserted into an existing gap (or shifted to nearest gap).
    Inserted,
    /// Leaf is completely full (64 live entries) and must split before inserting `val`.
    Full(V),
}

/// 8-slot sub-block pairing 8 keys (CL A) with 8 values (CL B) contiguously.
#[repr(C, align(64))]
pub(crate) struct SubBlock<E: Unsigned, V> {
    pub(crate) keys: [E; 8],
    pub(crate) vals: [MaybeUninit<V>; 8],
}

/// 64-slot interleaved gapped-SIMD leaf node.
#[repr(C, align(64))]
pub(crate) struct Leaf<E: Unsigned, V> {
    /// `sub_max[b] == subs[b].keys[7]` for `b = 0..7`.
    /// Must stay at offset 0 so 64-byte SIMD loads read `[sub_max[0..7], valid_mask]`.
    pub(crate) sub_max: [E; 7],
    /// Bit `i` is `1` iff slot `i` (`subs[i >> 3].vals[i & 7]`) is initialized.
    pub(crate) valid_mask: u64,
    /// 8 interleaved key/value sub-blocks.
    pub(crate) subs: [SubBlock<E, V>; 8],
}

impl<E: Unsigned, V> Drop for Leaf<E, V> {
    fn drop(&mut self) {
        let mut live = self.valid_mask;
        self.valid_mask = 0;
        while live != 0 {
            let idx = live.trailing_zeros() as usize;
            live &= live - 1;
            // SAFETY: bit `idx` was set in `valid_mask`, so that slot is initialized.
            unsafe {
                self.subs[idx >> 3].vals[idx & 7].assume_init_drop();
            }
        }
    }
}

impl<E: Unsigned, V> Leaf<E, V> {
    #[inline(always)]
    pub(crate) fn key_at(&self, idx: usize) -> E {
        debug_assert!(idx < LEAF_CAP);
        // SAFETY: `idx < 64`.
        unsafe {
            *self
                .subs
                .get_unchecked(idx >> 3)
                .keys
                .get_unchecked(idx & 7)
        }
    }

    #[inline(always)]
    fn set_key(&mut self, idx: usize, key: E) {
        debug_assert!(idx < LEAF_CAP);
        // SAFETY: `idx < 64`.
        unsafe {
            *self
                .subs
                .get_unchecked_mut(idx >> 3)
                .keys
                .get_unchecked_mut(idx & 7) = key;
        }
    }

    #[inline(always)]
    fn write_val(&mut self, idx: usize, val: V) {
        debug_assert!(idx < LEAF_CAP);
        // SAFETY: `idx < 64`.
        unsafe {
            self.subs
                .get_unchecked_mut(idx >> 3)
                .vals
                .get_unchecked_mut(idx & 7)
                .write(val);
        }
    }

    #[inline(always)]
    unsafe fn read_val(&self, idx: usize) -> V {
        debug_assert!(idx < LEAF_CAP);
        // SAFETY: caller guarantees slot `idx` is initialized and ownership transfers out.
        unsafe {
            self.subs
                .get_unchecked(idx >> 3)
                .vals
                .get_unchecked(idx & 7)
                .assume_init_read()
        }
    }

    #[inline(always)]
    unsafe fn val_ref(&self, idx: usize) -> &V {
        debug_assert!(idx < LEAF_CAP);
        // SAFETY: caller guarantees slot `idx` is live in `valid_mask`.
        unsafe {
            self.subs
                .get_unchecked(idx >> 3)
                .vals
                .get_unchecked(idx & 7)
                .assume_init_ref()
        }
    }

    #[inline(always)]
    unsafe fn val_mut(&mut self, idx: usize) -> &mut V {
        debug_assert!(idx < LEAF_CAP);
        // SAFETY: caller guarantees slot `idx` is live in `valid_mask`.
        unsafe {
            self.subs
                .get_unchecked_mut(idx >> 3)
                .vals
                .get_unchecked_mut(idx & 7)
                .assume_init_mut()
        }
    }

    /// Initializes an empty leaf directly inside `slot`.
    #[inline]
    pub(crate) fn init_empty(slot: &mut MaybeUninit<Self>) {
        let ptr = slot.as_mut_ptr();
        // SAFETY: initializing every non-`MaybeUninit` field via raw pointers.
        unsafe {
            ptr::addr_of_mut!((*ptr).sub_max).write([E::MAX; 7]);
            ptr::addr_of_mut!((*ptr).valid_mask).write(0);
            for b in 0..8 {
                ptr::addr_of_mut!((*ptr).subs[b].keys).write([E::MAX; 8]);
            }
        }
    }

    /// Initializes a leaf directly inside `slot` with a single `(key, val)` pair at slot 0
    /// and trailing `E::MAX` gaps at slots `1..64`.
    #[inline]
    pub(crate) fn init_single(slot: &mut MaybeUninit<Self>, key: E, val: V) {
        let ptr = slot.as_mut_ptr();
        let mut keys0 = [E::MAX; 8];
        keys0[0] = key;
        // SAFETY: writing every non-`MaybeUninit` field and `subs[0].vals[0]` via raw pointers
        // before any `&mut` reference to `Self` or `SubBlock` is formed.
        unsafe {
            ptr::addr_of_mut!((*ptr).sub_max).write([E::MAX; 7]);
            ptr::addr_of_mut!((*ptr).valid_mask).write(1);
            ptr::addr_of_mut!((*ptr).subs[0].keys).write(keys0);
            ptr::addr_of_mut!((*ptr).subs[0].vals[0]).write(MaybeUninit::new(val));
            for b in 1..8 {
                ptr::addr_of_mut!((*ptr).subs[b].keys).write([E::MAX; 8]);
            }
        }
    }

    /// Bulk-initializes a leaf inside `slot` from `count` strictly ascending `(key, val)` pairs
    /// (`1 <= count <= LEAF_CAP`), distributing 1 gap per 8-slot sub-block when `count <= 56`.
    pub(crate) fn init_from_sorted<I>(slot: &mut MaybeUninit<Self>, count: usize, mut iter: I)
    where
        I: Iterator<Item = (E, V)>,
    {
        struct PartialDrop<E: Unsigned, V>(*mut Leaf<E, V>);
        impl<E: Unsigned, V> Drop for PartialDrop<E, V> {
            fn drop(&mut self) {
                // SAFETY: `self.0` was fully initialized via `init_empty`, and `valid_mask`
                // tracks every `V` written so far.
                unsafe { ptr::drop_in_place(self.0) };
            }
        }

        debug_assert!((1..=LEAF_CAP).contains(&count));
        Self::init_empty(slot);
        let guard = PartialDrop(slot.as_mut_ptr());
        // SAFETY: `init_empty` initialized every non-`MaybeUninit` field of `Self`.
        let leaf = unsafe { &mut *guard.0 };

        if count <= BULK_LEAF_FILL {
            let base_per = count / 8;
            let extra = count % 8;
            let mut last_key = E::from_usize(0);
            let mut placed = 0usize;
            for b in 0..8 {
                let n = base_per + usize::from(b < extra);
                let sub = &mut leaf.subs[b];
                for i in 0..n {
                    let (k, v) = iter.next().expect("bulk iterator shorter than count");
                    sub.keys[i] = k;
                    sub.vals[i].write(v);
                    leaf.valid_mask |= 1u64 << (b * 8 + i);
                    last_key = k;
                    placed += 1;
                }
                let fill = if placed < count { last_key } else { E::MAX };
                for i in n..8 {
                    sub.keys[i] = fill;
                }
                if b < 7 {
                    leaf.sub_max[b] = sub.keys[7];
                }
            }
        } else {
            // 57..=64 items: pack densely at `0..count`, trailing gaps are `E::MAX`.
            for b in 0..8 {
                let sub = &mut leaf.subs[b];
                for i in 0..8 {
                    let idx = b * 8 + i;
                    if idx < count {
                        let (k, v) = iter.next().expect("bulk iterator shorter than count");
                        sub.keys[i] = k;
                        sub.vals[i].write(v);
                        leaf.valid_mask |= 1u64 << idx;
                    } else {
                        sub.keys[i] = E::MAX;
                    }
                }
                if b < 7 {
                    leaf.sub_max[b] = sub.keys[7];
                }
            }
        }
        #[cfg(debug_assertions)]
        leaf.debug_verify();
        mem::forget(guard);
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.valid_mask.count_ones() as usize
    }

    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        self.valid_mask == 0
    }

    #[inline(always)]
    pub(crate) fn max_key(&self) -> E {
        debug_assert!(self.valid_mask != 0);
        let last = 63 - self.valid_mask.leading_zeros() as usize;
        self.key_at(last)
    }

    /// Returns the first slot index `idx in 0..=64` with `key_at(idx) >= target`.
    /// Requires `self.valid_mask != 0`.
    #[inline(always)]
    pub(crate) fn lower_bound<S: Simd>(&self, simd: S, target: E) -> usize {
        let b = count_lt_7(simd, &self.sub_max, target);
        // SAFETY: `b in 0..=7`.
        let sub = unsafe { self.subs.get_unchecked(b) };
        (b << 3) + count_lt_8(simd, &sub.keys, target)
    }

    /// Immutable lookup: two inlined portable SIMD steps over adjacent key/value cache lines.
    #[inline(always)]
    pub(crate) fn get<S: Simd>(&self, simd: S, target: E) -> Option<&V> {
        let valid = self.valid_mask;
        if valid == 0 {
            return None;
        }
        let b = count_lt_7(simd, &self.sub_max, target);
        // SAFETY: `b in 0..=7`.
        let sub = unsafe { self.subs.get_unchecked(b) };
        let off = count_lt_8(simd, &sub.keys, target);
        let idx = (b << 3) + off;
        if off < 8 && ((valid >> idx) & 1) != 0 && sub.keys[off] == target {
            // SAFETY: bit `idx` is set in `valid_mask`.
            return Some(unsafe { sub.vals.get_unchecked(off).assume_init_ref() });
        }
        None
    }

    /// Refreshes `sub_max[b]` for `b in lo_slot/8 ..= (hi_slot/8).min(6)`.
    #[inline(always)]
    fn refresh_sub_max(&mut self, lo_slot: usize, hi_slot: usize) {
        let lo_b = lo_slot >> 3;
        let hi_b = (hi_slot >> 3).min(6);
        for b in lo_b..=hi_b {
            self.sub_max[b] = self.subs[b].keys[7];
        }
    }

    /// Shifts `count` elements in `[src .. src + count)` right by 1 to `[src + 1 .. src + 1 + count)`.
    #[inline]
    fn shift_right_by_1(&mut self, src: usize, count: usize) {
        if count == 0 {
            return;
        }
        let end = src + count;
        // Fast path (~80% of shifts): entire move fits within one 8-slot SubBlock!
        if (src >> 3) == (end >> 3) {
            let sub = &mut self.subs[src >> 3];
            let s = src & 7;
            // SAFETY: `s + count + 1 <= 8`; shifting `MaybeUninit<V>` bitwise is sound.
            unsafe {
                ptr::copy(
                    sub.keys.as_ptr().add(s),
                    sub.keys.as_mut_ptr().add(s + 1),
                    count,
                );
                ptr::copy(
                    sub.vals.as_ptr().add(s),
                    sub.vals.as_mut_ptr().add(s + 1),
                    count,
                );
            }
            return;
        }
        for i in (src..end).rev() {
            let k = self.key_at(i);
            // SAFETY: shifting live element `i` into vacated `i + 1`.
            let v = unsafe { self.read_val(i) };
            self.set_key(i + 1, k);
            self.write_val(i + 1, v);
        }
    }

    /// Shifts `count` elements in `[src .. src + count)` left by 1 to `[src - 1 .. src - 1 + count)`.
    #[inline]
    fn shift_left_by_1(&mut self, src: usize, count: usize) {
        if count == 0 {
            return;
        }
        let dst = src - 1;
        let last = src + count - 1;
        // Fast path (~80% of shifts): entire move fits within one 8-slot SubBlock!
        if (dst >> 3) == (last >> 3) {
            let sub = &mut self.subs[dst >> 3];
            let s = src & 7;
            // SAFETY: `s >= 1` and `s + count <= 8`.
            unsafe {
                ptr::copy(
                    sub.keys.as_ptr().add(s),
                    sub.keys.as_mut_ptr().add(s - 1),
                    count,
                );
                ptr::copy(
                    sub.vals.as_ptr().add(s),
                    sub.vals.as_mut_ptr().add(s - 1),
                    count,
                );
            }
            return;
        }
        for i in src..src + count {
            let k = self.key_at(i);
            // SAFETY: shifting live element `i` into vacated `i - 1`.
            let v = unsafe { self.read_val(i) };
            self.set_key(i - 1, k);
            self.write_val(i - 1, v);
        }
    }

    /// Inserts `(key, val)` into this leaf if space permits or key already exists.
    pub(crate) fn insert<S: Simd>(&mut self, simd: S, key: E, val: V) -> LeafInsert<V> {
        if self.valid_mask == 0 {
            self.sub_max = [E::MAX; 7];
            for b in 0..8 {
                self.subs[b].keys = [E::MAX; 8];
            }
            self.subs[0].keys[0] = key;
            self.subs[0].vals[0].write(val);
            self.valid_mask = 1;
            return LeafInsert::Inserted;
        }

        let pos = self.lower_bound(simd, key);
        if pos < LEAF_CAP && ((self.valid_mask >> pos) & 1) != 0 && self.key_at(pos) == key {
            // SAFETY: slot `pos` is live.
            let old = unsafe { core::mem::replace(self.val_mut(pos), val) };
            return LeafInsert::Replaced(old);
        }

        let gaps = !self.valid_mask;
        if gaps == 0 {
            return LeafInsert::Full(val);
        }

        // Fast Path A: `pos` itself is the first trailing `MAX_VAL` gap (`pos == last_live + 1`)!
        // Zero shifting — sequential inserts hit this 100% of the time!
        if pos < LEAF_CAP && ((gaps >> pos) & 1) != 0 {
            self.set_key(pos, key);
            self.write_val(pos, val);
            self.valid_mask |= 1u64 << pos;
            if (pos & 7) == 7 && pos < 56 {
                self.sub_max[pos >> 3] = key;
            }
            #[cfg(debug_assertions)]
            self.debug_verify();
            return LeafInsert::Inserted;
        }

        // Fast Path B: `pos > 0` and slot `pos - 1` is an immediately adjacent interior gap!
        if pos > 0 && ((gaps >> (pos - 1)) & 1) != 0 {
            let slot = pos - 1;
            self.set_key(slot, key);
            self.write_val(slot, val);
            self.valid_mask |= 1u64 << slot;
            if (slot & 7) == 7 && slot < 56 {
                self.sub_max[slot >> 3] = key;
            }
            #[cfg(debug_assertions)]
            self.debug_verify();
            return LeafInsert::Inserted;
        }

        // General path: find nearest gap left (`< pos`) or right (`>= pos`) in 1 cycle.
        let right_mask = if pos < 64 { gaps & (!0u64 << pos) } else { 0 };
        let left_mask = if pos >= 64 {
            gaps
        } else if pos > 0 {
            gaps & ((1u64 << pos) - 1)
        } else {
            0
        };

        let right_gap = if right_mask != 0 {
            Some(right_mask.trailing_zeros() as usize)
        } else {
            None
        };
        let left_gap = if left_mask != 0 {
            Some(63 - left_mask.leading_zeros() as usize)
        } else {
            None
        };

        let shift_left = match (left_gap, right_gap) {
            (Some(lg), Some(rg)) => (pos - 1 - lg) < (rg - pos),
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => unreachable!("gaps != 0"),
        };

        if shift_left {
            let gap = left_gap.unwrap();
            let count = pos - (gap + 1);
            self.shift_left_by_1(gap + 1, count);
            let target_slot = pos - 1;
            self.set_key(target_slot, key);
            self.write_val(target_slot, val);
            self.valid_mask |= 1u64 << gap;
            self.refresh_sub_max(gap, target_slot);
        } else {
            let gap = right_gap.unwrap();
            let count = gap - pos;
            self.shift_right_by_1(pos, count);
            self.set_key(pos, key);
            self.write_val(pos, val);
            self.valid_mask |= 1u64 << gap;
            self.refresh_sub_max(pos, gap);
        }

        #[cfg(debug_assertions)]
        self.debug_verify();
        LeafInsert::Inserted
    }

    /// Splits a completely full leaf (`valid_mask == u64::MAX`) into `self` (left) and
    /// `right_slot` (right), inserting `(key, val)`.
    ///
    /// - **Right-edge append fast path (`ins_pos == 64`)**: keeps all 64 keys densely packed in
    ///   `self` and initializes `right` with `(key, val)` at slot 0 + 63 trailing `MAX_VAL` gaps.
    ///   Eliminates sequential-insert split overhead and achieves ~99% memory density!
    /// - **General split (`ins_pos < 64`)**: 32/33 proactive `B^S-Tree` gapping (`0x5555...`).
    pub(crate) fn split_and_insert<S: Simd>(
        &mut self,
        simd: S,
        right: &mut Self,
        key: E,
        val: V,
    ) -> E {
        debug_assert_eq!(self.valid_mask, u64::MAX);
        debug_assert_eq!(right.valid_mask, 0);
        let ins_pos = self.lower_bound(simd, key);
        debug_assert!(ins_pos == LEAF_CAP || self.key_at(ins_pos) != key);

        // Right-edge append fast path (sequential inserts!):
        if ins_pos == LEAF_CAP {
            right.sub_max = [E::MAX; 7];
            right.subs[0].keys = [E::MAX; 8];
            right.subs[0].keys[0] = key;
            right.subs[0].vals[0].write(val);
            for b in 1..8 {
                right.subs[b].keys = [E::MAX; 8];
            }
            right.valid_mask = 1;
            #[cfg(debug_assertions)]
            right.debug_verify();
            return self.subs[7].keys[7];
        }

        let mut all_keys = [key; LEAF_CAP + 1];
        let mut all_vals: [MaybeUninit<V>; LEAF_CAP + 1] =
            [const { MaybeUninit::uninit() }; LEAF_CAP + 1];

        // Move all 64 entries out of `self` cleanly (`valid_mask = 0` prevents double drop).
        self.valid_mask = 0;
        for i in 0..ins_pos {
            all_keys[i] = self.key_at(i);
            // SAFETY: all 64 slots were live.
            all_vals[i].write(unsafe { self.read_val(i) });
        }
        all_keys[ins_pos] = key;
        all_vals[ins_pos].write(val);
        for i in ins_pos..LEAF_CAP {
            all_keys[i + 1] = self.key_at(i);
            // SAFETY: all 64 slots were live before `self.valid_mask` was cleared.
            all_vals[i + 1].write(unsafe { self.read_val(i) });
        }

        const LEFT_N: usize = 32;

        // Pack left (`self`): 32 entries at even slots `0, 2, ..., 62`.
        // Note: slot 63 is a trailing gap (> last_live = 62), so slot 63 holds `E::MAX`!
        for i in 0..LEFT_N {
            let k = all_keys[i];
            let s = i * 2;
            self.set_key(s, k);
            self.set_key(s + 1, if i + 1 < LEFT_N { k } else { E::MAX });
            // SAFETY: `all_vals[i]` is initialized.
            self.write_val(s, unsafe { all_vals[i].assume_init_read() });
        }
        for b in 0..7 {
            self.sub_max[b] = self.subs[b].keys[7];
        }
        self.valid_mask = 0x5555_5555_5555_5555;

        // Pack right (`right`): 33 entries -> 31 at `0, 2, ..., 60`, last 2 at `62, 63`.
        let mut rmask = 0u64;
        for i in 0..31 {
            let k = all_keys[LEFT_N + i];
            let s = i * 2;
            right.set_key(s, k);
            right.set_key(s + 1, k);
            // SAFETY: `all_vals[LEFT_N + i]` is initialized.
            let v = unsafe { all_vals[LEFT_N + i].assume_init_read() };
            right.write_val(s, v);
            rmask |= 1u64 << s;
        }
        right.set_key(62, all_keys[LEFT_N + 31]);
        right.set_key(63, all_keys[LEFT_N + 32]);
        // SAFETY: `all_vals[63]` and `all_vals[64]` are initialized.
        let (v62, v63) = unsafe {
            (
                all_vals[LEFT_N + 31].assume_init_read(),
                all_vals[LEFT_N + 32].assume_init_read(),
            )
        };
        right.write_val(62, v62);
        right.write_val(63, v63);
        rmask |= (1u64 << 62) | (1u64 << 63);
        for b in 0..7 {
            right.sub_max[b] = right.subs[b].keys[7];
        }
        right.valid_mask = rmask;

        #[cfg(debug_assertions)]
        {
            self.debug_verify();
            right.debug_verify();
        }
        all_keys[LEFT_N - 1]
    }

    /// Removes `key` in O(1) without moving live values (except when removing slot 0).
    pub(crate) fn remove<S: Simd>(&mut self, simd: S, key: E) -> Option<V> {
        if self.valid_mask == 0 {
            return None;
        }
        let pos = self.lower_bound(simd, key);
        if pos >= LEAF_CAP || ((self.valid_mask >> pos) & 1) == 0 || self.key_at(pos) != key {
            return None;
        }
        let old_last = 63 - self.valid_mask.leading_zeros() as usize;
        // SAFETY: slot `pos` is live; clear bit before returning ownership.
        let removed = unsafe { self.read_val(pos) };
        self.valid_mask &= !(1u64 << pos);

        if self.valid_mask == 0 {
            self.sub_max = [E::MAX; 7];
            for b in 0..8 {
                self.subs[b].keys = [E::MAX; 8];
            }
            return Some(removed);
        }

        if pos == old_last {
            // Removed the rightmost live entry: everything past `new_last` becomes trailing `E::MAX`.
            let new_last = 63 - self.valid_mask.leading_zeros() as usize;
            for i in (new_last + 1)..=old_last {
                self.set_key(i, E::MAX);
            }
            self.refresh_sub_max(new_last + 1, old_last);
        } else if pos > 0 {
            let fill_key = self.key_at(pos - 1);
            let next_live = (self.valid_mask & (!0u64 << pos)).trailing_zeros() as usize;
            for i in pos..next_live {
                self.set_key(i, fill_key);
            }
            self.refresh_sub_max(pos, next_live - 1);
        } else {
            // Slot 0 removed; promote next live entry into slot 0.
            let next_live = self.valid_mask.trailing_zeros() as usize;
            let new_first_key = self.key_at(next_live);
            // SAFETY: `next_live` is live and `!= 0`.
            let promoted_val = unsafe { self.read_val(next_live) };
            self.write_val(0, promoted_val);
            self.valid_mask = (self.valid_mask & !(1u64 << next_live)) | 1;
            if next_live == old_last {
                // Sole remaining entry is now at slot 0; `1..=old_last` are trailing `E::MAX`.
                self.set_key(0, new_first_key);
                for i in 1..=old_last {
                    self.set_key(i, E::MAX);
                }
                self.refresh_sub_max(0, old_last);
            } else {
                let after = (self.valid_mask & !1u64).trailing_zeros() as usize;
                for i in 0..after {
                    self.set_key(i, new_first_key);
                }
                self.refresh_sub_max(0, after - 1);
            }
        }

        #[cfg(debug_assertions)]
        self.debug_verify();
        Some(removed)
    }

    /// Visits all live `(encoded_key, &V)` pairs in `[start, end_inclusive]` in ascending order.
    #[inline]
    pub(crate) fn range_visit<S: Simd>(
        &self,
        simd: S,
        start: E,
        end_inclusive: E,
        f: &mut impl FnMut(E, &V),
    ) -> bool {
        if self.valid_mask == 0 {
            return true;
        }
        let pos = self.lower_bound(simd, start);
        if pos >= LEAF_CAP {
            return true;
        }
        let mut mask = self.valid_mask & (!0u64 << pos);
        while mask != 0 {
            let idx = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            let k = self.key_at(idx);
            if k > end_inclusive {
                return false;
            }
            // SAFETY: bit `idx` is set in `self.valid_mask`.
            let v = unsafe { self.val_ref(idx) };
            f(k, v);
        }
        true
    }

    /// Returns the smallest live `(encoded_key, &V)` in this leaf.
    #[inline]
    pub(crate) fn first(&self) -> Option<(E, &V)> {
        if self.valid_mask == 0 {
            return None;
        }
        debug_assert_eq!(self.valid_mask & 1, 1);
        // SAFETY: slot 0 is always live when `valid_mask != 0`.
        Some((self.key_at(0), unsafe { self.val_ref(0) }))
    }

    /// Returns the largest live `(encoded_key, &V)` in this leaf.
    #[inline]
    pub(crate) fn last(&self) -> Option<(E, &V)> {
        if self.valid_mask == 0 {
            return None;
        }
        let idx = 63 - self.valid_mask.leading_zeros() as usize;
        // SAFETY: bit `idx` is the highest set bit in `valid_mask`.
        Some((self.key_at(idx), unsafe { self.val_ref(idx) }))
    }

    /// Returns the `rank`-th live `(encoded_key, &V)` in this leaf (`0 <= rank < self.len()`).
    pub(crate) fn nth(&self, mut rank: usize) -> Option<(E, &V)> {
        let mut mask = self.valid_mask;
        while mask != 0 {
            let idx = mask.trailing_zeros() as usize;
            if rank == 0 {
                // SAFETY: bit `idx` is set in `self.valid_mask`.
                return Some((self.key_at(idx), unsafe { self.val_ref(idx) }));
            }
            rank -= 1;
            mask &= mask - 1;
        }
        None
    }

    #[cfg(debug_assertions)]
    pub(crate) fn debug_verify(&self) {
        if self.valid_mask == 0 {
            return;
        }
        assert_eq!(self.valid_mask & 1, 1, "slot 0 must be live");
        let last_live = 63 - self.valid_mask.leading_zeros() as usize;
        let mut prev_live = self.key_at(0);
        for i in 1..LEAF_CAP {
            assert!(
                self.key_at(i - 1) <= self.key_at(i),
                "keys must be monotonic at {i}"
            );
            if (self.valid_mask >> i) & 1 == 1 {
                assert!(
                    prev_live < self.key_at(i),
                    "live keys must be strictly increasing at {i}"
                );
                prev_live = self.key_at(i);
            } else if i < last_live {
                assert!(
                    self.key_at(i) == self.key_at(i - 1),
                    "interior gap slot {i} must duplicate left neighbor"
                );
            } else {
                assert!(
                    self.key_at(i) == E::MAX,
                    "trailing gap slot {i} (> last_live={last_live}) must hold E::MAX"
                );
            }
        }
        for b in 0..7 {
            assert!(
                self.sub_max[b] == self.subs[b].keys[7],
                "sub_max[{b}] mismatch"
            );
        }
    }
}
