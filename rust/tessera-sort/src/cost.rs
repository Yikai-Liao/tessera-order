// SPDX-License-Identifier: BSD-2-Clause

//! The cost model: the one place that prices plans and chooses between them.
//!
//! Routing asks this module what to do and executes the answer. Nothing else in
//! the engine decides how wide a window is, when a tile finishes flat, or when a
//! rotation beats a split, so a policy change is a change here.
//!
//! # What a plan is
//!
//! A plan finishes a slice in one of four ways, and the engine executes whichever
//! the model prices lowest:
//!
//! - [`Route::Rotate`]: the top varying bit already holds two runs, so one rotation
//!   separates them and both runs are then ordered or recursively split;
//! - [`Route::Pivot`]: one repeated value is split off, its band is finished by
//!   the pass, and the two outer bands keep the recursion;
//! - [`Route::Split`]: one counted pass by a window of key bits, then the
//!   recursion over the buckets;
//! - [`Route::Flat`]: one counting pass per window of varying bits, least
//!   significant first, which finishes the slice with no recursion at all.
//!
//! # Prices
//!
//! A plan is priced in estimated cycles per record of the slice from five
//! measured terms: a key read, a record move, a bucket slot, a prepared child,
//! and an insertion step. The weights below are estimates. When a decision looks
//! wrong, the fix is a weight or a new term here -- never a branch that names a
//! distribution.
//!
//! The model prices the loops the engine runs and ignores what those loops do
//! not depend on. It deliberately does not model branch prediction, cache
//! locality or alignment: two plans with equal prices are then chosen by the
//! order they are priced in, which [`node`] and [`tile`] fix to prefer the plan
//! that moves fewer records.

use crate::key::{Digit, Extract, Unsigned, bit_len};
use crate::thresholds::INSERTION_MAX;

/// Records a child may hold and still be finished by insertion sort.
///
/// Measured: at 48 records the quadratic leaf still beats another counting pass,
/// which pays a key read, a move, a table and a prepared child per record.
pub(crate) const LEAF: usize = INSERTION_MAX;

/// Widest window the model prices, in bits.
///
/// The cursor table costs two bytes per bucket, so 14 bits trade 32 KiB of
/// scratch for a small number of passes over a slice.
pub(crate) const MAX_DIGIT_BITS: usize = 14;

/// Most buckets a window of that width holds.
pub(crate) const MAX_BUCKETS: usize = 1 << MAX_DIGIT_BITS;

/// Narrowest window a flat plan uses, in bits: below it the cursor table costs
/// more passes than it saves.
pub(crate) const MIN_PLAN_BITS: usize = 6;

/// Windows a plan may need for a 128-bit key at the narrowest window.
pub(crate) const MAX_PASSES: usize = 128 / MIN_PLAN_BITS + 1;

/// Keys one occupancy sample reads.
const SAMPLE_LEN: usize = 128;

/// Cycles for one key read when the record is its own key: a load.
const KEY_LOAD: f64 = 1.0;

/// Cycles for one key read that runs a user key function.
///
/// A call into unknown code costs several loads even when the function is small.
const KEY_CALL: f64 = 8.0;

/// Cycles for one bucket slot: a cursor update and an index computation.
const BUCKET: f64 = 1.0;

/// Cycles of fixed work per prepared child: probe, scan setup and call.
const CHILD: f64 = 8.0;

/// Bytes one cycle of record movement covers.
///
/// A move is a read and a write of the record, so it scales with its width; the
/// unit keeps an eight-byte record at one cycle.
const MOVE_BYTES: usize = 8;

/// Cycles a recursion node spends sampling and pricing before it does work.
///
/// The sample reads 128 keys at a stride and the pricing walks the candidate
/// widths, so the cost is fixed per node whatever its size. A tile of a few
/// hundred records then spends more on its decision than on its pass, which is
/// what keeps the model from cutting slices far below the tile budget.
const PLANNING: f64 = 1_500.0;

/// Share of the uniform-spread expectation a sample must reach to count as spread.
///
/// A window whose sample repeats far more than a spread would is concentrated and
/// its occupied buckets are the ones the sample names. Sampling noise alone moves
/// a spread sample by a fraction of the expectation, so half is safely inside it.
const SPREAD_TENTHS: usize = 5;

