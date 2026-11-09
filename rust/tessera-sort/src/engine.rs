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
/// Keys an occupancy sample reads to judge whether a digit divides its range.
const DIGIT_SAMPLE_LEN: usize = 128;

/// Counts distinct digits in an evenly spaced sample.
fn sample_distinct_digits<T, E: Extract<T>>(
    records: &[T],
    digit: Digit,
    extractor: &mut E,
) -> usize {
    let mask = digit.mask::<E::Value>();
    let shift = E::Value::from_usize(digit.shift);

    let mut seen = [0u64; MAX_DIGIT_BUCKETS / 64];
    let sample_len = DIGIT_SAMPLE_LEN.min(records.len());
    let step = (records.len() / sample_len).max(1);
    let mut distinct = 0;
    for index in (0..records.len()).step_by(step).take(sample_len) {
        let bucket = ((extractor.key(&records[index]) & mask) >> shift).to_usize();
        let word = bucket / 64;
        let bit = 1u64 << (bucket % 64);
        if seen[word] & bit == 0 {
            seen[word] |= bit;
            distinct += 1;
        }
    }
    distinct
}

/// Keys a node samples to look for one value that a large share of it repeats.
const MODE_SAMPLE_LEN: usize = 64;

/// A value repeated by at least a quarter of the sample gets an equality partition.
const MODE_SHARE_DENOMINATOR: usize = 4;

/// Consecutive keys per sampled run.
const MODE_SAMPLE_RUN_LEN: usize = 8;

