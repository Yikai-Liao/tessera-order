// SPDX-License-Identifier: BSD-2-Clause

//! Routing: order probe, variation scan, node partitions and tile leaves.
//!
//! A *node* is a slice larger than the tile budget (64 KiB of records by
//! default); [`block`] splits it with a stable block partition. A *tile* fits
//! the budget and is finished here by insertion sort, a tile-local MSD
//! partition, or LSD counting passes through scratch. A *digit* ([`Digit`]) is
//! the window of key bits one pass handles, and a *cursor table* holds one
//! `u16` position per bucket. `docs/design/engine.md` describes each stage.
//!
//! Each tile-local MSD level keeps a cursor table of up to 32 KiB on the
//! stack. The depth is at most `ceil(key bits / MIN_TILE_DIGIT_BITS)`, 22
//! levels for `u128`.

#[cfg(feature = "rayon")]
pub(crate) mod parallel;

mod plan;
mod tile;
pub(crate) use tile::Cursor;

use crate::thresholds::{INSERTION_MAX, TILE_BYTES};
use plan::{Digit, NodePlan, bit_len, plan_node, plan_progress};
use tile::capacity as tile_capacity;

use alloc::vec::Vec;

use crate::block;
use crate::key::{ByKey, Direct, Extract, KeyHalf, Unsigned};

type KeyValue<T, E> = <E as Extract<T>>::Value;

/// Stably inserts inverted records until sorted or the swap budget runs out.
/// A failed pass still leaves a stable permutation of the input.
///
/// The scanner returns the earliest descending adjacent pair in its shared
/// suffix, using the same extractor where it needs user keys. Direct-key
/// callers instead capture the backend selected for this entire pass.
/// The SIMD caller enters this from a target-feature context. Keeping the
/// loop in that context avoids out-of-line calls for each vector operation.
#[inline(always)]
pub(crate) fn insertion_loop<T, E: Extract<T>>(
    records: &mut [T],
    budget: usize,
    extractor: &mut E,
    mut first_inversion: impl FnMut(&[T], &mut E) -> Option<usize>,
) -> Option<usize> {
    let mut swaps = 0;
    let mut start = 0;
    while let Some(offset) = first_inversion(&records[start..], extractor) {
        let index = start + offset + 1;
        let (prefix, tail) = records.split_at_mut(index);
        let (mut current, prefix) = prefix.split_last_mut().unwrap();
        // The scan already compared this pair, so swap without re-reading keys.
        core::mem::swap(current, &mut tail[0]);
        swaps += 1;
        if swaps > budget {
            return None;
        }
        for before in prefix.iter_mut().rev() {
            if extractor.key(before) <= extractor.key(current) {
                break;
            }
            core::mem::swap(before, current);
            current = before;
            swaps += 1;
            if swaps > budget {
                return None;
            }
        }
        start = index;
    }
    Some(swaps)
}

/// Finishes insertion without a swap limit.
///
/// The unit return keeps the unused swap total inside the inlined operation,
/// so it can be discarded before returning across a target-feature boundary.
#[inline(always)]
pub(crate) fn finish_insertion<T, E: Extract<T>>(
    records: &mut [T],
    extractor: &mut E,
    first_inversion: impl FnMut(&[T], &mut E) -> Option<usize>,
) {
    let _ = insertion_loop(records, usize::MAX, extractor, first_inversion);
}

/// Scans until all candidate bits have appeared in both states, or the input ends.
/// Only candidate bits are returned, so even changing keys cannot restore a
/// parent partition digit and prevent the recursion from making progress.
fn scan_varying_bits<T, E: Extract<T>>(records: &[T], candidate: u128, extractor: &mut E) -> u128 {
    if candidate == 0 {
        return 0;
    }
    let first = extractor.key(&records[0]);
    let (mut union, mut intersection) = (first, first);
    let mut index = 1;
    let mut batch = 64;
    while index < records.len() {
        let end = index + batch.min(records.len() - index);
        for record in &records[index..end] {
            let key = extractor.key(record);
            union |= key;
            intersection = intersection & key;
        }
        let seen = union.to_u128() ^ intersection.to_u128();
        if seen & candidate == candidate {
            break;
        }
        index = end;
        // Early witnesses stay cheap; long scans get vectorizable stretches.
        batch = batch.saturating_mul(2);
    }
    (union.to_u128() ^ intersection.to_u128()) & candidate
}