/// The shape of a slice that a plan has to finish.
#[derive(Clone, Copy)]
pub(crate) struct Slice {
    /// Records in the slice.
    pub(crate) records: usize,
    /// Bytes per record; moves scale with it.
    pub(crate) record_bytes: usize,
    /// Whether a key read is a load or a call.
    pub(crate) key_source: KeySource,
}

/// Where a key read comes from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeySource {
    /// The record holds its own encoded key: one load.
    Sealed,
    /// The caller's function computes the key: several loads and a call.
    Function,
}

/// How a slice is finished. The engine executes the route; the model only
/// chooses it and reports its price.
#[derive(Clone, Copy)]
pub(crate) enum Route {
    /// One rotation at the top varying bit.
    Rotate,
    /// Split off a repeated value; its band leaves the recursion.
    Pivot,
    /// One counted split by `digit` with `fanout` occupied buckets.
    Split { digit: Digit, fanout: usize },
    /// One counting pass per window of `width` bits, least significant first.
    Flat {
        width: usize,
        passes: usize,
        buckets: usize,
    },
}

/// The route the model chose, with its price for diagnostics and tests.
#[derive(Clone, Copy)]
pub(crate) struct Choice {
    pub(crate) route: Route,
    pub(crate) cost: f64,
    /// Whether the shape alone decided: a spread sample leaves nothing for
    /// the keys to answer, so the same shape decides the same way again.
    shape_only: bool,
}

impl Route {
    /// Digit of a split route, for callers that only split.
    pub(crate) fn digit(&self) -> Option<Digit> {
        match *self {
            Route::Split { digit, .. } => Some(digit),
            _ => None,
        }
    }
}

impl Slice {
    /// A slice of `records` records of `record_bytes` bytes.
    pub(crate) fn new(records: usize, record_bytes: usize, key_source: KeySource) -> Self {
        Self {
            records,
            record_bytes,
            key_source,
        }
    }

    /// Cycles for one key read.
    fn key_cost(&self) -> f64 {
        match self.key_source {
            KeySource::Sealed => KEY_LOAD,
            KeySource::Function => KEY_CALL,
        }
    }

    /// Cycles for one record move.
    fn move_cost(&self) -> f64 {
        self.record_bytes as f64 / MOVE_BYTES as f64
    }

    /// Cycles per record for one counting pass: every key read and every move.
    pub(crate) fn pass_cost(&self) -> f64 {
        self.key_cost() + self.move_cost()
    }

    /// Cycles per record for a bucket table of `buckets` slots.
    fn table_cost(&self, buckets: usize) -> f64 {
        BUCKET * buckets as f64 / self.records.max(1) as f64
    }

    /// Cycles per record to finish a leaf by insertion.
    ///
    /// An insertion pass moves a record past a quarter of its run on average and
    /// compares keys while doing it, so the price does not depend on the run
    /// length: every level that reaches the leaf pays it once per record.
    pub(crate) fn leaf_cost(&self) -> f64 {
        0.25 * self.pass_cost()
    }

    /// Cycles per record of a flat plan: `passes` counting passes over a table of
    /// `buckets` slots.
    fn flat_cost(&self, passes: usize, buckets: usize) -> f64 {
        self.pass_cost() * passes as f64 + self.table_cost(buckets)
    }

    /// Cycles per record of a counted split into `fanout` buckets, its children
    /// prepared and split again until they reach the leaf.
    ///
    /// A child the leaf finishes is never prepared: the insertion pass is its
    /// probe, so the probe is charged only where another split follows.
    fn split_cost(&self, fanout: usize) -> f64 {
        let fanout = fanout.max(2);
        let records = self.records.max(1) as f64;
        let mut child = records / fanout as f64;
        // The caller already paid to plan this slice; its children plan only
        // where they are too large for the leaf.
        let mut cost = self.pass_cost() + self.table_cost(fanout);
        while child > LEAF as f64 {
            cost += self.prepare_cost(child)
                + self.pass_cost()
                + self.table_cost(fanout)
                + self.plan_cost(records / child);
            child /= fanout as f64;
        }
        cost + self.leaf_cost()
    }