/// Fills `keys` with runs of `MODE_SAMPLE_RUN_LEN` consecutive keys spread over
/// the node, which avoids stride aliasing with periodic keys.
fn sample_keys<T, E: Extract<T>>(records: &[T], keys: &mut [KeyValue<T, E>], extractor: &mut E) {
    let runs = keys.len() / MODE_SAMPLE_RUN_LEN;
    let step = (records.len() / runs).max(1);
    let mut len = 0;
    for run in 0..runs {
        let start = run * step;
        for offset in 0..MODE_SAMPLE_RUN_LEN {
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
}

/// Finds a key repeated by at least `1 / MODE_SHARE_DENOMINATOR` of the sample.
/// Leaves the sample sorted; later uses may only depend on the multiset.
fn sample_mode<V: Unsigned>(keys: &mut [V]) -> Option<V> {
    let len = keys.len();
    keys.sort_unstable();
    let mut best = &keys[..1];
    for run in keys.chunk_by(|a, b| a == b) {
        if run.len() > best.len() {
            best = run;
        }
    }
    (best.len() * MODE_SHARE_DENOMINATOR >= len).then_some(best[0])
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

/// Capacity of a digit plan. A plan over a 128-bit key needs at most 22 windows
/// of at least `MIN_TILE_DIGIT_BITS` bits; the array keeps 48 entries until a
/// smaller one is measured.
const MAX_DIGITS: usize = 48;

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

/// Packs varying bits into windows, highest first; the LSD passes consume the
/// plan in reverse and each window skips the constant bits below it.
///
/// A window spans at most `width` bits. When every bit below the top one varies,
/// the run is instead tiled evenly over the passes that `width` would need, so
/// 32 bits in three passes become 11 + 11 + 10 rather than 14 + 14 + 4. Both
/// plans move each record three times and cover the same bits, but the even one
/// clears, counts and walks 5,120 bucket slots instead of 32,784. Gaps keep the
/// wide windows, which jump over them.
fn plan_digits(varying: u128, width: usize, digits: &mut [Digit]) -> usize {
    let top = bit_len(varying);
    // Passes left to tile the contiguous run over; `None` keeps fixed windows.
    let mut even = (varying.count_ones() as usize == top).then_some(top.div_ceil(width));
    let mut remaining = varying;
    let mut len = 0;
    while remaining != 0 {
        let window_width = match even {
            Some(left) if left > 0 => bit_len(remaining).div_ceil(left),
            // Gaps keep windows of the full budget that jump over them.
            _ => width,
        };
        let (digit, window) = top_window(remaining, window_width);
        debug_assert!(len < digits.len());
        debug_assert!(digit.width <= width);
        digits[len] = digit;
        len += 1;
        remaining &= !window;
        even = even.map(|left| left.saturating_sub(1));
    }
    len
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
    let sampled = probe_order(records, extractor);
    match sampled {
        Order::Ascending => return,
        Order::Descending { strict } => {
            if strict {
                reverse_strictly(records);
            } else {
                reverse_stably(records, extractor);
            }
            return;
        }
        Order::Unsorted => {
            // This input already fits the insertion leaf; a budget and varying-bit
            // scan cannot select a different finishing algorithm.
            if records.len() <= INSERTION_MAX
                && records.len().saturating_mul(record_bytes) <= tile_bytes
            {
                extractor.finish_insertion(records);
                return;
            }
            let probe_len = (tile_bytes / record_bytes).max(2).min(records.len());
            if insertion_probe(records, extractor, probe_len) {
                return;
            }
        }
        Order::Flat => {}
    }
    let varying = block::varying_key_bits(records, extractor);
    if varying == 0 {
        return;
    }
    if sampled == Order::Flat {
        // A flat sample contains an equal adjacent pair, so confirmed descending
        // keys here are not strict and must keep the stable reversal.
        match confirm_order(records, extractor) {
            Order::Ascending => return,
            Order::Descending { .. } => {
                reverse_stably(records, extractor);
                return;
            }
            _ => {}
        }
    }
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
fn choose_split_digit<T, E: Extract<T>>(
    records: &[T],
    top: usize,
    tile_bytes: usize,
    extractor: &mut E,
) -> Digit {
    let record_bytes = core::mem::size_of::<T>();
    let tile_records = (tile_bytes / record_bytes).max(64);
    let target = (tile_records / 2).max(1);
    let children = (records.len() / target).max(2);
    let bits = (children - 1).ilog2() + 1;
    // Wider fan-out increased page/TLB pressure in the 10M measurements.
    let mut width = (bits as usize).clamp(3, SPLIT_BITS).min(top).max(1);

    // Widen a concentrated top window until a quarter of its buckets are sampled.
    while width < top && width + 2 <= MAX_SPLIT_BITS {
        // At larger widths the sample cannot fill a quarter of the buckets.
        if (1usize << width) <= 4 * DIGIT_SAMPLE_LEN {
            let probe = Digit {
                shift: top - width,
                width,
            };

            if sample_distinct_digits(records, probe, extractor) * 4 >= 1usize << width {
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

impl<T, E: Extract<T>, const CACHED: bool> Sorter<'_, T, E, CACHED> {
    fn sort_node(&mut self, records: &mut [T], varying: u128) {
        let len = records.len();
        if len < 2 || varying == 0 {
            return;
        }
        let record_bytes = core::mem::size_of::<T>();
        let node_bytes = len.saturating_mul(record_bytes);
        // Partitions can expose ordered children inside an unordered input.
        // Reuse the root gate instead of recognizing particular end runs.
        match probe_order(records, self.extractor) {
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
        if node_bytes <= self.tile_bytes {
            self.sort_tile(records, varying);
            return;
        }
        // One digit finishes a narrow span; wider spans benefit from tiled children.
        let low = varying.trailing_zeros() as usize;
        let top = bit_len(varying);
        let span = top - low;

        if span <= SPLIT_BITS {
            // A record of its own key is interchangeable with every record of
            // the same key, so counting the window and filling each run can
            // finish the node without moving blocks. A user key function keeps
            // its payloads and takes the partition below instead.
            if self.extractor.try_count_window(records, low, span) {
                return;
            }
            // One block partition finishes the node without recursion.
            block::partition_digit(
                records,
                SPLIT_BITS,
                low,
                self.extractor,
                &mut self.scratch.block,
            );
            return;
        }
        // Amortize the key sample over at least four tiles.
        if node_bytes >= 4 * self.tile_bytes {
            let mut keys = [KeyValue::<T, E>::from_usize(0); MODE_SAMPLE_LEN];
            sample_keys(records, &mut keys, self.extractor);
            // An equality partition finishes the repeated key immediately.
            if let Some(equal) = sample_mode(&mut keys) {
                self.partition_equal(records, equal, varying);
                return;
            }
        }
        let digit = choose_split_digit(records, top, self.tile_bytes, self.extractor);
        self.partition_digit(records, digit, varying);
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
                    let varying = scan_varying_bits(child, candidate, self.extractor);
                    self.sort_node(child, varying);
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
            let digit = choose_split_digit(
                records,
                bit_len(parent_varying),
                self.tile_bytes,
                self.extractor,
            );
            self.partition_digit(records, digit, parent_varying);
            return;
        }
        for range in [0..ends[0], ends[1]..records.len()] {
            let child = &mut records[range];
            if child.len() > 1 {
                let varying = scan_varying_bits(child, parent_varying, self.extractor);
                self.sort_node(child, varying);
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
        let mut digits = [Digit { shift: 0, width: 0 }; MAX_DIGITS];
        let passes = plan_digits(varying, table_width, &mut digits);
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
            let distinct = sample_distinct_digits(records, digit, self.extractor).max(1);
            let fanout = if distinct >= DIGIT_SAMPLE_LEN.min(len) {
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
    fn short_unordered_finishes_directly() {
        for len in [INSERTION_MAX - 1, INSERTION_MAX] {
            let mut records: Vec<_> = (0..len).map(|id| ((id * 17 % len) as u64, id)).collect();
            let mut expected = records.clone();
            let mut expected_visits = Vec::new();
            let mut extractor = ByKey(|record: &(u64, usize)| {
                expected_visits.push(record.1);
                record.0
            });
            assert!(probe_order(&expected, &mut extractor) == Order::Unsorted);
            extractor.finish_insertion(&mut expected);

            let mut visits = Vec::new();
            sort::<_, _, true>(
                &mut records,
                &mut ByKey(|record: &(u64, usize)| {
                    visits.push(record.1);
                    record.0
                }),
            );
            assert_eq!(records, expected);
            assert_eq!(visits, expected_visits);
        }
    }

    #[test]
    fn short_root_respects_tile() {
        /// Records which insertion stage the root selected.
        #[derive(Default)]
        struct InsertionStages {
            probes: usize,
            finishes: usize,
        }

        impl<const WORDS: usize> Extract<[u64; WORDS]> for InsertionStages {
            type Value = u64;

            fn key(&mut self, record: &[u64; WORDS]) -> u64 {
                record[0]
            }

            fn insertion_sort(
                &mut self,
                records: &mut [[u64; WORDS]],
                budget: usize,
            ) -> Option<usize> {
                self.probes += 1;
                insertion_loop(records, budget, self)
            }

            fn finish_insertion(&mut self, records: &mut [[u64; WORDS]]) {
                self.finishes += 1;
                finish_insertion(records, self);
            }
        }

        /// Checks the root choice with one inversion that fits either budget.
        fn check<const WORDS: usize>(tile_bytes: usize, direct: bool) {
            let mut records: Vec<_> = (0..INSERTION_MAX)
                .map(|id| {
                    let mut record = [0u64; WORDS];
                    record[0] = id as u64;
                    record
                })
                .collect();
            let expected = records.clone();
            records.swap(0, 1);
            let mut stages = InsertionStages::default();
            sort_with_tile_bytes::<_, _, true>(&mut records, &mut stages, tile_bytes);
            assert_eq!(records, expected);
            assert_eq!(stages.finishes, usize::from(direct));
            assert_eq!(stages.probes, usize::from(!direct));
        }

        let fit_bytes = INSERTION_MAX * core::mem::size_of::<u64>();
        check::<1>(fit_bytes, true);
        check::<1>(fit_bytes - 1, false);
        check::<257>(TILE_BYTES, false);
    }

    #[test]
    fn unit_stride_visits_once() {
        let lengths: &[usize] = if cfg!(miri) {
            &[2, 3, 4, 49, 129]
        } else {
            &[2, 3, 4, 49, 129, 1000, 8191]
        };
        for &len in lengths {
            for descending in [false, true] {
                for ties in [false, true] {
                    let mut records: Vec<_> = (0..len)
                        .map(|index| (if ties { index / 2 } else { index }, index))
                        .collect();
                    if descending {
                        records.reverse();
                    }
                    let mut visited = Vec::new();
                    let order = probe_order(
                        &records,
                        &mut ByKey(|record: &(usize, usize)| {
                            visited.push(record.1);
                            record.0
                        }),
                    );
                    assert!(if descending && (len > 2 || !ties) {
                        matches!(order, Order::Descending { strict } if strict != ties)
                    } else {
                        matches!(order, Order::Ascending)
                    });
                    assert_eq!(
                        visited,
                        records.iter().map(|record| record.1).collect::<Vec<_>>()
                    );
                }
            }
            let mut calls = 0;
            let order = probe_order(
                &std::vec![7u64; len],
                &mut ByKey(|record: &u64| {
                    calls += 1;
                    *record
                }),
            );
            assert!(matches!(order, Order::Ascending));
            assert_eq!(calls, len);
        }
    }

    #[test]
    fn unit_stride_rejects_directly() {
        /// Records visits and rejects dispatch to the separate ascending scan.
        #[derive(Default)]
        struct AdjacentProbe {
            visits: Vec<usize>,
        }

        impl Extract<(u64, usize)> for AdjacentProbe {
            type Value = u64;

            fn key(&mut self, record: &(u64, usize)) -> u64 {
                self.visits.push(record.1);
                record.0
            }

            fn confirm_ascending(&mut self, _: &[(u64, usize)], _: u64) -> bool {
                panic!("unit stride dispatched a second scan");
            }
        }

        for len in [47, 48, 49, 1000] {
            for prefix in [
                &[1, 2, 1][..],
                &[2, 1, 2],
                &[1, 1, 2, 2, 1],
                &[2, 2, 1, 1, 2],
                &[1, 1, 1, 2, 1],
                &[2, 2, 2, 1, 2],
            ] {
                let records: Vec<_> = (0..len)
                    .map(|id| (prefix[id.min(prefix.len() - 1)], id))
                    .collect();
                let mut extractor = AdjacentProbe::default();
                assert!(probe_order(&records, &mut extractor) == Order::Unsorted);
                assert_eq!(extractor.visits, (0..prefix.len()).collect::<Vec<_>>());
            }
        }
    }

    #[test]
    fn unit_stride_rejects_inversions() {
        for len in [3, 49, 129] {
            for descending in [false, true] {
                let mut records: Vec<_> = (0..len).collect();
                if descending {
                    records.reverse();
                }
                for index in 0..len - 1 {
                    records.swap(index, index + 1);
                    assert!(probe_order(&records, &mut Direct) == Order::Unsorted);
                    records.swap(index, index + 1);
                }
            }
        }
    }

    #[test]
    fn confirm_order_calls_key_in_order() {
        for descending in [false, true] {
            let mut records: Vec<_> = (0..129u64).map(|id| (id / 4, id)).collect();
            if descending {
                records.reverse();
            }
            let mut visited = Vec::new();
            let order = confirm_order(
                &records,
                &mut ByKey(|record: &(u64, u64)| {
                    visited.push(record.1);
                    record.0
                }),
            );
            // Four records share a key, so descending order is never strict.
            assert!(
                order
                    == if descending {
                        Order::Descending { strict: false }
                    } else {
                        Order::Ascending
                    }
            );
            assert_eq!(
                visited,
                records.iter().map(|record| record.1).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn strict_reverse_skips_rescan() {
        let mut records: Vec<_> = (0..257u64).rev().map(|key| (key, key)).collect();
        let mut expected = records.clone();
        expected.reverse();

        let mut probe_calls = 0;
        assert!(
            probe_order(
                &records,
                &mut ByKey(|record: &(u64, u64)| {
                    probe_calls += 1;
                    record.0
                }),
            ) == Order::Descending { strict: true }
        );

        let mut calls = 0;
        sort_with_tile_bytes::<_, _, true>(
            &mut records,
            &mut ByKey(|record: &(u64, u64)| {
                calls += 1;
                record.0
            }),
            TILE_BYTES,
        );
        assert_eq!(calls, probe_calls);
        assert_eq!(records, expected);
    }

    #[test]
    fn reverse_keeps_equal_order() {
        let mut records: Vec<_> = (0..257u64).map(|id| (id / 4, id)).collect();
        records.reverse();
        let mut expected = records.clone();
        expected.sort_by_key(|record| record.0);

        sort_with_tile_bytes::<_, _, true>(
            &mut records,
            &mut ByKey(|record: &(u64, u64)| record.0),
            TILE_BYTES,
        );
        assert_eq!(records, expected);
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
        let constant_calls = DIGIT_SAMPLE_LEN + records.len();
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
    fn insertion_budget_keeps_floor() {
        struct Budget {
            actual: Option<usize>,
        }
        impl Extract<u32> for Budget {
            type Value = u32;

            fn key(&mut self, record: &u32) -> u32 {
                *record
            }

            fn insertion_sort(&mut self, _records: &mut [u32], budget: usize) -> Option<usize> {
                self.actual = Some(budget);
                Some(0)
            }
        }
        for (len, probe_len, pairs, expected) in [
            (3, 3, 0, Some(3)),
            (9, 4, 0, None),
            (9, 4, 1, Some(2)),
            (9, 4, 2, Some(4)),
            (10, 4, 2, Some(5)),
        ] {
            let mut records: Vec<_> = (0..len as u32).collect();
            for pair in 0..pairs {
                records.swap(2 * pair, 2 * pair + 1);
            }
            let mut extractor = Budget { actual: None };
            assert_eq!(
                insertion_probe(&mut records, &mut extractor, probe_len),
                expected.is_some(),
            );
            assert_eq!(extractor.actual, expected);
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
                let phase = calls % (len + 65 + 4 + MODE_SAMPLE_LEN);
                calls += 1;
                if phase < len {
                    1
                } else if phase < len + 65 {
                    if phase % 2 == 0 { 0 } else { 0x1ff }
                } else if phase < len + 65 + 4 {
                    // Reject the node order gate before the next mode sample.
                    [1, 0, 0, 1][phase - len - 65]
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

    #[test]
    fn narrow_window_counting() {
        let len = if cfg!(miri) { 129 } else { 8193 };
        // Windows at the bottom, inside and at the top of a 64-bit key, plus one
        // window narrower than eight bits. A 256-byte tile keeps every case a
        // node, which is where the fill stands in for the block partition.
        for &(low, width) in &[(0usize, 8usize), (0, 3), (9, 8), (57, 7)] {
            let value_mask = (1u64 << width) - 1;
            let base = 0x2b93_a3f7_1d05_0000u64 & !(value_mask << low);
            let mut values: Vec<u64> = (0..len)
                .map(|id| {
                    base | (((id as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) & value_mask) << low)
                })
                .collect();
            let mut expected = values.clone();
            expected.sort();
            sort_with_tile_bytes::<u64, _, true>(&mut values, &mut Direct, 256);
            assert_eq!(values, expected, "direct low={low} width={width}");
            // A user key function keeps its payloads, so the same shape takes the
            // block partition and must reach the same order.
            let mut values: Vec<u64> = (0..len)
                .map(|id| {
                    base | (((id as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) & value_mask) << low)
                })
                .collect();
            sort_with_tile_bytes::<u64, _, true>(
                &mut values,
                &mut ByKey(|record: &u64| *record),
                256,
            );
            assert_eq!(values, expected, "by key low={low} width={width}");
        }
        // A 128-bit key whose variation fits in one half is sorted by that half,
        // so the fill is reached through `KeyHalf` and has to move the window.
        for &(low, width) in &[(0usize, 8usize), (64, 8), (120, 8)] {
            let value_mask = (1u128 << width) - 1;
            let base = 0x2b93_a3f7_1d05_0000u128 & !(value_mask << low);
            let mut values: Vec<u128> = (0..len)
                .map(|id| {
                    let random =
                        (id as u128).wrapping_mul(0x9e37_79b9_7f4a_7c15_1b2c_3d4e_5f60_7181);
                    base | ((random & value_mask) << low)
                })
                .collect();
            let mut expected = values.clone();
            expected.sort();
            sort_with_tile_bytes::<u128, _, true>(&mut values, &mut Direct, 256);
            assert_eq!(values, expected, "u128 low={low} width={width}");
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

    #[test]
    fn ordered_nodes_skip_scratch() {
        for descending in [false, true] {
            let mut records: Vec<_> = (0..129u64)
                .map(|id| (if descending { (128 - id) / 4 } else { id / 4 }, id))
                .collect();
            let mut expected = records.clone();
            expected.sort_by_key(|record| record.0);
            let mut scratch = Scratch::new();
            Sorter::<_, _, true> {
                scratch: &mut scratch,
                extractor: &mut ByKey(|record: &(u64, u64)| record.0),
                tile_bytes: 128,
            }
            .sort_node(&mut records, u64::MAX as u128);
            assert_eq!(records, expected);
            assert!(scratch.tile.is_empty());
        }
    }

    #[test]
    fn end_runs_sort_stably() {
        let len = if cfg!(miri) { 129 } else { 8193 };
        for high in [false, true] {
            let mut values: Vec<_> = (0..len)
                .map(|id| {
                    let key = if id < 2 {
                        if high {
                            u64::MAX - id as u64
                        } else {
                            id as u64
                        }
                    } else {
                        1u64 << 63 | (id as u64).wrapping_mul(31)
                    };
                    (key, id)
                })
                .collect();
            for i in (3..len - 1).step_by(13) {
                values.swap(i, i + 1);
            }
            let mut expected = values.clone();
            expected.sort_by_key(|x| x.0);
            sort_with_tile_bytes::<_, _, true>(
                &mut values,
                &mut ByKey(|x: &(u64, usize)| x.0),
                256,
            );
            assert_eq!(values, expected);
        }
    }
}
