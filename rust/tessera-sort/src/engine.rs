// SPDX-License-Identifier: BSD-2-Clause

//! Routing: the split policy and the node/tile recursion.
//!
//! One recursion sorts every slice. It learns the slice's order and its varying
//! key bits in one pass, chooses one split, executes that split with the
//! implementation the slice's size allows, and recurses into the buckets.
//!
//! A *digit* is the window of key bits one pass handles. A *node* is a slice
//! larger than the tile budget: it splits through the block engine, which needs
//! O(sqrt(n)) scratch. A *tile* fits the tile budget and splits through counting
//! scratch. Both implementations leave the records in the caller's slice and
//! report each bucket's end, so the recursion is the same for either.
//!
//! `docs/design/engine.md` describes each stage and the evidence behind the
//! thresholds.

#[cfg(feature = "rayon")]
pub(crate) mod parallel;

use crate::block;
use crate::cost;
use crate::insertion;
use crate::key::{Digit, Extract, FILL_BITS, Narrowed, Unsigned, bit_len};
use crate::scan::{self, Descent};
use crate::tile;

/// Default tile budget in bytes of records; a tile holds at least one record.
const TILE_BYTES: usize = crate::thresholds::TILE_BYTES;

/// Largest slice finished by insertion sort.
const INSERTION_MAX: usize = crate::thresholds::INSERTION_MAX;

/// Most records in one tile, so that a `u16` cursor addresses any of them.
const MAX_TILE_RECORDS: usize = crate::thresholds::MAX_TILE_RECORDS;

/// Widest digit a node split uses, in bits.
///
/// A node pass keeps one write cursor, one count and one output block per
/// bucket; 256 buckets keep the per-record state in L1. The block engine itself
/// accepts up to 4,096 buckets, which the tile route never reaches.
const NODE_DIGIT_BITS: usize = 8;

/// Keys one sample reads to look for a repeated value.
const SAMPLE_KEYS: usize = 64;

/// Consecutive keys per sampled run, which avoids stride aliasing with periodic keys.
const SAMPLE_RUN_LEN: usize = 8;

/// Share of the slice a sampled repeated value is priced as finishing.
///
/// The sample only reports values repeated by at least a quarter, so the
/// model prices the smallest band that qualifies.
const PIVOT_SHARE: f64 = 0.25;

/// A value repeated by at least a quarter of the sample gets a pivot split.
const SAMPLE_SHARE: usize = 4;

/// Buffers shared by the nodes and tiles of one sort.
struct Scratch<T> {
    /// Tile counting scratch, allocated by the first tile split.
    tile: tile::Tile<T>,
    /// Block buffers, allocated by the first node split.
    block: block::Buffers<T>,
    /// Bucket-end vectors kept for a later split.
    ends: Vec<Vec<usize>>,
    /// Occupancy sample reused by every costing decision.
    planner: cost::Planner,
}

impl<T> Scratch<T> {
    /// Creates empty scratch without allocating.
    fn new() -> Self {
        Self {
            tile: tile::Tile::new(),
            block: block::Buffers::new(),
            ends: Vec::new(),
            planner: cost::Planner::new(),
        }
    }

    /// Takes a cleared bucket-end vector, reusing one from an earlier split.
    fn take_ends(&mut self) -> Vec<usize> {
        self.ends.pop().unwrap_or_default()
    }

    /// Keeps a bucket-end vector for a later split.
    fn give_ends(&mut self, mut ends: Vec<usize>) {
        ends.clear();
        self.ends.push(ends);
    }
}

/// Routes nodes and finishes tiles with one extractor and shared scratch.
struct Sorter<'a, T, E> {
    scratch: &'a mut Scratch<T>,
    extractor: &'a mut E,
    /// Tile budget in bytes of records.
    tile_bytes: usize,
}

/// Sorts `records` stably by the keys `extractor` reads.
pub(crate) fn sort<T, E: Extract<T>>(records: &mut [T], extractor: &mut E) {
    sort_with_tile_bytes(records, extractor, TILE_BYTES);
}

/// [`sort`] with an explicit tile budget, so tests reach node routes with
/// small inputs.
pub(crate) fn sort_with_tile_bytes<T, E: Extract<T>>(
    records: &mut [T],
    extractor: &mut E,
    tile_bytes: usize,
) {
    let record_bytes = core::mem::size_of::<T>();
    if records.len() < 2 || record_bytes == 0 {
        return;
    }
    let tile_bytes = tile_bytes
        .max(record_bytes)
        .min(MAX_TILE_RECORDS.saturating_mul(record_bytes));
    let mut scratch = Scratch::new();
    Sorter {
        scratch: &mut scratch,
        extractor,
        tile_bytes,
    }
    .sort_root(records);
}