    /// Cycles per record spent planning the `nodes` slices of a level.
    ///
    /// Every slice above the leaf pays its own sample and prices its own routes
    /// before it does any work, so a level that holds many small slices is not
    /// free: the same sample that guides a 4,096-record tile costs as much as
    /// sorting a hundred records, and a slice of a hundred records pays it per
    /// record.
    fn plan_cost(&self, nodes: f64) -> f64 {
        PLANNING * nodes / self.records.max(1) as f64
    }

    /// Cycles per record of a rotation: every record moves once, and the two runs
    /// then cost what their own shapes cost.
    fn rotate_cost(&self) -> f64 {
        self.move_cost() + self.pass_cost()
    }

    /// Cycles per record of a pivot pass that finishes a band of `share` of the
    /// records and leaves the rest for another route.
    fn pivot_cost(&self, share: f64) -> f64 {
        let outer = (1.0 - share).clamp(0.0, 1.0);
        self.pass_cost() + outer * self.pass_cost()
    }

    /// Cycles per record of the recursion over one level's children.
    fn prepare_cost(&self, child: f64) -> f64 {
        let children = (self.records as f64 / child.max(1.0)).ceil();
        (CHILD + 2.0 * self.key_cost()) * children / self.records.max(1) as f64
    }

    /// Prices `route` for this slice.
    pub(crate) fn price(&self, route: Route) -> f64 {
        match route {
            Route::Rotate => self.rotate_cost(),
            Route::Pivot => self.pivot_cost(0.0),
            Route::Split { fanout, .. } => self.split_cost(fanout),
            Route::Flat {
                passes, buckets, ..
            } => self.flat_cost(passes, buckets),
        }
    }
}

/// Chooses how to finish a tile: a flat plan or one counted split.
///
/// A tile is finished by whichever is cheaper for its own keys. The flat plan
/// needs one pass per window of varying bits and no recursion, which is the
/// cheapest way to sort a tile of unrelated keys. A counted split wins when the
/// window leaves few occupied buckets, because its children reach the insertion
/// leaf in a pass or two.
pub(crate) fn tile(slice: &Slice, varying: u128, occupancy: &mut Occupancy) -> Choice {
    let width = table_bits(slice.records, slice.record_bytes);
    let mut digits = [Digit { shift: 0, width: 0 }; MAX_PASSES];
    let passes = plan_flat(varying, width, &mut digits);
    // The flat plan walks one table per pass; its widest window bounds them all.
    let widest = digits[..passes]
        .iter()
        .map(|digit| digit.width)
        .max()
        .unwrap_or(1);
    let flat = Route::Flat {
        width,
        passes,
        buckets: 1usize << widest,
    };
    let (split, shape_only) = split_route(slice, varying, width, occupancy);
    // Ties go to the flat plan: it moves each record fewer times and needs no
    // prepared children.
    let route = if slice.price(flat) <= slice.price(split) {
        flat
    } else {
        split
    };
    Choice {
        cost: slice.price(route),
        route,
        shape_only,
    }
}

/// Chooses how to finish a node from the candidates its probes found.
///
/// The probes are cheap and answer two questions: whether the top varying bit
/// already holds two runs, and whether one value repeats. A rotation needs no
/// scratch, and a pivot finishes its own band, so both are priced against a
/// counted split rather than preferred by rule.
pub(crate) fn node(
    slice: &Slice,
    varying: u128,
    max_width: usize,
    rotation: bool,
    pivot_share: Option<f64>,
    occupancy: &mut Occupancy,
) -> Choice {
    let (split, shape_only) = split_route(slice, varying, max_width, occupancy);
    let mut best = Choice {
        cost: slice.price(split),
        route: split,
        shape_only,
    };
    if rotation && slice.price(Route::Rotate) < best.cost {
        best = Choice {
            route: Route::Rotate,
            cost: slice.price(Route::Rotate),
            shape_only: false,
        };
    }
    if let Some(share) = pivot_share {
        let cost = slice.pivot_cost(share);
        if cost < best.cost {
            best = Choice {
                route: Route::Pivot,
                cost,
                shape_only: false,
            };
        }
    }
    best
}

