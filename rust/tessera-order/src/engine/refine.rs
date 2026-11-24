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

mod indices;
pub(crate) use indices::{argsort, order_by_cached_indices};
mod plan;
#[cfg(test)]
mod refinement_tests;
mod spans;

use crate::engine::tile;

use crate::engine::order::scan_varying_bits;
use crate::engine::thresholds::{TILE_BYTES, insertion_limit};
use crate::engine::workspace::Workspace;
use plan::{NodePlan, plan_node};
use tile::capacity as tile_capacity;

use alloc::vec::Vec;

use crate::engine::block;
use crate::engine::block::Classify;
use crate::engine::key::{
    self, Digit, Direct, Extract, KeyDigit, KeyHalf, Rank, Unsigned, bit_len,
};

type KeyValue<T, E> = <E as Extract<T>>::Value;

/// Positions to finish in the current slice. Movement always completes every band.
#[derive(Clone, Copy)]
enum Requirement {
    All,
    Nth(usize),
}

impl Requirement {
    fn project(self, band: core::ops::Range<usize>) -> Option<Self> {
        match self {
            Self::All if !band.is_empty() => Some(Self::All),
            Self::Nth(index) if band.contains(&index) => Some(Self::Nth(index - band.start)),
            _ => None,
        }
    }
}

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
        required: Requirement,
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
    refine_with_tile_bytes(records, extractor, Requirement::All, TILE_BYTES);
}

/// Places the stable ranked record and returns the three disjoint regions.
pub(crate) fn select_nth<T, E: Extract<T>>(
    values: &mut [T],
    index: usize,
    extractor: E,
) -> (&mut [T], &mut T, &mut [T]) {
    assert!(index < values.len(), "selection index out of bounds");
    refine_with_tile_bytes(values, extractor, Requirement::Nth(index), TILE_BYTES);
    let (before, rest) = values.split_at_mut(index);
    let (selected, after) = rest.split_first_mut().unwrap();
    (before, selected, after)
}

/// Executes refinement with an explicit budget, shared by the production entry
/// and ownership tests that need node routes with small inputs.
fn refine_with_tile_bytes<T, E: Extract<T>>(
    records: &mut [T],
    mut extractor: E,
    required: Requirement,
    tile_bytes: usize,
) {
    let record_bytes = core::mem::size_of::<T>();
    if records.len() < 2 || record_bytes == 0 {
        return;
    }
    let tile_len = tile_capacity::<T>(tile_bytes);
    if let Requirement::Nth(index) = required
        && records.len() > tile_len
        && spans::SourceOrder::finish_cut(records, index, &mut extractor)
    {
        return;
    }
    let Some(varying) = prepare::<_, _>(records, &mut extractor, tile_len) else {
        return;
    };
    let workspace = &mut Workspace::new();
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
    refine_with_tile_bytes(records, extractor, Requirement::All, tile_bytes);
}

/// Finishes ordered or small roots, otherwise returns their varying bits.
/// Shared by serial and parallel entries before either allocates partition scratch.
fn prepare<T, E: Extract<T>>(
    records: &mut [T],
    extractor: &mut E,
    tile_len: usize,
) -> Option<u128> {
    prepare_with(
        records,
        extractor,
        None,
        &mut (),
        |r, order, e, _| finish_ordered(r, order, e),
        |r, e, _| insertion_probe::<_, _>(r, e, tile_len),
    )
}

/// Shared root-preparation policy: probes order, tries budgeted insertion on
/// unordered inputs, scans or reuses varying bits, and resolves flat samples.
fn prepare_with<R: AsRef<[T]>, T, E: Extract<T>, S>(
    mut records: R,
    extractor: &mut E,
    known_varying: Option<u128>,
    state: &mut S,
    mut finish: impl FnMut(&mut R, Order, &mut E, &mut S) -> bool,
    mut probe: impl FnMut(&mut R, &mut E, &mut S) -> bool,
) -> Option<u128> {
    let sampled = probe_order(records.as_ref(), extractor);
    if finish(&mut records, sampled, extractor, state)
        || (sampled == Order::Unsorted && probe(&mut records, extractor, state))
    {
        return None;
    }
    let varying = known_varying.unwrap_or_else(|| {
        let candidate = u128::MAX >> (128 - E::Value::BITS);
        scan_varying_bits(records.as_ref(), candidate, extractor)
    });
    if varying == 0 {
        return None;
    }
    if sampled == Order::Flat {
        let order = confirm_order(records.as_ref(), extractor);
        if finish(&mut records, order, extractor, state) {
            return None;
        }
    }
    Some(varying)
}