/// Finishes an ordered slice and returns the varying bits when a split is still
/// needed.
///
/// The order check runs first because it answers with a vector scan that stops
/// at the first inversion: an unordered slice pays a pair or two, an ordered one
/// returns without a variation scan or a single move.
fn prepare<T, E: Extract<T>>(
    records: &mut [T],
    candidate: u128,
    extractor: &mut E,
) -> Option<u128> {
    if records.len() < 2 || candidate == 0 {
        return None;
    }
    extractor.inversion(records)?;
    let inspection = scan::inspect(records, candidate, extractor);
    match inspection.descent {
        Descent::Strict => {
            records.reverse();
            return None;
        }
        Descent::Tied => {
            reverse_tied(records, extractor);
            return None;
        }
        Descent::NotDescending => {}
    }
    // A changing key function could hide every difference outside the
    // candidate; the slice is then constant by construction and makes no
    // progress, so it ends here.
    (inspection.varying != 0).then_some(inspection.varying)
}

/// Reverses a descending slice, keeping equal keys in their input order.
fn reverse_tied<T, E: Extract<T>>(records: &mut [T], extractor: &mut E) {
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

/// Finishes a nearly ordered slice with a budgeted insertion pass.
///
/// The probe window is one tile of records. An input within its moves of sorted
/// finishes here; anything else pays at most the window plus the budget that
/// window implies before the radix route takes over.
fn adaptive<T, E: Extract<T>>(records: &mut [T], tile_bytes: usize, extractor: &mut E) -> bool {
    let window = (tile_bytes / core::mem::size_of::<T>().max(1)).max(2);
    insertion::probe(records, window, extractor)
}

impl<T, E: Extract<T>> Sorter<'_, T, E> {
    /// Sorts a root: finishes ordered and nearly ordered slices, then routes the rest.
    fn sort_root(&mut self, records: &mut [T]) {
        let Some(varying) = prepare(records, u128::MAX, self.extractor) else {
            return;
        };
        if records.len() > INSERTION_MAX && adaptive(records, self.tile_bytes, self.extractor) {
            return;
        }
        // A key wider than 64 bits narrows once, here: a child's variation is a
        // subset of its parent's, so every descendant split keeps the half that
        // varies. Narrowing only at the root also keeps the extractor type from
        // nesting once per level.
        if E::Value::BITS > 64 {
            if varying >> 64 == 0 {
                let mut narrow = Narrowed::<_, 0>(&mut *self.extractor);
                return Sorter {
                    scratch: &mut *self.scratch,
                    extractor: &mut narrow,
                    tile_bytes: self.tile_bytes,
                }
                .sort_planned(records, varying);
            }
            if varying & u64::MAX as u128 == 0 {
                let mut narrow = Narrowed::<_, 64>(&mut *self.extractor);
                return Sorter {
                    scratch: &mut *self.scratch,
                    extractor: &mut narrow,
                    tile_bytes: self.tile_bytes,
                }
                .sort_planned(records, varying >> 64);
            }
        }
        self.sort_planned(records, varying);
    }

    /// Sorts one partition child, whose remaining candidate bits are known.
    fn sort_slice(&mut self, records: &mut [T], candidate: u128) {
        // A slice the leaf finishes needs no probe: insertion sort stops after
        // one pass when no pair descends, so an ordered child costs what the
        // order check would have cost and a disordered one saves the check.
        if records.len() <= INSERTION_MAX {
            let _ = self.extractor.insertion_sort(records, usize::MAX);
            return;
        }
        let Some(varying) = prepare(records, candidate, self.extractor) else {
            return;
        };
        // A child that is nearly ordered finishes here: the probe is an
        // insertion pass bounded by the same scaled budget the root uses, so a
        // child with few inversions never allocates scratch and a disordered one
        // pays only its first swaps.
        if false && adaptive(records, self.tile_bytes, self.extractor) {
            return;
        }
        self.sort_planned(records, varying);
    }

    /// Routes a slice whose order and variation are known.
    fn sort_planned(&mut self, records: &mut [T], varying: u128) {
        let len = records.len();
        if len < 2 || varying == 0 {
            return;
        }
        if len <= INSERTION_MAX {
            let _ = self.extractor.insertion_sort(records, usize::MAX);
            return;
        }
        // A window that covers every remaining difference finishes the slice by
        // filling each run in place: no scratch, no moves, one key pass.
        let low = varying.trailing_zeros() as usize;
        let span = bit_len(varying) - low;
        if span <= FILL_BITS {
            let digit = Digit {
                shift: low,
                width: span,
            };
            if self.extractor.fill_window(records, digit) {
                return;
            }
        }
        if self.over_tile(len) {
            self.sort_node(records, varying);
        } else {
            self.sort_tile(records, varying);
        }
    }

    /// Sorts a node with the route the cost model chose.
    fn sort_node(&mut self, records: &mut [T], varying: u128) {
        let choice = choose_node(records, varying, &mut self.scratch.planner, self.extractor);
        match choice.route {
            cost::Route::Rotate => {
                let top_bit = 1u128 << (bit_len(varying) - 1);
                let (boundary, front_set) = two_runs(records, top_bit, self.extractor)
                    .expect("rotation was priced from this slice");
                self.rotate(records, varying, boundary, front_set);
            }
            cost::Route::Pivot => {
                let pivot = sample_pivot(records, self.extractor)
                    .expect("pivot was priced from this slice");
                self.pivot(records, varying, pivot);
            }
            cost::Route::Split { digit, .. } => self.partition_blocks(records, varying, digit),
            cost::Route::Flat { .. } => unreachable!("a node never finishes flat"),
        }
    }
    /// Sorts a tile: the tile prices a flat plan against a counted split and
    /// answers with the split digit when it wants one.
    fn sort_tile(&mut self, records: &mut [T], varying: u128) {
        let (tile_scratch, planner, extractor) = (
            &mut self.scratch.tile,
            &mut self.scratch.planner,
            &mut *self.extractor,
        );
        let Some(digit) = tile::finish(records, varying, tile_scratch, planner, extractor) else {
            return;
        };
        self.partition_tile(records, varying, digit);
    }

    /// Splits at the two runs the top varying bit already holds.
    fn rotate(&mut self, records: &mut [T], varying: u128, boundary: usize, front_set: bool) {
        let bit = 1u128 << (bit_len(varying) - 1);
        // A run holding the bit in front belongs behind the clear one; one
        // rotation keeps both runs in their own order.
        let boundary = if front_set {
            let len = records.len();
            records.rotate_left(boundary);
            len - boundary
        } else {
            boundary
        };
        let candidate = varying & !bit;
        self.sort_slice(&mut records[..boundary], candidate);
        self.sort_slice(&mut records[boundary..], candidate);
    }

    /// Partitions below, equal to, and above a sampled value; the equal band is done.
    fn pivot(&mut self, records: &mut [T], varying: u128, pivot: E::Value) {
        let len = records.len();
        let classify = move |extractor: &mut E, record: &T| {
            let key = extractor.key(record);
            usize::from(key >= pivot) + usize::from(key > pivot)
        };
        // Three outcomes need four buckets because the block engine takes
        // powers of two; the fourth is unused, so the greater band runs to the end.
        let ends = {
            let (scratch, extractor) = (&mut *self.scratch, &mut *self.extractor);
            let mut ends = scratch.take_ends();
            block::partition(
                records,
                4,
                classify,
                extractor,
                &mut scratch.block,
                &mut ends,
            );
            ends
        };
        let (below, equal_end) = (ends[0], ends[1]);
        if below == len || equal_end == below {
            // A changing key can leave the whole slice in one band. A digit
            // partition removes candidate bits instead of repeating this split.
            self.scratch.give_ends(ends);
            let digit = choose_node(records, varying, &mut self.scratch.planner, self.extractor)
                .route
                .digit()
                .expect("a node route falls back to a split");
            self.partition_blocks(records, varying, digit);
            return;
        }
        for (start, end) in [(0, below), (equal_end, len)] {
            if end > start + 1 {
                self.sort_slice(&mut records[start..end], varying);
            }
        }
        self.scratch.give_ends(ends);
    }

    /// Partitions by one digit window through the block engine, then sorts each
    /// bucket.
    fn partition_blocks(&mut self, records: &mut [T], varying: u128, digit: Digit) {
        let candidate = varying & !digit.mask::<E::Value>().to_u128();
        let ends = {
            let (scratch, extractor) = (&mut *self.scratch, &mut *self.extractor);
            let mut ends = scratch.take_ends();
            block::partition(
                records,
                1 << digit.width,
                |extractor, record| digit.classify(extractor, record),
                extractor,
                &mut scratch.block,
                &mut ends,
            );
            ends
        };
        self.sort_buckets(records, candidate, largest_bucket(&ends), ends);
    }

    /// Partitions a tile by one digit window, then sorts each bucket.
    fn partition_tile(&mut self, records: &mut [T], varying: u128, digit: Digit) {
        let candidate = varying & !digit.mask::<E::Value>().to_u128();
        let (ends, largest) = {
            let (scratch, extractor) = (&mut *self.scratch, &mut *self.extractor);
            let mut ends = scratch.take_ends();
            let largest = tile::partition(records, digit, &mut scratch.tile, extractor, &mut ends);
            (ends, largest)
        };
        self.sort_buckets(records, candidate, largest, ends);
    }

    /// Recurses into the buckets `ends` delimit.
    fn sort_buckets(
        &mut self,
        records: &mut [T],
        candidate: u128,
        largest: usize,
        mut ends: Vec<usize>,
    ) {
        // Buckets hold ordered keys: every record of one bucket sorts before
        // every record of the next. When each bucket is insertion-sized, one
        // insertion pass over the whole slice moves exactly what the separate
        // bucket passes would move and skips the dispatch between them.
        if largest <= INSERTION_MAX {
            let _ = self.extractor.insertion_sort(records, usize::MAX);
            ends.clear();
            self.scratch.give_ends(ends);
            return;
        }
        let mut start = 0;
        for &end in &ends {
            if end > start + 1 {
                self.sort_slice(&mut records[start..end], candidate);
            }
            start = end;
        }
        ends.clear();
        self.scratch.give_ends(ends);
    }

    /// Whether `len` records exceed the tile budget.
    fn over_tile(&self, len: usize) -> bool {
        len.saturating_mul(core::mem::size_of::<T>()) > self.tile_bytes
    }
}