/// Widest cursor table a slice can afford, in bits.
///
/// The buffers and the cursor table are comparable in bytes, so the table may use
/// half the slice: two bytes per bucket. A cap in records keeps the cleared,
/// counted and walked slots within a small multiple of the slice.
pub(crate) fn table_bits(records: usize, record_bytes: usize) -> usize {
    let bytes = records.saturating_mul(record_bytes);
    let widest = (bytes / 2).max(1).ilog2() as usize;
    // Empty-bucket work stays within eight times the record count, or four for
    // records wider than a machine word, where the same table holds half as many
    // records and more of its buckets stay empty. Measured on 16-byte records
    // with 10M random keys: 8.0% faster at the wider cap (15 of 15 rounds; the
    // A/A pair of that case disagreed by 23.6%, so the direction is the result,
    // not its size).
    let slack = if record_bytes > 8 { 2 } else { 3 };
    let by_records = records.max(1).ilog2() as usize + slack;
    widest.min(by_records).clamp(MIN_PLAN_BITS, MAX_DIGIT_BITS)
}

/// Packs windows of the varying bits, highest first, and returns how many.
///
/// A window spans at most `width` bits. When every bit below the top one varies,
/// the run is tiled evenly over the passes that `width` would need, so 32 bits in
/// three passes become 11 + 11 + 10 rather than 14 + 14 + 4: both plans move each
/// record three times, but the even one clears, counts and walks 3,074 bucket
/// slots instead of 32,784. Gaps keep the wide windows, which jump over them.
pub(crate) fn plan_flat(varying: u128, width: usize, digits: &mut [Digit]) -> usize {
    let top = bit_len(varying);
    // Passes left to tile the contiguous run over; None keeps fixed windows.
    let mut even = (varying.count_ones() as usize == top).then_some(top.div_ceil(width));
    let mut remaining = varying;
    let mut len = 0;
    while remaining != 0 {
        let window_width = match even {
            Some(left) if left > 0 => bit_len(remaining).div_ceil(left),
            _ => width,
        };
        let digit = Digit::top(remaining, window_width);
        debug_assert!(len < digits.len() && digit.width <= width);
        digits[len] = digit;
        len += 1;
        remaining &= !digit.mask::<u128>();
        even = even.map(|left| left.saturating_sub(1));
    }
    len
}

/// The cheapest counted split for a slice, bounded by `max_width` bits.
fn split_route(
    slice: &Slice,
    varying: u128,
    max_width: usize,
    occupancy: &mut Occupancy,
) -> (Route, bool) {
    let span = bit_len(varying) - varying.trailing_zeros() as usize;
    let widest = max_width.min(span).clamp(1, MAX_DIGIT_BITS);
    let mut best = split_at(varying, widest, slice, occupancy);
    // A spread window leaves nothing for the keys to answer, and narrowing it
    // would only raise the depth of the descent.
    if spread(best, slice.records) {
        return (best, true);
    }
    // A concentrated window leaves the keys in charge: the narrow windows are
    // priced from the buckets the sample found.
    let mut best_cost = slice.price(best);
    for width in 1..widest {
        let candidate = split_at(varying, width, slice, occupancy);
        let cost = slice.price(candidate);
        if cost < best_cost {
            best = candidate;
            best_cost = cost;
        }
    }
    (best, spread(best, slice.records))
}

/// Whether a split window separates the slice as far as the window allows.
///
/// A spread window leaves nothing for the keys to answer, so a decision that
/// lands on one is settled by the slice's shape alone.
fn spread(route: Route, records: usize) -> bool {
    match route {
        Route::Split { digit, fanout } => fanout >= (1usize << digit.width).min(records),
        _ => false,
    }
}

/// A split window of `width` bits with its measured occupancy.
fn split_at(varying: u128, width: usize, slice: &Slice, occupancy: &mut Occupancy) -> Route {
    let digit = Digit::top(varying, width);
    let fanout = occupancy.occupancy(slice.records, digit);
    Route::Split { digit, fanout }
}

/// The decisions of one sort run.
///
/// Owns the sampler and remembers the last decision the shape alone settled: a
/// recursion that meets the same shape again reuses it instead of sampling and
/// pricing again, which matters where one level holds tens of thousands of
/// slices of equal size. A planner serves one record type and one key source,
/// which are fixed for a sort run.
pub(crate) struct Planner {
    occupancy: Occupancy,
    memo: Option<((usize, u128, usize), Choice)>,
}

