// SPDX-License-Identifier: BSD-2-Clause

//! Routing: order probe, variation scan, node partitions and tile leaves.
//!
//! A *node* is a slice larger than the tile budget (64 KiB of records by
//! default); [`block`] splits it with a stable block partition. A *tile* fits
//! the budget and is finished by [`tile`] with insertion, a tile-local MSD
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
#[cfg(test)]
mod refinement_tests;

use crate::tile;

use crate::order::scan_varying_bits;
use crate::rank::Requirement;
use crate::thresholds::{INSERTION_MAX, TILE_BYTES};
use crate::workspace::Workspace;
use plan::{NodePlan, plan_node};
use tile::capacity as tile_capacity;

use alloc::vec::Vec;

use crate::block;
use crate::block::Classify;
use crate::key::{self, Digit, Extract, KeyDigit, KeyHalf, Rank, Unsigned, bit_len};

type KeyValue<T, E> = <E as Extract<T>>::Value;

/// The candidate bits left when every child holds the top bit of a nonzero
/// `varying` constant, as after a run or equality split.
fn without_top_bit(varying: u128) -> u128 {
    varying & !(1 << (bit_len(varying) - 1))
}

/// How a node partition leaves its children.
#[derive(Clone, Copy)]
struct Split {
    /// Bits of the classifier's bucket index.
    radix: usize,
    /// Key bits that may still vary inside each unfinished child.
    candidate: u128,
    /// A bucket that is already complete, such as an equality partition's
    /// equal band.
    finished: Option<usize>,
}

impl Split {
    /// The split an equality classification leaves: the equal bucket is
    /// complete and every other bucket holds the top varying bit constant, so
    /// every unfinished child consumes it, as after a digit or run split.
    fn equality<E, V: Unsigned>(rank: &Rank<E, V>, varying: u128) -> Self {
        Self {
            radix: 2,
            candidate: without_top_bit(varying),
            finished: Some(rank.equal_bucket()),
        }
    }

    /// Partitions all records and finishes requested tile-sized children
    /// while they are cache-hot, using the extractor `lend` returns for child
    /// work. Returns each bucket's end; larger children remain.
    fn partition<T, E: Extract<T>, C: Classify<T>>(
        self,
        records: &mut [T],
        classifier: C,
        lend: fn(&mut C) -> &mut E,
        scratch: &mut Workspace<T>,
        tile_len: usize,
        required: Requirement<'_>,
    ) -> Vec<usize> {
        let tile = &mut scratch.tile;
        let mut visit =
            |bucket: usize, range: core::ops::Range<usize>, child: &mut [T], classifier: &mut C| {
                if child.len() <= tile_len
                    && self.finished != Some(bucket)
                    && required.project(range).is_some()
                {
                    tile.finish_child::<_>(child, self.candidate, lend(classifier));
                }
            };
        block::partition(
            records,
            self.radix,
            classifier,
            &mut scratch.block,
            &mut visit,
        )
    }
}

/// Refines requested ranks through shared node and tile decisions.
/// Unrequested bands have completed movement but receive no further key work.
struct Sorter<'a, T, E> {
    scratch: &'a mut Workspace<T>,
    extractor: &'a mut E,
    tile_len: usize,
}

/// Sorts the complete input through the shared ordered-refinement entry.
pub(crate) fn sort<T, E: Extract<T>>(records: &mut [T], extractor: E) {
    let ranks = 0..records.len();
    let required = Requirement::new(records.len(), core::slice::from_ref(&ranks));
    refine(records, extractor, required, None);
}

/// Refines requested ranks with invocation-owned reusable capacity.
///
/// Root preparation and half-key narrowing are performed once here, not in
/// algorithm adapters. Empty requirements do no key work. A borrowed workspace
/// retains capacity for the owning invocation; otherwise fresh storage is
/// constructed only after preparation. The engine chooses its default tile
/// budget; ordered and constant roots do not allocate or initialize unused
/// movement storage.
pub(crate) fn refine<T, E: Extract<T>>(
    records: &mut [T],
    extractor: E,
    required: Requirement<'_>,
    workspace: Option<&mut Workspace<T>>,
) {
    refine_with_tile_bytes(records, extractor, required, workspace, TILE_BYTES);
}