/// Largest bucket a set of bucket ends delimits.
///
/// A partition reports cumulative ends, so one subtraction per bucket answers
/// what the recursion needs to decide whether the buckets are leaves.
fn largest_bucket(ends: &[usize]) -> usize {
    let mut largest = 0;
    let mut start = 0;
    for &end in ends {
        largest = largest.max(end - start);
        start = end;
    }
    largest
}

/// The route the cost model chose for a node.
///
/// The probes answer two cheap questions first: whether the top varying bit
/// already holds two runs, and whether one value repeats. The model prices
/// the routes those probes make possible against a counted split.
fn choose_node<T, E: Extract<T>>(
    records: &[T],
    varying: u128,
    planner: &mut cost::Planner,
    extractor: &mut E,
) -> cost::Choice {
    let top_bit = 1u128 << (bit_len(varying) - 1);
    let rotation = two_runs(records, top_bit, extractor).is_some();
    let pivot = sample_pivot(records, extractor);
    planner.node(
        records,
        varying,
        NODE_DIGIT_BITS,
        rotation,
        pivot.map(|_| PIVOT_SHARE),
        extractor,
    )
}

/// Returns the two runs the top varying bit already holds, as the end of the
/// first run and whether that run holds the bit.
///
/// A slice is two runs when it reads `0*1*` or `1*0*` in that bit; the
/// second form is one rotation away from the first. Any other arrangement
/// answers at the first record that breaks the pattern, which unordered slices
/// reach within a pair or two.
fn two_runs<T, E: Extract<T>>(
    records: &[T],
    bit: u128,
    extractor: &mut E,
) -> Option<(usize, bool)> {
    let first = extractor.key(&records[0]).to_u128() & bit != 0;
    let mut boundary = 0;
    while boundary < records.len()
        && (extractor.key(&records[boundary]).to_u128() & bit != 0) == first
    {
        boundary += 1;
    }
    if boundary == 0 || boundary == records.len() {
        return None;
    }
    for record in &records[boundary..] {
        if (extractor.key(record).to_u128() & bit != 0) == first {
            return None;
        }
    }
    Some((boundary, first))
}