impl Planner {
    /// Creates a planner without allocating.
    pub(crate) fn new() -> Self {
        Self {
            occupancy: Occupancy::new(),
            memo: None,
        }
    }

    /// Chooses how to finish a tile: one flat plan or one counted split.
    ///
    /// Samples the keys once, and not at all when the shape already decided.
    pub(crate) fn tile<T, E: Extract<T>>(
        &mut self,
        records: &[T],
        varying: u128,
        extractor: &mut E,
    ) -> Choice {
        let slice = Slice::new(
            records.len(),
            core::mem::size_of::<T>(),
            if E::key_function() {
                KeySource::Function
            } else {
                KeySource::Sealed
            },
        );
        let key = (
            slice.records,
            varying,
            table_bits(slice.records, slice.record_bytes),
        );
        self.choose(key, |occupancy| {
            occupancy.sample(records, extractor);
            tile(&slice, varying, occupancy)
        })
    }

    /// Chooses how to finish a node from its probes and the model's prices.
    pub(crate) fn node<T, E: Extract<T>>(
        &mut self,
        records: &[T],
        varying: u128,
        max_width: usize,
        rotation: bool,
        pivot_share: Option<f64>,
        extractor: &mut E,
    ) -> Choice {
        let slice = Slice::new(
            records.len(),
            core::mem::size_of::<T>(),
            if E::key_function() {
                KeySource::Function
            } else {
                KeySource::Sealed
            },
        );
        let key = (slice.records, varying, max_width);
        self.choose(key, |occupancy| {
            occupancy.sample(records, extractor);
            node(&slice, varying, max_width, rotation, pivot_share, occupancy)
        })
    }

    /// Answers `decide` for `key`, reusing a remembered shape-only decision.
    fn choose(
        &mut self,
        key: (usize, u128, usize),
        decide: impl FnOnce(&mut Occupancy) -> Choice,
    ) -> Choice {
        if let Some((cached, choice)) = self.memo
            && cached == key
        {
            return choice;
        }
        let choice = decide(&mut self.occupancy);
        self.memo = choice.shape_only.then_some((key, choice));
        choice
    }
}

/// Sampled occupancy, reused across decisions.
///
/// Every decision that prices a split needs the spread of records over a window.
/// One sample answers for every candidate width, and a generation stamp per
/// bucket keeps the table reusable without clearing it: only the buckets a sample
/// touched change, so a decision costs one key read per sampled record and
/// nothing else.
pub(crate) struct Occupancy {
    keys: [u128; SAMPLE_LEN],
    len: usize,
    /// One bit per bucket of the window being measured, cleared per measurement.
    seen: Vec<u64>,
}

impl Occupancy {
    /// Creates an unsampled table without allocating.
    pub(crate) fn new() -> Self {
        Self {
            keys: [0; SAMPLE_LEN],
            len: 0,
            seen: Vec::new(),
        }
    }

    /// Replaces the sample with evenly spaced keys of `records`.
    pub(crate) fn sample<T, E: Extract<T>>(&mut self, records: &[T], extractor: &mut E) {
        // A small slice cannot afford a full sample: the decision would cost more
        // than the pass it guides, and a handful of keys already separates spread
        // keys from repeated ones.
        let len = SAMPLE_LEN.min((records.len() / 8).max(4));
        let step = (records.len() / len.max(1)).max(1);
        let mut slot = 0;
        for index in (0..records.len()).step_by(step).take(len) {
            self.keys[slot] = extractor.key(&records[index]).to_u128();
            slot += 1;
        }
        self.len = slot;
    }