/// Ranks a key below (0), equal to (1) or above (2) a sampled key.
///
/// Serial and parallel equality partitions read this two-bit digit; the
/// fourth bucket is unused, so the greater band runs to the end.
///
/// The rank is a full-width integer: byte arithmetic writes partial registers
/// and made the scatter loop wait on the previous record's bucket pointer
/// (1.5x slower on `u64/two_values/65536`).
#[inline]
fn equality_rank<V: Ord>(key: V, equal: V) -> usize {
    usize::from(key >= equal) + usize::from(key > equal)
}

/// Buffers shared by the nodes and tiles of one sort.
struct Scratch<T> {
    /// Tile-owned record scratch and cached classification storage.
    tile: tile::Scratch<T>,
    /// Block buffers allocated when a node first needs them.
    block: block::Buffers<T>,
}

impl<T> Scratch<T> {
    /// Creates empty scratch without allocating.
    fn new() -> Self {
        Self {
            tile: tile::Scratch::new(),
            block: block::Buffers::new(),
        }
    }
}

/// Routes nodes and finishes tiles with one extractor and shared scratch.
struct Sorter<'a, T, E> {
    scratch: &'a mut Scratch<T>,
    extractor: &'a mut E,
    tile_len: usize,
}

/// Sorts `records` stably by the keys `key` extracts.
///
/// Allocates tile and block buffers only after the order probe and varying-bit
/// scan have not finished the sort.
pub(crate) fn sort<T, E: Extract<T>>(records: &mut [T], extractor: E) {
    sort_with_tile_bytes::<T, E>(records, extractor, TILE_BYTES);
}

/// [`sort`] with a tile budget of `tile_bytes`, so tests can reach node routes
/// with small inputs.
fn sort_with_tile_bytes<T, E: Extract<T>>(records: &mut [T], mut extractor: E, tile_bytes: usize) {
    let record_bytes = core::mem::size_of::<T>();
    if records.len() < 2 || record_bytes == 0 {
        return;
    }
    let tile_len = tile_capacity::<T>(tile_bytes);
    let Some(varying) = prepare(records, &mut extractor, tile_len) else {
        return;
    };
    let mut scratch = Scratch::new();
    let scratch = &mut scratch;
    if E::Value::BITS > 64 {
        if varying >> 64 == 0 {
            Sorter::<T, _> {
                scratch,
                extractor: &mut KeyHalf::<_, 0>(extractor),
                tile_len,
            }
            .sort_node(records, varying);
            return;
        }
        if varying & ((1u128 << 64) - 1) == 0 {
            Sorter::<T, _> {
                scratch,
                extractor: &mut KeyHalf::<_, 64>(extractor),
                tile_len,
            }
            .sort_node(records, varying >> 64);
            return;
        }
    }
    Sorter::<T, E> {
        scratch,
        extractor: &mut extractor,
        tile_len,
    }
    .sort_node(records, varying);
}

/// Finishes ordered or insertion-friendly roots, otherwise returns their varying bits.
/// Shared by serial and parallel entries before either allocates partition scratch.
fn prepare<T, E: Extract<T>>(
    records: &mut [T],
    extractor: &mut E,
    tile_len: usize,
) -> Option<u128> {
    let sampled = probe_order(records, extractor);
    if finish_ordered(records, sampled, extractor) {
        return None;
    }
    if sampled == Order::Unsorted && insertion_probe(records, extractor, tile_len) {
        return None;
    }
    let candidate = u128::MAX >> (128 - E::Value::BITS);
    let varying = scan_varying_bits(records, candidate, extractor);
    if varying == 0 {
        return None;
    }
    if sampled == Order::Flat {
        let order = confirm_order(records, extractor);
        if finish_ordered(records, order, extractor) {
            return None;
        }
    }
    Some(varying)
}

