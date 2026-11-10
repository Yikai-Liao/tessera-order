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

use alloc::vec::Vec;
use core::mem::MaybeUninit;

use crate::block;
use crate::key::{Direct, Extract, KeyHalf, Unsigned};

type KeyValue<T, E> = <E as Extract<T>>::Value;

/// Initial split width; also the block engine's tuned width.
const SPLIT_BITS: usize = 8;

/// Split fan-out is bounded by the block engine's widest digit.
const MAX_SPLIT_BITS: usize = block::MAX_RADIX_BITS;

/// At 14 bits the u16 cursor table occupies 32 KiB.
const MAX_TILE_DIGIT_BITS: usize = 14;
/// Maximum cursor table length.
const MAX_DIGIT_BUCKETS: usize = 1 << MAX_TILE_DIGIT_BITS;
/// Narrowest digit a leaf may use.
const MIN_TILE_DIGIT_BITS: usize = 6;
/// Default tile size in bytes of records; a tile always holds at least one record.
const TILE_BYTES: usize = crate::thresholds::TILE_BYTES;
/// Cost model: an MSD level is one counting pass, and the insertion finish that
/// ends the descent costs about one more. Charging a level as three passes kept
/// three-pass plans on flat passes, which measured 11-38% slower for 32-bit keys
/// at 1K-64K records.
const MSD_FINISH_PASSES: usize = 1;
/// Tiles and buckets at or below this many records are finished with an
/// insertion sort instead of paying for a bucket table per remaining digit.
const INSERTION_MAX: usize = crate::thresholds::INSERTION_MAX;
/// u16 cursors bound a tile to 65535 records.
const MAX_TILE_RECORDS: usize = crate::thresholds::MAX_TILE_RECORDS;
/// Bounded sample shared by equality and digit-occupancy decisions.
const KEY_SAMPLE_LEN: usize = 128;
/// Consecutive keys per run avoid aliasing with periodic input.
const SAMPLE_RUN_LEN: usize = 8;
/// A value occupying this fraction of the sample merits an equality partition.
const MODE_SHARE_DENOMINATOR: usize = 4;

/// Sorted sampled keys, independent of the digit currently being considered.
/// This is advisory evidence only: execution still scans actual child variation.
struct KeySample<V> {
    keys: [V; KEY_SAMPLE_LEN],
    len: usize,
}

impl<V: Unsigned> KeySample<V> {
    /// Reads bounded runs distributed across a nonempty input, then sorts the keys.
    fn collect<T, E: Extract<T, Value = V>>(records: &[T], extractor: &mut E) -> Self {
        let len = records.len().min(KEY_SAMPLE_LEN);
        let mut sample = Self {
            keys: [V::from_usize(0); KEY_SAMPLE_LEN],
            len,
        };
        let runs = len.div_ceil(SAMPLE_RUN_LEN);
        let stride = SAMPLE_RUN_LEN + (records.len() - len) / runs;
        for (run, keys) in sample.keys[..len].chunks_mut(SAMPLE_RUN_LEN).enumerate() {
            let start = run * stride;
            for (key, record) in keys.iter_mut().zip(&records[start..]) {
                *key = extractor.key(record);
            }
        }
        sample.keys[..len].sort_unstable();
        sample
    }

    /// Finds a sufficiently frequent complete key, not merely a repeated digit.
    fn mode(&self) -> Option<V> {
        self.keys[..self.len]
            .chunk_by(|a, b| a == b)
            .max_by_key(|run| run.len())
            .filter(|run| run.len() * MODE_SHARE_DENOMINATOR >= self.len)
            .map(|run| run[0])
    }

    /// Counts occupied top-digit buckets without a radix-sized bitmap.
    /// Above this digit keys are constant, so sorted sample projections are ordered.
    /// Inconsistent extractors may overestimate occupancy; no safety relies on it.
    fn distinct(&self, digit: Digit) -> usize {
        let mask = digit.mask::<V>();
        self.keys[..self.len]
            .chunk_by(|a, b| (*a & mask) == (*b & mask))
            .count()
    }
}