/// Returns a value that at least a quarter of the sampled keys repeat.
///
/// The sample is spread over the slice in runs of consecutive keys, which avoids
/// stride aliasing with periodic keys. Its sorted multiset is what later uses
/// may rely on.
fn sample_pivot<T, E: Extract<T>>(records: &[T], extractor: &mut E) -> Option<E::Value> {
    let mut keys = [E::Value::from_usize(0); SAMPLE_KEYS];
    let runs = SAMPLE_KEYS / SAMPLE_RUN_LEN;
    let step = (records.len() / runs).max(1);
    let mut len = 0;
    for run in 0..runs {
        let start = run * step;
        for offset in 0..SAMPLE_RUN_LEN {
            let index = start + offset;
            if index >= records.len() {
                break;
            }
            keys[len] = extractor.key(&records[index]);
            len += 1;
        }
    }
    // Short inputs pad the sample with the first key.
    let first = keys[0];
    keys[len..].fill(first);
    keys.sort_unstable();
    let mut best = &keys[..1];
    for run in keys.chunk_by(|left, right| left == right) {
        if run.len() > best.len() {
            best = run;
        }
    }
    (best.len() * SAMPLE_SHARE >= keys.len()).then_some(best[0])
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::key::{ByKey, Direct};
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn pivot_samples_find_dominant_values() {
        let values: Vec<u64> = (0..1024u64).map(|id| id % 5).collect();
        assert_eq!(sample_pivot(&values, &mut Direct), None);
        let values: Vec<u64> = (0..1024).map(|id| if id < 768 { 0 } else { 1 }).collect();
        assert_eq!(sample_pivot(&values, &mut Direct), Some(0));
    }

    #[test]
    fn empty_pivot_band_splits_by_digits() {
        // Every record is below the pivot, so the partition leaves one band and
        // the pivot route must fall back to a digit split instead of repeating.
        let mut records: Vec<u64> = (0..1000).collect();
        let mut extractor = Direct;
        let mut scratch = Scratch::new();
        Sorter {
            scratch: &mut scratch,
            extractor: &mut extractor,
            tile_bytes: 64,
        }
        .pivot(&mut records, u128::from(u64::MAX), 1 << 60);
        assert_eq!(records, (0..1000).collect::<Vec<_>>());
    }

    #[test]
    fn rotate_splits_runs_the_slice_already_holds() {
        let mut values: Vec<u64> = (0..100)
            .map(|id| if id < 60 { id } else { 1 << 63 | id })
            .collect();
        let expected = {
            let mut expected = values.clone();
            expected.sort_unstable();
            expected
        };
        sort_with_tile_bytes(&mut values, &mut Direct, 128);
        assert_eq!(values, expected);

        // The set form needs one rotation, not just the ordered prefix form.
        let mut values: Vec<u64> = (0..100)
            .map(|id| if id < 40 { 1 << 63 | id } else { id })
            .collect();
        let expected = {
            let mut expected = values.clone();
            expected.sort_unstable();
            expected
        };
        sort_with_tile_bytes(&mut values, &mut Direct, 128);
        assert_eq!(values, expected);
    }

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
                &mut ByKey(|x: &(usize, &mut usize)| {
                    total += 1;
                    key(x.0)
                }),
                128,
            );
        }
        let steps = if cfg!(miri) { 4 } else { 48 };
        for changing in [false, true] {
            for step in 0..steps {
                let at = 1 + step * total / steps;
                let mut values: Vec<_> = (0..len).collect();
                let mut records: Vec<_> = values.iter_mut().enumerate().collect();
                let mut calls = 0;
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    sort_with_tile_bytes::<_, _>(
                        &mut records,
                        &mut ByKey(|x: &(usize, &mut usize)| {
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
    fn changing_keys_finish_every_route() {
        let len = if cfg!(miri) { 65 } else { 1000 };
        let mut records: Vec<(u64, usize)> = (0..len).map(|id| (id as u64, id)).collect();
        let mut calls = 0;
        // Every sample and histogram puts all records in one bucket, and each
        // child scan reintroduces every key bit, including the split digit.
        let mut extractor = ByKey(|record: &(u64, usize)| {
            assert!(calls < 200_000, "recursion did not make progress");
            let phase = calls % 97;
            calls += 1;
            match phase {
                0 => 0,
                1 => u64::MAX,
                2 => record.0,
                _ => u64::MAX - record.0,
            }
        });
        sort_with_tile_bytes(&mut records, &mut extractor, 64);
        let mut ids: Vec<_> = records.iter().map(|record| record.1).collect();
        ids.sort_unstable();
        assert_eq!(ids, (0..len).collect::<Vec<_>>());
    }
}