/// Executes refinement with an explicit budget, shared by the production entry
/// and ownership tests that need node routes with small inputs.
fn refine_with_tile_bytes<T, E: Extract<T>>(
    records: &mut [T],
    mut extractor: E,
    required: Requirement<'_>,
    workspace: Option<&mut Workspace<T>>,
    tile_bytes: usize,
) {
    let record_bytes = core::mem::size_of::<T>();
    if records.len() < 2 || record_bytes == 0 {
        return;
    }
    let Some(required) = required.project(0..records.len()) else {
        return;
    };
    let tile_len = tile_capacity::<T>(tile_bytes);
    let Some(varying) = prepare::<_, _>(records, &mut extractor, tile_len) else {
        return;
    };
    let mut local;
    let workspace = match workspace {
        Some(workspace) => workspace,
        None => {
            local = Workspace::new();
            &mut local
        }
    };
    if E::Value::BITS > 64 {
        if varying >> 64 == 0 {
            Sorter::<T, _> {
                scratch: workspace,
                extractor: &mut KeyHalf::<_, 0>(extractor),
                tile_len,
            }
            .sort_node(records, varying, required);
            return;
        }
        if varying & ((1u128 << 64) - 1) == 0 {
            Sorter::<T, _> {
                scratch: workspace,
                extractor: &mut KeyHalf::<_, 64>(extractor),
                tile_len,
            }
            .sort_node(records, varying >> 64, required);
            return;
        }
    }
    Sorter::<T, E> {
        scratch: workspace,
        extractor: &mut extractor,
        tile_len,
    }
    .sort_node(records, varying, required);
}

#[cfg(test)]
fn sort_with_tile_bytes<T, E: Extract<T>>(records: &mut [T], extractor: E, tile_bytes: usize) {
    let ranks = 0..records.len();
    let required = Requirement::new(records.len(), core::slice::from_ref(&ranks));
    refine_with_tile_bytes(records, extractor, required, None, tile_bytes);
}