/// Probes one tile of keys on a long input and scales its nonzero swap count
/// to the whole slice's insertion budget.
fn probe_insertion_budget<T, E: Extract<T>>(
    records: &[T],
    extractor: &mut E,
    tile_len: usize,
) -> Option<usize> {
    let probe_len = tile_len.max(2).min(records.len());
    let mut keys: Vec<_> = records[..probe_len]
        .iter()
        .map(|record| extractor.key(record))
        .collect();
    // Bound failed probes without changing the unlimited insertion leaf.
    let swaps = Direct
        .insertion_sort(&mut keys, probe_len / 4)
        .filter(|&s| s > 0)?;
    // The successful probe bounds swaps by probe_len, so the quotient fits
    // usize even when the intermediate product does not.
    Some((swaps as u128 * records.len() as u128 / probe_len as u128) as usize)
}

/// Tries to finish `records` with a budgeted insertion pass and returns whether it
/// did.
///
/// A small tile finishes by stable insertion; another tile gets one
/// swap per four records. A longer input first insertion-sorts one tile of keys in a
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
    // A failed limited pass on a small leaf would scan varying bits only to
    // select comparison finishing again. Keep its result unit-returning, so
    // stable direct insertion discards the unused count inside SIMD dispatch.
    if records.len() <= insertion_limit::<T>().min(tile_len) {
        extractor.finish_insertion(records);
        return true;
    }
    if records.len() <= tile_len.max(2) {
        return extractor
            .insertion_sort(records, records.len() / 4)
            .is_some();
    }
    let Some(budget) = probe_insertion_budget(records, extractor, tile_len) else {
        return false;
    };
    extractor.insertion_sort(records, budget).is_some()
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

/// Target pair count for choosing the stride on large inputs. A contiguous
/// region covering a fraction `f` of the input meets about `f * ORDER_SAMPLES` pairs.
const ORDER_SAMPLES: usize = 4096;
/// Minimum stride: the sample reads at most two keys per this many records,
/// bounding its cost on ordered inputs, which also pay the confirming scan.
const ORDER_SAMPLE_MIN_STRIDE: usize = 128;
/// Short inputs confirm directly: their sample would hold fewer than 64 pairs
/// and could save at most part of one short scan.
const ORDER_SAMPLE_MIN_LEN: usize = 8192;