/// Stably inserts inverted records until sorted or the swap budget runs out.
/// A failed pass still leaves a stable permutation of the input.
///
/// The SIMD extractor calls this from a target-feature context. Keeping the
/// loop in that context avoids out-of-line calls for each vector operation.
#[inline(always)]
pub(crate) fn insertion_loop<T, E: Extract<T>>(
    records: &mut [T],
    budget: usize,
    extractor: &mut E,
) -> Option<usize> {
    let mut swaps = 0;
    let mut start = 0;
    while let Some(offset) = extractor.first_inversion(&records[start..]) {
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
pub(crate) fn finish_insertion<T, E: Extract<T>>(records: &mut [T], extractor: &mut E) {
    let _ = insertion_loop(records, usize::MAX, extractor);
}

/// Number of bits needed to represent `value`; 0 for 0.
#[inline]
fn bit_len(value: u128) -> usize {
    (u128::BITS - value.leading_zeros()) as usize
}

/// One pass worth of key bits: `width` bits starting at `shift`.
#[derive(Clone, Copy)]
struct Digit {
    shift: usize,
    width: usize,
}

impl Digit {
    #[inline]
    fn mask<V: Unsigned>(self) -> V {
        (V::from_usize((1usize << self.width) - 1)) << V::from_usize(self.shift)
    }
}

/// A minimum-pass plan cannot exceed the number of windows needed for all bits.
const MAX_DIGITS: usize = (u128::BITS as usize).div_ceil(MIN_TILE_DIGIT_BITS);

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
    while index < records.len() {
        let end = (index + 64).min(records.len());
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
    }
    (union.to_u128() ^ intersection.to_u128()) & candidate
}

/// The window over the highest remaining bits that a pass of `width` bits
/// clears, snapped to the lowest varying bit it contains: constant bits below the
/// top one stay out of the digit. Returns the digit and the bits it clears.
fn top_window(remaining: u128, width: usize) -> (Digit, u128) {
    let high = remaining.ilog2() as usize;
    let window = u128::MAX << (high + 1).saturating_sub(width);
    let low = (remaining & window).trailing_zeros() as usize;
    let digit = Digit {
        shift: low,
        width: high - low + 1,
    };
    (digit, window)
}

/// Minimum number of width-bounded windows; greedy coverage is optimal for count.
fn digit_passes(mut varying: u128, width: usize) -> usize {
    let mut passes = 0;
    while varying != 0 {
        let (_, window) = top_window(varying, width);
        varying &= !window;
        passes += 1;
    }
    passes
}

/// Covers varying positions with the fewest passes, then the fewest bucket slots.
///
/// Every legal final window extends an optimal lower-bit prefix. This bounded
/// search treats dense, shifted and sparse variation alike: constant gaps cost
/// table slots only when keeping them avoids another record pass.
fn plan_digits(varying: u128, width: usize, digits: &mut [Digit]) -> usize {
    if varying == 0 {
        return 0;
    }
    let (digit, window) = top_window(varying, width);
    if varying & !window == 0 {
        digits[0] = digit;
        return 1;
    }
    let mut positions = [0u8; u128::BITS as usize];
    let mut bits = varying;
    let len = bits.count_ones() as usize;
    for position in &mut positions[..len] {
        *position = bits.trailing_zeros() as u8;
        bits &= bits - 1;
    }
    let mut cost = [(0u8, 0u32); u128::BITS as usize + 1];
    let mut previous = [0u8; u128::BITS as usize + 1];
    for end in 1..=len {
        cost[end] = (u8::MAX, u32::MAX);
        for start in (0..end).rev() {
            let span = (positions[end - 1] - positions[start]) as usize + 1;
            if span > width {
                break;
            }
            let candidate = (cost[start].0 + 1, cost[start].1 + (1 << span));
            if candidate < cost[end] {
                cost[end] = candidate;
                previous[end] = start as u8;
            }
        }
    }
    let mut end = len;
    let mut passes = 0;
    while end != 0 {
        let start = previous[end] as usize;
        digits[passes] = Digit {
            shift: positions[start] as usize,
            width: (positions[end - 1] - positions[start]) as usize + 1,
        };
        passes += 1;
        end = start;
    }
    passes
}

/// Buffers shared by the nodes and tiles of one sort.
struct Scratch<T> {
    /// Tile records followed by cached bucket IDs beyond the cursor table.
    tile: Vec<MaybeUninit<T>>,
    /// Block buffers allocated when a node first needs them.
    block: block::Buffers<T>,
}

impl<T> Scratch<T> {
    /// Creates empty scratch without allocating.
    fn new() -> Self {
        Self {
            tile: Vec::new(),
            block: block::Buffers::new(),
        }
    }
}

/// Routes nodes and finishes tiles with one extractor and shared scratch.
///
/// `CACHED` stores each record's bucket ID during a tile pass's histogram so
/// the scatter never calls the key function again; it is `true` for user key
/// functions and `false` for library-encoded keys.
struct Sorter<'a, T, E, const CACHED: bool> {
    scratch: &'a mut Scratch<T>,
    extractor: &'a mut E,
    tile_bytes: usize,
}

/// Sorts `records` stably by the keys `key` extracts.
///
/// Allocates tile and block buffers only after the order probe and varying-bit
/// scan have not finished the sort.
pub(crate) fn sort<T, E: Extract<T>, const CACHED: bool>(records: &mut [T], extractor: &mut E) {
    sort_with_tile_bytes::<T, E, CACHED>(records, extractor, TILE_BYTES);
}

/// [`sort`] with a tile budget of `tile_bytes`, so tests can reach node routes
/// with small inputs.
fn sort_with_tile_bytes<T, E: Extract<T>, const CACHED: bool>(
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
    let Some(varying) = prepare(records, extractor, tile_bytes) else {
        return;
    };
    let mut scratch = Scratch::new();
    let scratch = &mut scratch;
    if E::Value::BITS > 64 {
        if varying >> 64 == 0 {
            Sorter::<T, _, CACHED> {
                scratch,
                extractor: &mut KeyHalf::<_, 0>(extractor),
                tile_bytes,
            }
            .sort_node(records, varying);
            return;
        }
        if varying & ((1u128 << 64) - 1) == 0 {
            Sorter::<T, _, CACHED> {
                scratch,
                extractor: &mut KeyHalf::<_, 64>(extractor),
                tile_bytes,
            }
            .sort_node(records, varying >> 64);
            return;
        }
    }
    Sorter::<T, E, CACHED> {
        scratch,
        extractor,
        tile_bytes,
    }
    .sort_node(records, varying);
}