/// Tries to finish `records` with a budgeted insertion pass and returns whether it
/// did.
///
/// An insertion-sized tile gets an unlimited budget; another tile gets one
/// swap per record. A longer input first insertion-sorts one tile of keys in a
/// local buffer, and scales that window's swap count to the whole input as the
/// budget for the records.
/// A window that needs no swaps says nothing about the rest, so the records are
/// not touched. A pass that runs out of budget leaves `records` a stable
/// permutation of its input.
fn insertion_probe<T, E: Extract<T>>(
    records: &mut [T],
    extractor: &mut E,
    tile_len: usize,
) -> bool {
    // A failed limited pass on an insertion leaf would scan varying bits
    // only to select insertion again. The finishing entry also avoids carrying
    // an unused swap count across the direct extractor's dispatch boundary.
    if records.len() <= INSERTION_MAX && records.len() <= tile_len {
        extractor.finish_insertion(records);
        return true;
    }
    let probe_len = tile_len.max(2).min(records.len());
    if probe_len == records.len() {
        return extractor.insertion_sort(records, probe_len).is_some();
    }
    let mut keys: Vec<_> = records[..probe_len]
        .iter()
        .map(|record| extractor.key(record))
        .collect();
    match Direct.insertion_sort(&mut keys, probe_len) {
        Some(0) | None => false,
        Some(swaps) => {
            // The successful probe bounds swaps by probe_len, so the quotient
            // fits usize even when the intermediate product does not.
            let budget = (swaps as u128 * records.len() as u128 / probe_len as u128) as usize;
            extractor.insertion_sort(records, budget).is_some()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    Ascending,
    /// Keys descend; `strict` is false once an adjacent pair ties.
    Descending {
        strict: bool,
    },
    /// Every sampled pair had equal keys; gaps may still contain differing keys.
    Flat,
    Unsorted,
}

/// Finishes a confirmed order without changing the order of equal-key records.
/// Sampled flat and unordered inputs still need preparation or partitioning.
#[inline]
fn finish_ordered<T, E: Extract<T>>(records: &mut [T], order: Order, extractor: &mut E) -> bool {
    match order {
        Order::Ascending => true,
        Order::Descending { strict } => {
            if strict {
                reverse_strictly(records);
            } else {
                reverse_stably(records, extractor);
            }
            true
        }
        Order::Flat | Order::Unsorted => false,
    }
}

/// Target pair count used to choose the order gate's stride.
const ORDER_SAMPLES: usize = 4096;

/// Classifies adjacent keys directly at unit stride, otherwise samples pairs
/// before confirming candidate order with a full scan.
#[inline(never)]
fn probe_order<T, E: Extract<T>>(records: &[T], extractor: &mut E) -> Order {
    let stride = (records.len() / ORDER_SAMPLES).max(1);
    if stride == 1 {
        return confirm_order(records, extractor);
    }
    let mut ascending = true;
    let mut descending = true;
    for index in (stride..records.len()).step_by(stride) {
        // SAFETY: the range starts at `stride >= 1`, so `index - 1` is in bounds.
        let before = extractor.key(unsafe { records.get_unchecked(index - 1) });
        // SAFETY: the range ends before `records.len()`.
        let after = extractor.key(unsafe { records.get_unchecked(index) });
        ascending &= before <= after;
        descending &= before >= after;
        if !ascending && !descending {
            return Order::Unsorted;
        }
    }
    if ascending && descending {
        return Order::Flat;
    }
    confirm_order(records, extractor)
}

/// Rejects a mixed prefix, then confirms one direction while preserving strictness.
fn confirm_order<T, E: Extract<T>>(records: &[T], extractor: &mut E) -> Order {
    // Three keys are the shortest mixed-direction witness. Reject it
    // before selecting a directional continuation; each key is read once.
    let first = extractor.key(&records[0]);
    let mut keys = records[1..].iter().map(|record| extractor.key(record));
    let Some(mut current) = keys.next() else {
        return Order::Ascending;
    };
    let mut ascending = first <= current;
    let mut descending = first >= current;
    let mut strict = first != current;
    if let Some(after) = keys.next() {
        ascending &= current <= after;
        descending &= current >= after;
        strict &= current != after;
        if !ascending && !descending {
            return Order::Unsorted;
        }
        current = after;
    }
    if ascending && descending {
        let Some(after) = keys.find(|&key| key != current) else {
            return Order::Ascending;
        };
        ascending = current < after;
        current = after;
    }
    if ascending {
        // The prefix is already encoded and checked; scan only the remaining suffix.
        let tail = &records[records.len() - keys.len()..];
        return if extractor.confirm_ascending(tail, current) {
            Order::Ascending
        } else {
            Order::Unsorted
        };
    }
    for after in keys {
        if current < after {
            return Order::Unsorted;
        }
        strict &= current != after;
        current = after;
    }
    Order::Descending { strict }
}

/// Confirms ascending keys after an already encoded predecessor.
#[inline]
pub(crate) fn confirm_ascending<T, E: Extract<T>>(
    records: &[T],
    extractor: &mut E,
    previous: E::Value,
) -> bool {
    records
        .iter()
        .try_fold(previous, |before, record| {
            let after = extractor.key(record);
            (before <= after).then_some(after)
        })
        .is_some()
}

/// Finishes a strictly descending array: every key is distinct, so one
/// reversal sorts it without moving equal keys past each other.
// Keep the reversal loop out of the general sort path.
#[inline(never)]
fn reverse_strictly<T>(records: &mut [T]) {
    records.reverse();
}

/// Sorts a descending array, stably, by reversing it and then reversing each
/// run of equal keys back. Reversal alone would swap equal keys past each other.
fn reverse_stably<T, E: Extract<T>>(records: &mut [T], extractor: &mut E) {
    records.reverse();
    let mut start = 0;
    while start < records.len() {
        let key = extractor.key(&records[start]);
        let mut end = start + 1;
        while end < records.len() && extractor.key(&records[end]) == key {
            end += 1;
        }
        records[start..end].reverse();
        start = end;
    }
}

impl<T, E: Extract<T>> Sorter<'_, T, E> {
    /// Consumes an ordered-run decision without probing or partitioning again.
    fn split_ordered(&mut self, records: &mut [T], boundary: usize, set: bool, varying: u128) {
        // Rotation preserves each run's internal order.
        let boundary = if set {
            let len = records.len();
            records.rotate_left(boundary);
            len - boundary
        } else {
            boundary
        };
        let candidate = varying & !(1u128 << (bit_len(varying) - 1));
        self.sort_child(&mut records[..boundary], candidate);
        self.sort_child(&mut records[boundary..], candidate);
    }

    /// Finishes a child using remaining variation, order and the shared node policy.
    fn sort_child(&mut self, records: &mut [T], candidate: u128) {
        if records.len() < 2 || candidate == 0 {
            return;
        }
        // Constant children finish after the varying scan; confirming order
        // lets ordered children skip partitioning.
        let varying = scan_varying_bits(records, candidate, self.extractor);
        if varying == 0 {
            return;
        }
        let order = confirm_order(records, self.extractor);
        if finish_ordered(records, order, self.extractor) {
            return;
        }
        self.sort_node(records, varying);
    }

    /// Finishes a privately staged bucket, using its available record capacity.
    /// The larger tile allowance is local to this operation and cursor-bounded.
    #[cfg(feature = "rayon")]
    fn sort_staged_child(&mut self, records: &mut [T], candidate: u128) {
        Sorter::<T, E> {
            scratch: self.scratch,
            extractor: self.extractor,
            tile_len: self
                .tile_len
                .max(records.len())
                .min(crate::thresholds::MAX_TILE_RECORDS),
        }
        .sort_child(records, candidate);
    }

    /// Routes a slice whose order has already been probed.
    fn sort_node(&mut self, records: &mut [T], varying: u128) {
        let len = records.len();
        if len < 2 || varying == 0 {
            return;
        }
        if len <= self.tile_len {
            self.scratch
                .tile
                .sort::<E>(records, varying, self.extractor);
            return;
        }
        let plan = plan_node(records, varying, self.tile_len, self.extractor);
        self.execute_node(records, varying, plan);
    }

    /// Executes an already selected partition; parallel fallbacks do not resample.
    fn execute_node(&mut self, records: &mut [T], varying: u128, plan: NodePlan<E::Value>) {
        match plan {
            NodePlan::Equal(equal) => self.partition_equal(records, equal, varying),
            NodePlan::Runs { boundary, set } => self.split_ordered(records, boundary, set, varying),
            NodePlan::Digit(digit) => self.partition_digit(records, digit, varying),
        }
    }

    /// Partitions by digit, then sorts each unfinished child.
    fn partition_digit(&mut self, records: &mut [T], digit: Digit, varying: u128) {
        // Bucket boundaries come from the block engine. The split digit is
        // constant inside each child, so its bits are excluded from the next scan.
        let candidate = varying & !digit.mask::<E::Value>().to_u128();
        // A digit covering all remaining variation leaves constant buckets;
        // interchangeable records can be counted instead of moved.
        if candidate == 0
            && self
                .extractor
                .try_count_window(records, digit.shift, digit.width)
        {
            return;
        }
        let tile_len = self.tile_len;
        let mut block_buffers = core::mem::take(&mut self.scratch.block);
        let mut visit = |child: &mut [T], extractor: &mut &mut E| {
            let extractor = &mut **extractor;
            if child.len() <= tile_len {
                let varying = scan_varying_bits(child, candidate, extractor);
                self.scratch.tile.sort::<E>(child, varying, extractor);
            }
        };
        let ends = block::partition_digit(
            records,
            digit.width,
            digit.shift,
            &mut *self.extractor,
            &mut block_buffers,
            &mut visit,
        );
        self.scratch.block = block_buffers;
        let mut start = 0;
        for &end in &ends {
            if end > start {
                let child = &mut records[start..end];
                if child.len() > tile_len {
                    self.sort_child(child, candidate);
                }
            }
            start = end;
        }
        self.scratch.block.recycle_ends(ends);
    }

    /// Partitions below, equal to, and above a sampled key. The equal band is done.
    fn partition_equal(&mut self, records: &mut [T], equal: KeyValue<T, E>, parent_varying: u128) {
        let extractor = &mut *self.extractor;
        // The same rank digit as the parallel root. ByKey caches classification
        // and declines filling: equal ranks do not identify record payloads.
        let rank = ByKey(move |record: &T| equality_rank(extractor.key(record), equal));
        let ends =
            block::partition_digit(records, 2, 0, rank, &mut self.scratch.block, &mut |_, _| {});
        if ends[0] == records.len() || ends[1] == 0 {
            // A changing key can leave the whole node below or above the
            // sampled pivot. Force a digit partition to remove candidate bits
            // rather than repeat the same equality partition indefinitely.
            self.scratch.block.recycle_ends(ends);
            let plan = plan_progress(records, parent_varying, self.tile_len, self.extractor);
            self.execute_node(records, parent_varying, plan);
            return;
        }
        for range in [0..ends[0], ends[1]..records.len()] {
            let child = &mut records[range];
            if child.len() > 1 {
                self.sort_child(child, parent_varying);
            }
        }
        self.scratch.block.recycle_ends(ends);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn confirmation_preserves_direction_and_ties() {
        for len in 1..=6 {
            for mut encoded in 0..3usize.pow(len) {
                let records: Vec<u64> = (0..len)
                    .map(|_| {
                        let key = (encoded % 3) as u64;
                        encoded /= 3;
                        key
                    })
                    .collect();
                let expected = if records.windows(2).all(|pair| pair[0] <= pair[1]) {
                    Order::Ascending
                } else if records.windows(2).all(|pair| pair[0] >= pair[1]) {
                    Order::Descending {
                        strict: records.windows(2).all(|pair| pair[0] > pair[1]),
                    }
                } else {
                    Order::Unsorted
                };
                assert!(
                    confirm_order(&records, &mut Direct) == expected,
                    "{records:?}"
                );
                assert!(
                    confirm_order(&records, &mut ByKey(|key: &u64| *key)) == expected,
                    "{records:?}"
                );
            }
        }
    }

    #[test]
    fn confirm_order_finds_unsampled_inversion() {
        let len = if cfg!(miri) { 130 } else { 8192 };
        let mut values: Vec<_> = (0..len as u64).collect();
        let index = (len / 2) & !1;
        values.swap(index, index + 1);
        // At 8192 records the strided gate checks odd pair offsets. Miri
        // exercises the confirmation directly on a shorter vector scan.
        assert!(confirm_order(&values, &mut Direct) == Order::Unsorted);
        crate::stable_sort(&mut values);
        assert_eq!(values, (0..len as u64).collect::<Vec<_>>());
    }

    // Tiny tiles exercise block materialization and cache-hot callbacks under
    // Miri without requiring thousands of large records.
    #[test]
    fn key_panic_keeps_borrowed_records() {
        let len = if cfg!(miri) { 65 } else { 513 };
        let mut total = 0;
        let key = |id: usize| (id as u64).wrapping_mul(0x9e3779b97f4a7c15);
        {
            let mut backing: Vec<_> = (0..len).collect();
            let mut probe: Vec<_> = backing.iter_mut().enumerate().collect();
            sort_with_tile_bytes::<_, _>(
                &mut probe,
                ByKey(|x: &(usize, &mut usize)| {
                    total += 1;
                    key(x.0)
                }),
                128,
            );
        }
        let steps = if cfg!(miri) { 4 } else { 64 };
        for changing in [false, true] {
            for step in 0..steps {
                let at = 1 + step * total / steps;
                let mut values: Vec<_> = (0..len).collect();
                let mut records: Vec<_> = values.iter_mut().enumerate().collect();
                let mut calls = 0;
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    sort_with_tile_bytes::<_, _>(
                        &mut records,
                        ByKey(|x: &(usize, &mut usize)| {
                            calls += 1;
                            if !changing && calls == at {
                                panic!("key failed");
                            }
                            if changing && calls >= at {
                                calls as u64 % 3
                            } else {
                                key(x.0)
                            }
                        }),
                        128,
                    );
                }));
                if !changing {
                    assert!(outcome.is_err(), "unreached panic at {at}/{total}");
                    assert_eq!(calls, at);
                }
                records.sort_by_key(|x| x.0);
                for (id, (identity, value)) in records.iter_mut().enumerate() {
                    assert_eq!(*identity, id);
                    assert_eq!(**value, id);
                    **value += len;
                }
                drop(records);
                assert_eq!(values, (len..2 * len).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn variation_stays_within_candidate() {
        assert_eq!(
            scan_varying_bits(&[0u64, u64::MAX], 0xff, &mut Direct),
            0xff
        );
    }

    #[test]
    fn variation_covers_late_bits_and_key_widths() {
        for len in [1, 64, 65, 192, 193, 1025] {
            let mut records = alloc::vec![7u128; len];
            records[len - 1] |= 1 << 127;
            for candidate in [0, 0xff, u128::MAX] {
                let expected = records
                    .iter()
                    .fold(0, |bits, key| bits | (key ^ records[0]));
                assert_eq!(
                    scan_varying_bits(&records, candidate, &mut Direct),
                    expected & candidate
                );
            }
            let narrow = alloc::vec![17u8; len];
            assert_eq!(scan_varying_bits(&narrow, u8::MAX as u128, &mut Direct), 0);
        }
    }

    #[test]
    fn changing_keys_finish_node() {
        for greater in [false, true] {
            let len = if cfg!(miri) { 65 } else { 129 };
            let mut records: Vec<_> = (0..len as u16).collect();
            let mut scratch = Scratch::new();
            let mut calls = 0;
            // Each equality pass puts every record in one non-equal band.
            // The child scan restores all bits; the next sample picks the
            // same absent pivot. Stop safely before a broken sort exhausts
            // the stack.
            let mut extractor = ByKey(|_: &u16| -> u16 {
                assert!(calls < 12_000, "node recursion did not make progress");
                let phase = calls % (len + 4 + 65 + plan::KEY_SAMPLE_LEN);
                calls += 1;
                if phase < len {
                    1
                } else if phase < len + 4 {
                    // Reject order before the candidate scan and mode sample.
                    [1, 0, 0, 1][phase - len]
                } else if phase < len + 4 + 65 {
                    if phase.is_multiple_of(2) { 0 } else { 0x1ff }
                } else if greater {
                    0
                } else {
                    0x1ff
                }
            });
            Sorter::<_, _> {
                scratch: &mut scratch,
                extractor: &mut extractor,
                tile_len: tile_capacity::<u16>(32),
            }
            .partition_equal(&mut records, if greater { 0 } else { 0x1ff }, 0x1ff);
            records.sort_unstable();
            assert_eq!(records, (0..len as u16).collect::<Vec<_>>());
        }
    }
}