/// Finishes ordered or small roots, otherwise returns their varying bits.
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
    if sampled == Order::Unsorted && insertion_probe::<_, _>(records, extractor, tile_len) {
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
/// A small tile finishes by stable insertion. For larger inputs, the shared
/// insertion operation owns the probe window and swap budget; a failed pass
/// leaves a stable permutation of its input.
fn insertion_probe<T, E: Extract<T>>(
    records: &mut [T],
    extractor: &mut E,
    tile_len: usize,
) -> bool {
    // A failed limited pass on a small leaf would scan varying bits only to
    // select comparison finishing again. Keep its result unit-returning, so
    // stable direct insertion discards the unused count inside SIMD dispatch.
    if records.len() <= INSERTION_MAX && records.len() <= tile_len {
        extractor.finish_insertion(records);
        return true;
    }
    extractor.try_insertion(records, tile_len)
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
    fn split_ordered(
        &mut self,
        records: &mut [T],
        boundary: usize,
        set: bool,
        varying: u128,
        required: Requirement<'_>,
    ) {
        // Rotation preserves each run's internal order.
        let boundary = if set {
            let len = records.len();
            records.rotate_left(boundary);
            len - boundary
        } else {
            boundary
        };
        let candidate = without_top_bit(varying);
        let len = records.len();
        if let Some(child) = required.project(0..boundary) {
            self.sort_child(&mut records[..boundary], candidate, child);
        }
        if let Some(child) = required.project(boundary..len) {
            self.sort_child(&mut records[boundary..], candidate, child);
        }
    }

    /// Finishes a child using remaining variation, order and the shared node policy.
    fn sort_child(&mut self, records: &mut [T], candidate: u128, required: Requirement<'_>) {
        if records.len() < 2 || candidate == 0 {
            return;
        }
        // Ordered children need only one full scan. Flat samples retain the
        // varying-bit scan, which can finish constant children cheaply.
        let sampled = probe_order(records, self.extractor);
        if finish_ordered(records, sampled, self.extractor) {
            return;
        }
        let varying = scan_varying_bits(records, candidate, self.extractor);
        if varying == 0 {
            return;
        }
        if sampled == Order::Flat {
            let order = confirm_order(records, self.extractor);
            if finish_ordered(records, order, self.extractor) {
                return;
            }
        }
        self.sort_node(records, varying, required);
    }

    /// Routes a slice whose order has already been probed.
    fn sort_node(&mut self, records: &mut [T], varying: u128, required: Requirement<'_>) {
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
        self.execute_node(records, varying, plan, required);
    }

    /// Executes an already selected partition; parallel fallbacks do not resample.
    fn execute_node(
        &mut self,
        records: &mut [T],
        varying: u128,
        plan: NodePlan<E::Value>,
        required: Requirement<'_>,
    ) {
        match plan {
            NodePlan::Equal(equal) => self.partition_equal(records, equal, varying, required),
            NodePlan::Runs { boundary, set } => {
                self.split_ordered(records, boundary, set, varying, required)
            }
            NodePlan::Digit(digit) => self.partition_digit(records, digit, varying, required),
        }
    }

    /// Partitions by digit, then sorts each unfinished child.
    fn partition_digit(
        &mut self,
        records: &mut [T],
        digit: Digit,
        varying: u128,
        required: Requirement<'_>,
    ) {
        // The split digit is constant inside each child, so its bits are
        // excluded from the next scan.
        let candidate = varying & !digit.mask::<E::Value>().to_u128();
        // A digit covering all remaining variation leaves constant buckets;
        // interchangeable records can be counted instead of moved.
        if candidate == 0 && key::try_count_window::<T, E>(records, digit.shift, digit.width) {
            return;
        }
        let split = Split {
            radix: digit.width,
            candidate,
            finished: None,
        };
        let ends = split.partition::<_, _, _>(
            records,
            KeyDigit::new(&mut *self.extractor, digit.shift),
            |digit| &mut *digit.extractor,
            self.scratch,
            self.tile_len,
            required,
        );
        self.finish_children(records, ends, split, required);
    }

    /// Partitions below, equal to, and above a sampled key, splitting the
    /// outer band that crosses the top varying bit. The equal band is done.
    fn partition_equal(
        &mut self,
        records: &mut [T],
        equal: KeyValue<T, E>,
        varying: u128,
        required: Requirement<'_>,
    ) {
        let rank = Rank::new(&mut *self.extractor, equal, varying);
        let split = Split::equality(&rank, varying);
        let ends = split.partition::<_, _, _>(
            records,
            rank,
            |rank| &mut *rank.extractor,
            self.scratch,
            self.tile_len,
            required,
        );
        self.finish_children(records, ends, split, required);
    }

    /// Continues requested large children after parent movement has retired.
    /// Requested tiles were finished cache-hot; unrelated bands remain untouched.
    fn finish_children(
        &mut self,
        records: &mut [T],
        ends: Vec<usize>,
        split: Split,
        required: Requirement<'_>,
    ) {
        let mut start = 0;
        for (bucket, &end) in ends.iter().enumerate() {
            if end - start > self.tile_len
                && split.finished != Some(bucket)
                && let Some(child) = required.project(start..end)
            {
                self.sort_child(&mut records[start..end], split.candidate, child);
            }
            start = end;
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::key::{ByKey, Direct};
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
        crate::sort(&mut values);
        assert_eq!(values, (0..len as u64).collect::<Vec<_>>());
    }

    #[test]
    #[cfg_attr(miri, ignore = "sampling adds no distinct ownership boundary")]
    fn child_flat_sample_does_not_certify_order() {
        let mut records = alloc::vec![0u64; 3 * ORDER_SAMPLES];
        // At stride three, index four belongs to neither sampled pair.
        records[4] = 1;
        let mut expected = records.clone();
        expected.sort();
        let len = records.len();
        let mut scratch = Workspace::new();
        Sorter {
            scratch: &mut scratch,
            extractor: &mut Direct,
            tile_len: tile_capacity::<u64>(TILE_BYTES),
        }
        .sort_child(
            &mut records,
            u64::MAX as u128,
            Requirement::new(len, core::slice::from_ref(&(0..len))),
        );
        assert_eq!(records, expected);
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

    /// Places a new, smaller key on exactly the records each node samples, so
    /// an equality split peels only its sampled records and leaves every
    /// other record in one outer band. Equality splits that keep the top
    /// varying bit nest once per peel: quadratic work and linear depth.
    #[test]
    fn peeled_equality_chains_stay_shallow() {
        const TILE_BYTES: usize = 128;
        let levels = if cfg!(miri) { 4 } else { 200 };
        let min_sampled = 4 * tile_capacity::<u64>(TILE_BYTES);
        let len = min_sampled + levels * plan::KEY_SAMPLE_LEN;
        let mut remaining: Vec<usize> = (0..len).collect();
        let mut keys = alloc::vec![0u64; len];
        let mut pivot = u64::MAX;
        while remaining.len() >= min_sampled + plan::KEY_SAMPLE_LEN {
            let sampled: Vec<_> = (0..plan::KEY_SAMPLE_LEN)
                .map(|stratum| plan::sample_index(remaining.len(), stratum))
                .collect();
            for &index in sampled.iter().rev() {
                keys[remaining.remove(index)] = pivot;
            }
            pivot -= 1;
        }
        // Unordered keys below every pivot keep their band unordered.
        for (index, &slot) in remaining.iter().enumerate() {
            keys[slot] = (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 8;
        }
        let mut records: Vec<_> = keys.iter().copied().enumerate().collect();
        let mut calls = 0usize;
        sort_with_tile_bytes(
            &mut records,
            ByKey(|record: &(usize, u64)| {
                calls += 1;
                record.1
            }),
            TILE_BYTES,
        );
        assert!(records.windows(2).all(|pair| pair[0].1 <= pair[1].1));
        assert!(
            records
                .windows(2)
                .all(|pair| pair[0].1 < pair[1].1 || pair[0].0 < pair[1].0)
        );
        // Each node level reads a key a few times per record; the bound
        // depends on key width, not on the number of peels.
        assert!(calls < 64 * len, "{calls} key calls for {len} records");
    }

    #[test]
    fn changing_pivot_keeps_owned_records() {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;

        struct Owned {
            id: usize,
            text: String,
            key: Cell<u16>,
            drops: Rc<RefCell<Vec<usize>>>,
        }
        impl Drop for Owned {
            fn drop(&mut self) {
                self.drops.borrow_mut().push(self.id);
            }
        }

        for greater in [false, true] {
            let len = if cfg!(miri) { 65 } else { 129 };
            let pivot = if greater { 0 } else { 0x1ff };
            let drops = Rc::new(RefCell::new(Vec::new()));
            let mut records: Vec<_> = (0..len)
                .map(|id| Owned {
                    id,
                    text: format!("record {id}"),
                    key: Cell::new(1),
                    drops: drops.clone(),
                })
                .collect();
            let mut scratch = Workspace::new();
            let planning = Cell::new(true);
            let mut extractor = ByKey(|record: &Owned| {
                if planning.get() {
                    return pivot;
                }
                // Every record first enters the same outer band. Later keys
                // differ only in the bit the equality split consumed, so
                // continued sorting must leave their original order intact.
                record.key.replace((record.id as u16 % 2) << 8)
            });
            let tile_len = tile_capacity::<Owned>(128);
            // Select the real equality plan, then change keys at its execution
            // boundary. No fixture needs to count probes, samples or scans.
            let plan = plan_node(&records, 0x1ff, tile_len, &mut extractor);
            assert!(matches!(plan, NodePlan::Equal(equal) if equal == pivot));
            planning.set(false);
            Sorter::<_, _> {
                scratch: &mut scratch,
                extractor: &mut extractor,
                tile_len,
            }
            .execute_node(
                &mut records,
                0x1ff,
                plan,
                Requirement::new(len, core::slice::from_ref(&(0..len))),
            );

            assert!(drops.borrow().is_empty());
            for (id, record) in records.iter().enumerate() {
                assert_eq!(record.id, id);
                assert_eq!(record.text, format!("record {id}"));
                assert_eq!(record.key.get(), (id as u16 % 2) << 8);
            }
            // The owned records remain usable through another public key width.
            crate::sort_by_key(&mut records, |record| (record.id as u128) << 64);
            assert!(
                records
                    .iter()
                    .enumerate()
                    .all(|(id, record)| record.id == id)
            );
            drop(records);
            drops.borrow_mut().sort_unstable();
            assert_eq!(*drops.borrow(), (0..len).collect::<Vec<_>>());
        }
    }
}