/// Finishes ordered or insertion-friendly roots, otherwise returns their varying bits.
/// Shared by serial and parallel entries before either allocates partition scratch.
fn prepare<T, E: Extract<T>>(
    records: &mut [T],
    extractor: &mut E,
    tile_bytes: usize,
) -> Option<u128> {
    let record_bytes = core::mem::size_of::<T>();
    let sampled = probe_order(records, extractor);
    match sampled {
        Order::Ascending => return None,
        Order::Descending { strict } => {
            if strict {
                reverse_strictly(records);
            } else {
                reverse_stably(records, extractor);
            }
            return None;
        }
        Order::Unsorted => {
            // This input already fits the insertion leaf; a budget and varying-bit
            // scan cannot select a different finishing algorithm.
            if records.len() <= INSERTION_MAX
                && records.len().saturating_mul(record_bytes) <= tile_bytes
            {
                extractor.finish_insertion(records);
                return None;
            }
            let probe_len = (tile_bytes / record_bytes).max(2).min(records.len());
            if insertion_probe(records, extractor, probe_len) {
                return None;
            }
        }
        Order::Flat => {}
    }
    let varying = block::varying_key_bits(records, extractor);
    if varying == 0 {
        return None;
    }
    if sampled == Order::Flat {
        // A flat sample contains an equal adjacent pair, so confirmed descending
        // keys here are not strict and must keep the stable reversal.
        match confirm_order(records, extractor) {
            Order::Ascending => return None,
            Order::Descending { .. } => {
                reverse_stably(records, extractor);
                return None;
            }
            _ => {}
        }
    }
    Some(varying)
}