/// Confirms short inputs directly; larger inputs sample adjacent pairs before
/// confirming candidate order with a full scan.
#[inline(never)]
fn probe_order<T, E: Extract<T>>(records: &[T], extractor: &mut E) -> Order {
    if records.len() < ORDER_SAMPLE_MIN_LEN {
        return confirm_order(records, extractor);
    }
    let stride = (records.len() / ORDER_SAMPLES).max(ORDER_SAMPLE_MIN_STRIDE);
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
    // before starting the directional scan.
    let first = extractor.key(&records[0]);
    let mut tail = records[1..].iter();
    let Some(mut current) = tail.next().map(|record| extractor.key(record)) else {
        return Order::Ascending;
    };
    let mut ascending = first <= current;
    let mut descending = first >= current;
    let mut strict = first != current;
    if let Some(after) = tail.next().map(|record| extractor.key(record)) {
        ascending &= current <= after;
        descending &= current >= after;
        strict &= current != after;
        if !ascending && !descending {
            return Order::Unsorted;
        }
        current = after;
    }
    if ascending
        && descending
        && let Some((last, middle)) = tail.as_slice().split_last()
    {
        // An equal prefix leaves just one possible direction between the
        // endpoints. Equal endpoints require a constant middle.
        let last = extractor.key(last);
        if current == last {
            let anchor = current.to_u128();
            return if middle.chunks(64).all(|block| {
                block
                    .iter()
                    .fold(0, |diff, r| diff | (extractor.key(r).to_u128() ^ anchor))
                    == 0
            }) {
                Order::Ascending
            } else {
                Order::Unsorted
            };
        }
        ascending = current < last;
    }
    if ascending {
        // The prefix is already encoded and checked; scan only the remaining suffix.
        let tail = tail.as_slice();
        return if extractor.confirm_ascending(tail, current) {
            Order::Ascending
        } else {
            Order::Unsorted
        };
    }
    for record in tail {
        let after = extractor.key(record);
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
        required: Requirement,
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
    fn sort_child(&mut self, records: &mut [T], candidate: u128, required: Requirement) {
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
    fn sort_node(&mut self, records: &mut [T], varying: u128, required: Requirement) {
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
        if let Some(order) =
            spans::SourceOrder::for_node(records, varying, required, self.extractor, size_of::<T>())
        {
            order.apply(records);
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
        required: Requirement,
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
        required: Requirement,
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
        required: Requirement,
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
        required: Requirement,
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
    use crate::engine::key::ByKey;
    use core::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn endpoint_selection_preserves_callback_edits() {
        for rank in [0, 3] {
            let keys = if rank == 0 {
                [5, 5, 4, 1]
            } else {
                [9, 6, 5, 5]
            };
            let records = || core::array::from_fn::<_, 4, _>(|id| (id, Cell::new(keys[id])));
            let mut values = records();
            let tile_bytes = core::mem::size_of_val(&values) / 2;
            let peer = if rank == 0 { 1 } else { 2 };
            // The boundary probe stops at the opposite end. Only the winner
            // scan reaches the peer next to the displaced endpoint.
            let panic = catch_unwind(AssertUnwindSafe(|| {
                refine_with_tile_bytes(
                    &mut values,
                    ByKey(|(id, key): &(usize, Cell<u8>)| {
                        if *id == peer {
                            key.set(42);
                            std::panic::panic_any(*id);
                        }
                        key.get()
                    }),
                    Requirement::Nth(rank),
                    tile_bytes,
                );
            }))
            .unwrap_err();
            assert_eq!(*panic.downcast::<usize>().unwrap(), peer);
            assert_eq!(
                values.map(|(id, key)| (id, key.get())),
                core::array::from_fn(|id| (id, if id == peer { 42 } else { keys[id] }))
            );

            let mut values = records();
            let mut edits = [0; 4];
            refine_with_tile_bytes(
                &mut values,
                ByKey(|(id, key): &(usize, Cell<u8>)| {
                    edits[*id] += 1;
                    key.replace(key.get() + 1)
                }),
                Requirement::Nth(rank),
                tile_bytes,
            );
            values.sort_by_key(|r| r.0);
            // The opposite endpoint is read by both the boundary probe and
            // the winner scan and returns a different key the second time.
            assert!(edits[3 - rank] >= 2);
            assert_eq!(
                values.map(|(id, key)| (id, key.get())),
                core::array::from_fn(|id| (id, keys[id] + edits[id]))
            );
        }
    }

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
        let len = if cfg!(miri) {
            130
        } else {
            ORDER_SAMPLE_MIN_LEN
        };
        let mut values: Vec<_> = (0..len as u64).collect();
        // At the sampling threshold, pairs end at multiples of 128. The
        // swap sits between two pairs. Miri uses direct confirmation.
        let index = len / 2 + 1;
        values.swap(index, index + 1);
        assert!(confirm_order(&values, &mut Direct) == Order::Unsorted);
        crate::sort(&mut values);
        assert_eq!(values, (0..len as u64).collect::<Vec<_>>());
    }

    #[test]
    #[cfg_attr(miri, ignore = "sampling adds no distinct ownership boundary")]
    fn child_flat_sample_does_not_certify_order() {
        let mut records = alloc::vec![0u64; 3 * ORDER_SAMPLE_MIN_LEN];
        // Index four belongs to neither sampled pair at this stride.
        records[4] = 1;
        let mut expected = records.clone();
        expected.sort();
        let mut scratch = Workspace::new();
        Sorter {
            scratch: &mut scratch,
            extractor: &mut Direct,
            tile_len: tile_capacity::<u64>(TILE_BYTES),
        }
        .sort_child(&mut records, u64::MAX as u128, Requirement::All);
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
            .execute_node(&mut records, 0x1ff, plan, Requirement::All);

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