    /// Occupied buckets of `digit` in the whole slice.
    ///
    /// A uniform spread over `buckets` buckets puts about
    /// `buckets * (1 - (1 - 1/buckets)^sampled)` of the sampled keys in distinct
    /// buckets, so that is what the sample is compared against. A sample that
    /// reaches most of it is consistent with a spread, and the slice then occupies
    /// as many buckets as it has records or the window holds. A sample far below
    /// it repeats buckets the window cannot separate, so those buckets are what
    /// the slice occupies.
    pub(crate) fn occupancy(&mut self, records: usize, digit: Digit) -> usize {
        let buckets = (1usize << digit.width).min(records.max(1)).max(2);
        // A bit per bucket of the window: the sample touches one word per
        // bucket, and clearing the window costs at most a few hundred stores.
        let words = (1usize << digit.width).div_ceil(64);
        if self.seen.len() < words {
            self.seen.resize(MAX_BUCKETS.div_ceil(64), 0);
        }
        self.seen[..words].fill(0);
        let mut distinct = 0;
        for &key in &self.keys[..self.len] {
            let bucket = digit.slot(key);
            let word = &mut self.seen[bucket >> 6];
            let bit = 1u64 << (bucket & 63);
            if *word & bit == 0 {
                *word |= bit;
                distinct += 1;
            }
        }
        if distinct == 0 {
            return 2;
        }
        let spread = uniform_distinct(buckets, self.len);
        if distinct * 10 >= spread * SPREAD_TENTHS {
            buckets
        } else {
            distinct.max(2)
        }
    }
}

/// Distinct buckets `keys` keys occupy when spread uniformly over `buckets`.
///
/// `buckets * (1 - (1 - 1/buckets)^keys)`: the bucket fraction a
/// spread fills, computed by repeated squaring in 1/1048576 units so a
/// window far wider than the sample keeps its small deficit.
fn uniform_distinct(buckets: usize, keys: usize) -> usize {
    if buckets <= 1 {
        return 1;
    }
    const UNIT: u64 = 1 << 20;
    let buckets = buckets as u64;
    // (1 - 1/buckets) in 1/UNIT units.
    let mut factor = ((buckets - 1) * UNIT / buckets).max(1);
    let mut empty = UNIT;
    let mut remaining = keys;
    while remaining > 0 {
        if remaining & 1 == 1 {
            empty = empty * factor / UNIT;
        }
        factor = factor * factor / UNIT;
        remaining >>= 1;
    }
    ((buckets * (UNIT - empty) / UNIT) as usize).max(1)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn slice(records: usize, record_bytes: usize) -> Slice {
        Slice::new(records, record_bytes, KeySource::Sealed)
    }

    #[test]
    fn uniform_spread_expectations_are_sane() {
        // 128 keys over 256 buckets leave about 101 of them occupied; the
        // integer arithmetic is within a few percent of the exact value.
        let occupied = uniform_distinct(256, 128);
        assert!((95..=110).contains(&occupied), "{occupied}");
        // A window wider than the sample holds every key in its own bucket.
        // A window far wider than the sample holds almost every key alone.
        let wide = uniform_distinct(16_384, 128);
        assert!((126..=128).contains(&wide), "{wide}");
        assert_eq!(uniform_distinct(2, 128), 2);
    }

    #[test]
    fn prices_grow_with_the_work_a_route_does() {
        let tile = slice(4096, 8);
        let one_pass = tile.price(Route::Flat {
            width: 8,
            passes: 1,
            buckets: 256,
        });
        let three_passes = tile.price(Route::Flat {
            width: 8,
            passes: 3,
            buckets: 256,
        });
        assert!(one_pass < three_passes);
        let wide = tile.price(Route::Flat {
            width: 13,
            passes: 1,
            buckets: 8192,
        });
        assert!(one_pass < wide);
        let split = tile.price(Route::Split {
            digit: Digit {
                shift: 0,
                width: 12,
            },
            fanout: 4096,
        });
        assert!(split > one_pass && split < three_passes);
    }

    #[test]
    fn windows_cover_every_varying_bit_once() {
        for &varying in &[
            1u128,
            0xff,
            1 << 63,
            (1 << 63) | 1,
            u64::MAX as u128,
            u128::MAX,
            0x00ff_00ff_00ff_00ff,
        ] {
            for &width in &[6usize, 8, 13, 14] {
                let mut digits = [Digit { shift: 0, width: 0 }; MAX_PASSES];
                let passes = plan_flat(varying, width, &mut digits);
                let mut covered = 0u128;
                for digit in &digits[..passes] {
                    assert!(digit.width <= width && digit.width > 0);
                    let mask = digit.mask::<u128>();
                    assert_eq!(mask & varying, mask, "window leaves the variation");
                    assert_eq!(mask & covered, 0, "windows overlap");
                    covered |= mask;
                }
                assert_eq!(covered, varying, "plan leaves bits uncovered");
            }
        }
    }
}