/// Tries to finish `records` with a budgeted insertion pass and returns whether it
/// did.
///
/// An input of at most `probe_len` records gets a budget of `probe_len` swaps.
/// A longer input first insertion-sorts the keys of its first `probe_len`
/// records in a local key buffer, and scales that window's swap count to the
/// whole input as the budget for the records.
/// A window that needs no swaps says nothing about the rest, so the records are
/// not touched. A pass that runs out of budget leaves `records` a stable
/// permutation of its input.
fn insertion_probe<T, E: Extract<T>>(
    records: &mut [T],
    extractor: &mut E,
    probe_len: usize,
) -> bool {
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

/// Target pair count used to choose the order gate's stride.
const ORDER_SAMPLES: usize = 4096;

/// Classifies adjacent keys directly at unit stride, otherwise samples pairs
/// before confirming candidate order with a full scan.
#[inline(never)]
fn probe_order<T, E: Extract<T>>(records: &[T], extractor: &mut E) -> Order {
    let stride = (records.len() / ORDER_SAMPLES).max(1);
    if stride == 1 {
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
            return if keys
                .try_fold(current, |before, after| (before <= after).then_some(after))
                .is_some()
            {
                Order::Ascending
            } else {
                Order::Unsorted
            };
        }
        return keys
            .try_fold((current, strict), |(before, strict), after| {
                if before > after {
                    Some((after, strict))
                } else if before == after {
                    Some((after, false))
                } else {
                    None
                }
            })
            .map_or(Order::Unsorted, |(_, strict)| Order::Descending { strict });
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

/// Confirms direction from the first unequal key, marking strict descent.
fn confirm_order<T, E: Extract<T>>(records: &[T], extractor: &mut E) -> Order {
    let first = extractor.key(&records[0]);
    let mut keys = records[1..].iter().map(|record| extractor.key(record));
    let Some(current) = keys.find(|&key| key != first) else {
        return Order::Ascending;
    };
    if first < current {
        // The iterator has already encoded the first unequal key.
        let tail = &records[records.len() - keys.len()..];
        if extractor.confirm_ascending(tail, current) {
            Order::Ascending
        } else {
            Order::Unsorted
        }
    } else {
        // Strict descent requires the first unequal key to be the second record.
        let strict = keys.len() == records.len() - 2;
        keys.try_fold((current, strict), |(before, strict), after| {
            if before > after {
                Some((after, strict))
            } else if before == after {
                Some((after, false))
            } else {
                None
            }
        })
        .map_or(Order::Unsorted, |(_, strict)| Order::Descending { strict })
    }
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

/// One stable counting pass: histogram, prefix sum, then scatter from `source`
/// to `destination`. Returns the size of the largest bucket.
///
/// When `CACHED`, the histogram stores each record's bucket and the scatter
/// reads it back, so a key function that changes its answer cannot send two
/// records to one slot. Otherwise the scatter recomputes the key.
///
/// # Safety
///
/// - `0 < len <= u16::MAX`.
/// - `source` holds `len` initialized records; `destination` has `len`
///   writable slots in a separate allocation.
/// - `cursor` has `MAX_DIGIT_BUCKETS` writable `u16` slots.
/// - When `CACHED` and the IDs do not fit after the cursor table, `cache_tail`
///   has `len` aligned `u16` slots disjoint from both record buffers.
/// - When not `CACHED`, `key` is a library encoder: deterministic and
///   panic-free.
/// - If `key` panics, `destination` holds a partial copy that must be neither
///   read nor dropped.
#[inline]
unsafe fn counting_pass<T, E: Extract<T>, const CACHED: bool>(
    source: *const T,
    destination: *mut T,
    len: usize,
    digit: Digit,
    cursor: *mut u16,
    cache_tail: *mut u16,
    extractor: &mut E,
) -> u16 {
    // SAFETY: the caller supplies initialized source records, disjoint writable
    // output and cursor/cache storage as specified by this function's contract.
    unsafe {
        let mask = digit.mask::<E::Value>();
        let shift = E::Value::from_usize(digit.shift);
        let buckets = 1usize << digit.width;
        let bucket_ids = if CACHED && len + buckets <= MAX_DIGIT_BUCKETS {
            cursor.add(buckets)
        } else {
            cache_tail
        };
        for slot in 0..buckets {
            // SAFETY: the caller guarantees room for `buckets` entries.
            *cursor.add(slot) = 0;
        }
        let mut largest = 0u16;
        for index in 0..len {
            // SAFETY: the caller guarantees `len` initialized records.
            let key = extractor.key(&*source.add(index));
            let bucket = ((key & mask) >> shift).to_usize();
            if CACHED {
                // SAFETY: the scratch tail has len aligned u16 slots.
                *bucket_ids.add(index) = bucket as u16;
            }
            // SAFETY: the digit mask keeps the index below `buckets`, and no bucket
            // can hold more than `len <= u16::MAX` records.
            let count = *cursor.add(bucket) + 1;
            *cursor.add(bucket) = count;
            largest = largest.max(count);
        }
        #[cfg(feature = "portable-simd")]
        {
            // SAFETY: the caller guarantees `buckets` writable cursor slots.
            let starts = core::slice::from_raw_parts_mut(cursor, buckets);
            crate::portable_simd::prefix_sum(starts);
        }
        #[cfg(not(feature = "portable-simd"))]
        {
            let mut offset = 0u16;
            for slot in 0..buckets {
                // SAFETY: as above.
                let count = *cursor.add(slot);
                *cursor.add(slot) = offset;
                offset += count;
            }
        }
        for index in 0..len {
            let record = source.add(index);
            let bucket = if !CACHED {
                // Only sealed library encoders use the uncached path; their
                // classification is deterministic and cannot panic.
                ((extractor.key(&*record) & mask) >> shift).to_usize()
            } else {
                // SAFETY: histogram initialized every cached classification before
                // any record was copied. Scatter runs no user code on this path.
                *bucket_ids.add(index) as usize
            };
            let slot = *cursor.add(bucket);
            *cursor.add(bucket) = slot.wrapping_add(1);
            let target = if CACHED {
                slot as usize
            } else {
                (slot as usize).min(len - 1)
            };
            // SAFETY: cached classifications or deterministic sealed encoders
            // give disjoint output intervals that initialize every output slot.
            core::ptr::write(destination.add(target), core::ptr::read(record));
        }
        largest
    }
}

/// Chooses the widest cursor table that fits beside the tile, capped by
/// record count to avoid clearing mostly empty buckets on wide records.
fn tile_digit_bits(len: usize, record_bytes: usize) -> usize {
    let tile_bytes = len.saturating_mul(record_bytes);
    // The widest table that still fits: `2 * 2^width` bytes of cursors.
    let widest = (tile_bytes / 2).max(1).ilog2() as usize;
    // Limit empty-bucket work to at most eight times the record count, or four
    // for a record wider than a machine word: the same table then holds half as
    // many records, and the buckets it clears are mostly empty. 16-byte records,
    // 10M random keys: 8.0% faster at four (15 of 15 rounds; the A/A pair
    // disagrees by 23.6% on that case, so the direction is the result, not the
    // size). 128-byte records are unchanged: the byte cap already decides.
    let slack = if record_bytes > 8 { 2 } else { 3 };
    let by_records = len.max(1).ilog2() as usize + slack;
    widest
        .min(by_records)
        .clamp(MIN_TILE_DIGIT_BITS, MAX_TILE_DIGIT_BITS)
}

/// Chooses an initial width targeting half-tile children, then widens if needed.
fn choose_split_digit<V: Unsigned>(
    len: usize,
    record_bytes: usize,
    top: usize,
    tile_bytes: usize,
    sample: &KeySample<V>,
) -> Digit {
    let tile_records = (tile_bytes / record_bytes).max(64);
    let target = (tile_records / 2).max(1);
    let children = (len / target).max(2);
    let bits = (children - 1).ilog2() + 1;
    // Wider fan-out increased page/TLB pressure in the 10M measurements.
    let mut width = (bits as usize).clamp(3, SPLIT_BITS).min(top).max(1);

    // Widen a concentrated top window until a quarter of its buckets are sampled.
    while width < top && width + 2 <= MAX_SPLIT_BITS {
        // At larger widths the sample cannot fill a quarter of the buckets.
        if (1usize << width) <= 4 * sample.len {
            let probe = Digit {
                shift: top - width,
                width,
            };

            if sample.distinct(probe) * 4 >= 1usize << width {
                break;
            }
        }
        // Clamp to the highest varying bit, or `top - width` underflows: a
        // 9-bit node would widen to 10 bits.
        width = (width + 2).min(top);
    }
    Digit {
        shift: top - width,
        width,
    }
}

/// Returns the two runs the top varying bit already holds, as the end of the
/// first run and whether it holds the bit.
///
/// A node is two runs when it reads `0*1*` or `1*0*` in that bit; the second
/// form is one rotation away from the first. Any other arrangement answers at
/// the first record that breaks the pattern, which unordered inputs reach
/// within a pair or two.
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

/// A node's partition decision, consumed by either execution path.
/// Sampling and ordered-run recognition belong here, not in the scheduler.
enum NodePlan<V> {
    Window(Digit),
    Equal(V),
    Runs { boundary: usize, set: bool },
    Digit(Digit),
}

/// Chooses a partition for a nonconstant node larger than its tile budget.
fn plan_node<T, E: Extract<T>>(
    records: &[T],
    varying: u128,
    tile_bytes: usize,
    extractor: &mut E,
) -> NodePlan<E::Value> {
    let low = varying.trailing_zeros() as usize;
    let span = bit_len(varying) - low;
    if span <= SPLIT_BITS {
        return NodePlan::Window(Digit {
            shift: low,
            width: span,
        });
    }
    let sample = (records.len().saturating_mul(core::mem::size_of::<T>()) >= 4 * tile_bytes)
        .then(|| KeySample::collect(records, extractor));
    if let Some(equal) = sample.as_ref().and_then(KeySample::mode) {
        return NodePlan::Equal(equal);
    }
    plan_digit(records, varying, tile_bytes, extractor, sample)
}

/// Consumes key bits even when an inconsistent extractor defeats an equality split.
fn plan_digit<T, E: Extract<T>>(
    records: &[T],
    varying: u128,
    tile_bytes: usize,
    extractor: &mut E,
    sample: Option<KeySample<E::Value>>,
) -> NodePlan<E::Value> {
    let top = bit_len(varying);
    if let Some((boundary, set)) = two_runs(records, 1u128 << (top - 1), extractor) {
        NodePlan::Runs { boundary, set }
    } else {
        let sample = sample.unwrap_or_else(|| KeySample::collect(records, extractor));
        NodePlan::Digit(choose_split_digit(
            records.len(),
            core::mem::size_of::<T>(),
            top,
            tile_bytes,
            &sample,
        ))
    }
}

impl<T, E: Extract<T>, const CACHED: bool> Sorter<'_, T, E, CACHED> {
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

    /// Finishes an ordered child before scanning its remaining candidate bits.
    fn sort_child(&mut self, records: &mut [T], candidate: u128) {
        if records.len() < 2 || candidate == 0 {
            return;
        }
        // A constant child finishes on the varying scan alone, as without this
        // route. An ordered child is confirmed next so it skips the partition
        // the digit would otherwise need.
        let varying = scan_varying_bits(records, candidate, self.extractor);
        if varying == 0 {
            return;
        }
        match confirm_order(records, self.extractor) {
            Order::Ascending => return,
            Order::Descending { strict: true } => {
                reverse_strictly(records);
                return;
            }
            Order::Descending { strict: false } => {
                reverse_stably(records, self.extractor);
                return;
            }
            Order::Flat | Order::Unsorted => {}
        }
        self.sort_node(records, varying);
    }

    /// Routes a slice whose order has already been probed.
    fn sort_node(&mut self, records: &mut [T], varying: u128) {
        let len = records.len();
        if len < 2 || varying == 0 {
            return;
        }
        let record_bytes = core::mem::size_of::<T>();
        let node_bytes = len.saturating_mul(record_bytes);
        if node_bytes <= self.tile_bytes {
            self.sort_tile(records, varying);
            return;
        }
        let plan = plan_node(records, varying, self.tile_bytes, self.extractor);
        self.execute_node(records, varying, plan);
    }

    /// Executes an already selected partition; parallel fallbacks do not resample.
    fn execute_node(&mut self, records: &mut [T], varying: u128, plan: NodePlan<E::Value>) {
        match plan {
            NodePlan::Window(digit) => {
                if !self
                    .extractor
                    .try_count_window(records, digit.shift, digit.width)
                {
                    block::partition_digit(
                        records,
                        digit.width,
                        digit.shift,
                        self.extractor,
                        &mut self.scratch.block,
                    );
                }
            }
            NodePlan::Equal(equal) => self.partition_equal(records, equal, varying),
            NodePlan::Runs { boundary, set } => self.split_ordered(records, boundary, set, varying),
            NodePlan::Digit(digit) => self.partition_digit(records, digit, varying),
        }
    }

    /// Partitions by digit, then sorts each unfinished child.
    fn partition_digit(&mut self, records: &mut [T], digit: Digit, varying: u128) {
        let record_bytes = core::mem::size_of::<T>();
        // Bucket boundaries come from the block engine. The split digit is
        // constant inside each child, so its bits are excluded from the next scan.
        let candidate = varying & !digit.mask::<E::Value>().to_u128();
        let tile_bytes = self.tile_bytes;
        let mut block_buffers = core::mem::take(&mut self.scratch.block);
        let mut visit = |child: &mut [T], extractor: &mut E| {
            if child.len().saturating_mul(record_bytes) <= tile_bytes {
                let varying = scan_varying_bits(child, candidate, extractor);
                Sorter::<T, E, CACHED> {
                    scratch: self.scratch,
                    extractor,
                    tile_bytes,
                }
                .sort_tile(child, varying);
            }
        };
        let ends = block::partition_digit_with_ends(
            records,
            digit.width,
            digit.shift,
            self.extractor,
            &mut block_buffers,
            &mut visit,
        );
        self.scratch.block = block_buffers;
        let mut start = 0;
        for &end in &ends {
            if end > start {
                let child = &mut records[start..end];
                if child.len().saturating_mul(record_bytes) > tile_bytes {
                    self.sort_child(child, candidate);
                }
            }
            start = end;
        }
        self.scratch.block.recycle_ends(ends);
    }

    /// Partitions below, equal to, and above a sampled key. The equal band is done.
    fn partition_equal(&mut self, records: &mut [T], equal: KeyValue<T, E>, parent_varying: u128) {
        let classify = move |extractor: &mut E, record: &T| {
            let key = extractor.key(record);
            usize::from(key >= equal) + usize::from(key > equal)
        };
        // Three outcomes need four buckets because `partition_by` takes powers
        // of two; the fourth is unused, so the greater band runs to the end.
        let ends = block::partition_by(
            records,
            4,
            self.extractor,
            &mut self.scratch.block,
            classify,
        );
        if ends[0] == records.len() || ends[1] == 0 {
            // A changing key can leave the whole node below or above the
            // sampled pivot. Force a digit partition to remove candidate bits
            // rather than repeat the same equality partition indefinitely.
            self.scratch.block.recycle_ends(ends);
            let plan = plan_digit(
                records,
                parent_varying,
                self.tile_bytes,
                self.extractor,
                None,
            );
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

    /// Reserves `len` scratch records and, when bucket IDs are cached and do not
    /// fit after `buckets` cursors, `len` aligned `u16` slots after them.
    ///
    /// Returns the scratch records and the ID slots, or a null ID pointer when
    /// the IDs fit in the cursor table.
    fn reserve_scratch(&mut self, len: usize, buckets: usize) -> (*mut T, *mut u16) {
        // Cached IDs first use the unused suffix of the cursor table. Larger
        // caches share the record allocation, without another allocation or
        // zero initialization.
        let tail = CACHED && len + buckets > MAX_DIGIT_BUCKETS;
        if tail {
            let bytes = core::mem::size_of::<T>();
            let extra = (len * core::mem::size_of::<u16>() + 1).div_ceil(bytes);
            let required = len
                .checked_add(extra)
                .expect("tile scratch capacity overflow");
            if self.scratch.tile.capacity() < required {
                self.scratch
                    .tile
                    .reserve_exact(required - self.scratch.tile.len());
            }
        }
        if self.scratch.tile.len() < len {
            self.scratch
                .tile
                .reserve_exact(len - self.scratch.tile.len());
            self.scratch.tile.resize_with(len, MaybeUninit::<T>::uninit);
        }
        let records = self.scratch.tile.as_mut_ptr().cast::<T>();
        let ids = if tail {
            // SAFETY: capacity includes len records followed by 2*len+1 bytes.
            // u16 alignment needs at most one byte; the regions do not overlap.
            unsafe {
                let start = records.add(len).cast::<u8>();
                start
                    .add(start.align_offset(core::mem::align_of::<u16>()))
                    .cast::<u16>()
            }
        } else {
            core::ptr::null_mut()
        };
        (records, ids)
    }

    /// Partitions a tile through scratch, then finishes each bucket.
    fn partition_tile(&mut self, records: &mut [T], digit: Digit, varying: u128) {
        let len = records.len();
        let (scratch, cache_tail) = self.reserve_scratch(len, 1 << digit.width);
        let mut counts = MaybeUninit::<[u16; MAX_DIGIT_BUCKETS]>::uninit();
        let counts_ptr = counts.as_mut_ptr().cast::<u16>();
        let tile = records.as_mut_ptr();
        let mut guard = RestoreTile {
            tile,
            len,
            source: tile,
        };
        let mut destination = scratch;
        // SAFETY: both buffers have len slots and the cursor/cache regions
        // satisfy pass requirements; uncached mode uses only sealed encoders.
        let largest = unsafe {
            guard.pass::<E, CACHED>(
                &mut destination,
                digit,
                counts_ptr,
                cache_tail,
                self.extractor,
            )
        };

        drop(guard);
        // With insertion-sized buckets, whole-tile insertion has the same inversion
        // count as separate bucket sorts and skips the boundary walk.
        if largest as usize <= INSERTION_MAX {
            self.extractor.finish_insertion(records);
            return;
        }
        // Read boundaries from the completed cursors; recomputing them from keys
        // measured 48% slower when there was roughly one bucket per record.
        let candidate = varying & !digit.mask::<E::Value>().to_u128();
        // SAFETY: a completed pass initialized exactly this cursor prefix.
        let ends = unsafe { core::slice::from_raw_parts(counts_ptr, 1usize << digit.width) };
        let mut start = 0;
        for &end in ends {
            let end = end as usize;
            debug_assert!(start <= end && end <= len);
            // Insertion-sized children need no variation scan.
            if end - start > INSERTION_MAX {
                let child = &records[start..end];
                let varying = scan_varying_bits(child, candidate, self.extractor);
                self.sort_tile(&mut records[start..end], varying);
            } else if end > start + 1 {
                self.extractor.finish_insertion(&mut records[start..end]);
            }
            start = end;
        }
    }

    /// Finishes a tile by insertion, an MSD partition, or LSD counting passes.
    fn sort_tile(&mut self, records: &mut [T], varying: u128) {
        let len = records.len();
        if len < 2 || varying == 0 {
            return;
        }
        if len <= INSERTION_MAX {
            self.extractor.finish_insertion(records);
            return;
        }
        // Widest counting table this tile can afford; LSD spreads it over passes.
        let table_width = tile_digit_bits(len, core::mem::size_of::<T>());
        let passes = digit_passes(varying, table_width);
        // Compare an MSD descent against the flat plan in passes over the tile.
        // A level plus its finish cannot undercut two flat passes, so shorter
        // plans skip the occupancy sample.
        if passes > MSD_FINISH_PASSES + 1 {
            // One partition finishes the tile, so it takes the widest table rather
            // than a pass-plan window; a narrower digit leaves more to insertion.
            let (digit, _) = top_window(varying, table_width);
            // The tile is non-empty, so its sample is non-empty too. Repeats in
            // the sample cap the split; a sample with no repeats at all is limited
            // by its length, not by the keys, so the digit's buckets apply.
            // `max(1)` is redundant in value but not in codegen: it tells the
            // optimizer the divisor below is non-zero, and removing it measured
            // as a slower `sort_tile` (an extra zero check and different register
            // allocation).
            let sample = KeySample::collect(records, self.extractor);
            let distinct = sample.distinct(digit).max(1);
            let fanout = if distinct >= sample.len {
                1usize << digit.width
            } else {
                distinct
            };
            // Estimate the descent until the largest child fits insertion; each
            // level is one counting pass and the finish is one more.
            let mut largest = len;
            let mut depth = 0;
            while largest > INSERTION_MAX && depth < passes {
                depth += 1;
                largest /= fanout;
            }
            // The tile holds more than INSERTION_MAX records, so the loop ran.
            // As above, `max(1)` is kept for the optimizer, not for the value.
            let level_cost = depth.max(1) + MSD_FINISH_PASSES;
            // A low-fanout sample can hide a concentrated bucket that becomes
            // equal or insertion-sized after one split.
            if distinct <= 2 || level_cost < passes {
                self.partition_tile(records, digit, varying);
                return;
            }
        }
        let mut digits = [Digit { shift: 0, width: 0 }; MAX_DIGITS];
        let passes = plan_digits(varying, table_width, &mut digits);
        let buckets = if CACHED {
            1 << digits[..passes]
                .iter()
                .map(|digit| digit.width)
                .max()
                .unwrap()
        } else {
            0
        };
        let (scratch_ptr, cache_tail) = self.reserve_scratch(len, buckets);
        debug_assert!(len <= MAX_TILE_RECORDS);
        let mut counts = MaybeUninit::<[u16; MAX_DIGIT_BUCKETS]>::uninit();
        let counts_ptr = counts.as_mut_ptr().cast::<u16>();
        let tile = records.as_mut_ptr();
        let mut guard = RestoreTile {
            tile,
            len,
            source: tile,
        };
        let mut destination = scratch_ptr;
        for &digit in digits[..passes].iter().rev() {
            // SAFETY: scratch and tile are disjoint len-slot buffers. Cached
            // IDs fit in the cursor suffix or the reserved scratch tail.
            unsafe {
                guard.pass::<E, CACHED>(
                    &mut destination,
                    digit,
                    counts_ptr,
                    cache_tail,
                    self.extractor,
                )
            };
        }
    }
}

/// Tracks which buffer holds the records of a tile between counting passes.
///
/// `source` points at the records written by the last completed pass, either
/// the tile itself or scratch. On drop, including during a key-function panic,
/// records in scratch are copied back so the tile again owns all of them.
struct RestoreTile<T> {
    tile: *mut T,
    len: usize,
    source: *mut T,
}

impl<T> RestoreTile<T> {
    /// Runs one counting pass from `source` into `*destination`, then swaps
    /// the two so `source` names the completed output.
    ///
    /// # Safety
    ///
    /// The requirements of [`counting_pass`], with `self.source` as the source
    /// and `*destination` as the destination.
    #[inline]
    unsafe fn pass<E: Extract<T>, const CACHED: bool>(
        &mut self,
        destination: &mut *mut T,
        digit: Digit,
        cursor: *mut u16,
        cache_tail: *mut u16,
        extractor: &mut E,
    ) -> u16 {
        // SAFETY: the caller supplies initialized source records, disjoint writable
        // output and cursor/cache storage as specified by this function's contract.
        unsafe {
            let largest = counting_pass::<T, E, CACHED>(
                self.source,
                *destination,
                self.len,
                digit,
                cursor,
                cache_tail,
                extractor,
            );
            core::mem::swap(&mut self.source, destination);
            largest
        }
    }
}

impl<T> Drop for RestoreTile<T> {
    fn drop(&mut self) {
        if self.source != self.tile {
            // SAFETY: source holds initialized records in scratch; tile holds
            // Copy values or stale non-Copy bytes. The allocations do not overlap.
            unsafe {
                core::ptr::copy_nonoverlapping(self.source, self.tile, self.len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::key::ByKey;
    use std::panic::{AssertUnwindSafe, catch_unwind};

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
            sort_with_tile_bytes::<_, _, true>(
                &mut probe,
                &mut ByKey(|x: &(usize, &mut usize)| {
                    total += 1;
                    key(x.0)
                }),
                128,
            );
        }
        let steps = if cfg!(miri) { 4 } else { 64 };
        for changing in [false, true] {
            if !changing && !cfg!(feature = "std") {
                continue;
            }
            for step in 0..steps {
                let at = 1 + step * total / steps;
                let mut values: Vec<_> = (0..len).collect();
                let mut records: Vec<_> = values.iter_mut().enumerate().collect();
                let mut calls = 0;
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    sort_with_tile_bytes::<_, _, true>(
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
    fn sampled_keys_cover_short_inputs_and_top_buckets() {
        for len in [1, 7, 8, 9, 63, 127, 128, 129, 1024] {
            let records: Vec<u64> = (0..len).map(|index| (index * 7919) as u64).collect();
            let sample = KeySample::collect(&records, &mut Direct);
            assert_eq!(sample.len, len.min(KEY_SAMPLE_LEN));
            if len <= KEY_SAMPLE_LEN {
                let mut expected = records.clone();
                expected.sort_unstable();
                assert_eq!(&sample.keys[..sample.len], expected);
            }
            for width in 1..=MAX_SPLIT_BITS {
                let digit = Digit {
                    shift: 24 - width,
                    width,
                };
                let mut buckets: Vec<_> = sample.keys[..sample.len]
                    .iter()
                    .map(|key| key & digit.mask::<u64>())
                    .collect();
                buckets.sort_unstable();
                buckets.dedup();
                assert_eq!(sample.distinct(digit), buckets.len());
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
    fn changing_keys_finish_tile() {
        let mut records: Vec<_> = (0..1000u64).collect();
        let mut scratch = Scratch::new();
        let mut calls = 0;
        // Every sample and histogram puts all records in one bucket. Each child
        // scan then reintroduces every key bit, including the partitioned digit.
        let constant_calls = KEY_SAMPLE_LEN + records.len();
        let mut extractor = ByKey(|_: &u64| {
            assert!(calls < 12_000, "tile recursion did not make progress");
            let phase = calls % (constant_calls + 65);
            calls += 1;
            if phase < constant_calls || phase % 2 == 0 {
                0
            } else {
                u64::MAX
            }
        });
        Sorter::<_, _, true> {
            scratch: &mut scratch,
            extractor: &mut extractor,
            tile_bytes: TILE_BYTES,
        }
        .sort_tile(&mut records, u64::MAX as u128);
        records.sort_unstable();
        assert_eq!(records, (0..1000u64).collect::<Vec<_>>());
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
                let phase = calls % (len + 4 + 65 + KEY_SAMPLE_LEN);
                calls += 1;
                if phase < len {
                    1
                } else if phase < len + 4 {
                    // Reject order before the candidate scan and mode sample.
                    [1, 0, 0, 1][phase - len]
                } else if phase < len + 4 + 65 {
                    if phase % 2 == 0 { 0 } else { 0x1ff }
                } else if greater {
                    0
                } else {
                    0x1ff
                }
            });
            Sorter::<_, _, true> {
                scratch: &mut scratch,
                extractor: &mut extractor,
                tile_bytes: 32,
            }
            .partition_equal(&mut records, if greater { 0 } else { 0x1ff }, 0x1ff);
            records.sort_unstable();
            assert_eq!(records, (0..len as u16).collect::<Vec<_>>());
        }
    }

    #[test]
    fn cache_tail_after_odd_records() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        struct Odd([u8; 3]);
        let mut records: Vec<_> = (0..65u16)
            .map(|id| {
                let bits = (id * 251).to_le_bytes();
                Odd([bits[0], bits[1], id as u8])
            })
            .collect();
        let mut expected = records.clone();
        expected.sort_by_key(|x| u16::from_le_bytes([x.0[0], x.0[1]]));
        let mut scratch = Scratch::new();
        let mut key = ByKey(|x: &Odd| u16::from_le_bytes([x.0[0], x.0[1]]));
        Sorter::<_, _, true> {
            scratch: &mut scratch,
            extractor: &mut key,
            tile_bytes: TILE_BYTES,
        }
        .partition_tile(
            &mut records,
            Digit {
                shift: 0,
                width: 14,
            },
            0x3fff,
        );
        assert_eq!(records, expected);
    }

    /// Exhaustively enumerates window partitions independently of the planner.
    fn optimal_digit_cost(bits: u128, width: usize) -> (usize, usize) {
        if bits == 0 {
            return (0, 0);
        }
        let high = bit_len(bits);
        (1..=width.min(high))
            .map(|span| {
                let remaining = bits & !(u128::MAX << (high - span));
                let (passes, slots) = optimal_digit_cost(remaining, width);
                (passes + 1, slots + (1 << span))
            })
            .min()
            .unwrap()
    }

    #[test]
    fn digit_plan_minimizes_passes_then_tables() {
        for bits in 1..256u128 {
            for width in 1..=5 {
                let mut digits = [Digit { shift: 0, width: 0 }; 128];
                let len = plan_digits(bits, width, &mut digits);
                let cost = (
                    len,
                    digits[..len].iter().map(|digit| 1 << digit.width).sum(),
                );
                assert_eq!(cost, optimal_digit_cost(bits, width));
                assert_eq!(len, digit_passes(bits, width));
            }
        }
        for shift in [0, 17, 96] {
            let mut digits = [Digit { shift: 0, width: 0 }; MAX_DIGITS];
            let len = plan_digits(0xffff_ffffu128 << shift, 14, &mut digits);
            assert_eq!(len, 3);
            assert_eq!(
                digits[..len]
                    .iter()
                    .map(|digit| 1usize << digit.width)
                    .sum::<usize>(),
                5120
            );
        }
    }

    #[test]
    fn digit_plan_covers_varying_bits() {
        for width in MIN_TILE_DIGIT_BITS..=MAX_TILE_DIGIT_BITS {
            for varying in [
                1,
                1 << 127,
                (1 << 127) | 1,
                u128::MAX,
                0x01010101010101010101010101010101,
            ] {
                let mut digits = [Digit { shift: 0, width: 0 }; MAX_DIGITS];
                let len = plan_digits(varying, width, &mut digits);
                let mut mask = 0;
                for digit in &digits[..len] {
                    let bits = digit.mask::<u128>();
                    assert_eq!(mask & bits, 0);
                    mask |= bits;
                }
                assert_eq!(mask & varying, varying);
            }
        }
    }
}
