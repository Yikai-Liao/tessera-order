// SPDX-License-Identifier: BSD-2-Clause

//! Shared strategy decisions from key variation, bounded samples and work estimates.
//! The executor receives a complete decision, not sample layout or cost-model steps.

use super::tile::{Cursor, MAX_DIGIT_BUCKETS};
use crate::block;
use crate::key::{Extract, Unsigned};
use crate::thresholds::INSERTION_MAX;

/// Initial split width; also the block engine's tuned width.
const SPLIT_BITS: usize = 8;

/// Split fan-out is bounded by the block engine's widest digit.
const MAX_SPLIT_BITS: usize = block::MAX_RADIX_BITS;

/// Widest digit supported by the tile cursor representation and budget.
const MAX_TILE_DIGIT_BITS: usize = MAX_DIGIT_BUCKETS.ilog2() as usize;
/// Narrowest digit a leaf may use.
const MIN_TILE_DIGIT_BITS: usize = 6;
/// Bounded sample shared by equality and digit-occupancy decisions.
pub(super) const KEY_SAMPLE_LEN: usize = 128;
/// A value occupying this fraction of the sample merits an equality partition.
const MODE_SHARE_DENOMINATOR: usize = 4;

/// Sorted sampled keys, independent of the digit currently being considered.
/// This is advisory evidence only: execution still scans actual child variation.
struct KeySample<V> {
    keys: [V; KEY_SAMPLE_LEN],
    len: usize,
}

impl<V: Unsigned> KeySample<V> {
    /// Reads one key from each equal stratum of a nonempty input, then sorts them.
    ///
    /// A golden-ratio offset within each stratum avoids aliasing with periodic
    /// input, and locally ordered keys still yield independent observations.
    fn collect<T, E: Extract<T, Value = V>>(records: &[T], extractor: &mut E) -> Self {
        let len = records.len().min(KEY_SAMPLE_LEN);
        let mut sample = Self {
            keys: [V::from_usize(0); KEY_SAMPLE_LEN],
            len,
        };
        let stride = records.len() / len;
        for (stratum, key) in sample.keys[..len].iter_mut().enumerate() {
            // A fixed-point fraction times `stride` stays below `stride`.
            let fraction = (stratum as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let offset = ((u128::from(fraction) * stride as u128) >> 64) as usize;
            *key = extractor.key(&records[stratum * stride + offset]);
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
    fn top_digit_occupancy(&self, digit: Digit) -> usize {
        let mask = digit.mask::<V>();
        self.keys[..self.len]
            .chunk_by(|a, b| (*a & mask) == (*b & mask))
            .count()
    }
}

/// Number of bits needed to represent `value`; 0 for 0.
#[inline]
pub(super) fn bit_len(value: u128) -> usize {
    (u128::BITS - value.leading_zeros()) as usize
}

/// One pass worth of key bits: `width` bits starting at `shift`.
#[derive(Clone, Copy)]
pub(super) struct Digit {
    pub(super) shift: usize,
    pub(super) width: usize,
}

impl Digit {
    #[inline]
    pub(super) fn mask<V: Unsigned>(self) -> V {
        (V::from_usize((1usize << self.width) - 1)) << V::from_usize(self.shift)
    }
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

/// Finishes in one or two flat passes, otherwise partitions the highest digit.
/// Two-pass plans minimize table slots by checking at most `width` split points;
/// longer plans inspect actual children instead of predicting their work.
fn plan_digits(varying: u128, width: usize) -> TilePlan {
    let (high, window) = top_window(varying, width);
    let remaining = varying & !window;
    if remaining == 0 {
        return TilePlan::Lsd {
            digits: [high; 2],
            passes: 1,
        };
    }
    let (low, _) = top_window(remaining, 128);
    if low.width > width {
        return TilePlan::Msd(high);
    }
    let mut digits = [high, low];
    let mut slots = (1 << high.width) + (1 << low.width);
    for high_width in 1..high.width {
        let (high, window) = top_window(varying, high_width);
        let (low, _) = top_window(varying & !window, 128);
        if low.width <= width {
            let cost = (1 << high.width) + (1 << low.width);
            if cost < slots {
                digits = [high, low];
                slots = cost;
            }
        }
    }
    TilePlan::Lsd { digits, passes: 2 }
}

/// Chooses the widest cursor table that fits beside the tile, capped by
/// record count to avoid clearing mostly empty buckets on wide records.
fn tile_digit_bits(len: usize, record_bytes: usize) -> usize {
    let tile_bytes = len.saturating_mul(record_bytes);
    // The widest table that still fits beside the records.
    let widest = (tile_bytes / core::mem::size_of::<Cursor>()).max(1).ilog2() as usize;
    // Limit empty-bucket work to at most four buckets per record. For records
    // of at most eight bytes the cursor-byte cap already implies this bound;
    // wider records need it. 16-byte records, 10M random keys: 8.0% faster at
    // four than at eight (15 of 15 rounds; the A/A pair disagrees by 23.6% on
    // that case, so the direction is the result, not the size).
    let by_records = len.max(1).ilog2() as usize + 2;
    widest
        .min(by_records)
        .clamp(MIN_TILE_DIGIT_BITS, MAX_TILE_DIGIT_BITS)
}

/// Chooses an initial width targeting half-tile children, then widens if needed.
fn choose_split_digit<V: Unsigned>(
    len: usize,
    top: usize,
    tile_len: usize,
    sample: &KeySample<V>,
) -> Digit {
    let tile_records = tile_len.max(64);
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

            if sample.top_digit_occupancy(probe) * 4 >= 1usize << width {
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
pub(super) enum NodePlan<V> {
    Equal(V),
    Runs { boundary: usize, set: bool },
    Digit(Digit),
}

/// Chooses a partition for a nonconstant node larger than its tile budget.
pub(super) fn plan_node<T, E: Extract<T>>(
    records: &[T],
    varying: u128,
    tile_len: usize,
    extractor: &mut E,
) -> NodePlan<E::Value> {
    // One digit that covers every remaining difference leaves finished
    // buckets, so no sample or run check can select a cheaper partition.
    let low = varying.trailing_zeros() as usize;
    let span = bit_len(varying) - low;
    if span <= SPLIT_BITS {
        return NodePlan::Digit(Digit {
            shift: low,
            width: span,
        });
    }
    let sample = (records.len() >= 4 * tile_len).then(|| KeySample::collect(records, extractor));
    if let Some(equal) = sample.as_ref().and_then(KeySample::mode) {
        return NodePlan::Equal(equal);
    }
    plan_digit(records, varying, tile_len, extractor, sample)
}

/// Returns the bit-consuming plan that replaces an equality partition which
/// left the node unchanged, or `None` when its bands progressed.
///
/// A changing key can leave every record below or above the sampled key, so
/// equality would repeat indefinitely; a sample cannot certify progress, only
/// the partition's `ends` can. Both executors consult this one rule.
pub(super) fn plan_after_equality<T, E: Extract<T>>(
    records: &[T],
    ends: &[usize],
    varying: u128,
    tile_len: usize,
    extractor: &mut E,
) -> Option<NodePlan<E::Value>> {
    let unchanged = ends[0] == records.len() || ends[1] == 0;
    unchanged.then(|| plan_digit(records, varying, tile_len, extractor, None))
}

/// Reuses any equality sample when planning a bit-consuming partition.
fn plan_digit<T, E: Extract<T>>(
    records: &[T],
    varying: u128,
    tile_len: usize,
    extractor: &mut E,
    sample: Option<KeySample<E::Value>>,
) -> NodePlan<E::Value> {
    let top = bit_len(varying);
    if let Some((boundary, set)) = two_runs(records, 1u128 << (top - 1), extractor) {
        NodePlan::Runs { boundary, set }
    } else {
        let sample = sample.unwrap_or_else(|| KeySample::collect(records, extractor));
        NodePlan::Digit(choose_split_digit(records.len(), top, tile_len, &sample))
    }
}

/// An executable tile decision; cursor layout and scratch remain private to tiles.
pub(super) enum TilePlan {
    Insertion,
    Msd(Digit),
    Lsd { digits: [Digit; 2], passes: usize },
}

/// Chooses a leaf for a nonconstant, nonempty tile without allocating scratch.
pub(super) fn plan_tile<T>(len: usize, varying: u128) -> TilePlan {
    if len <= INSERTION_MAX {
        return TilePlan::Insertion;
    }
    plan_digits(varying, tile_digit_bits(len, core::mem::size_of::<T>()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Direct;
    use alloc::vec::Vec;
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
                assert_eq!(sample.top_digit_occupancy(digit), buckets.len());
            }
        }
    }

    #[test]
    fn unchanged_equality_partitions_consume_key_bits() {
        let len = 4096;
        let records: Vec<u64> = (0..len as u64)
            .map(|key| key.wrapping_mul(0x9e37_79b9))
            .collect();
        let varying = records.iter().fold(0, |bits, &key| bits | key) as u128;
        let replan = |ends: &[usize]| plan_after_equality(&records, ends, varying, 64, &mut Direct);
        // Every record below or every record above the sampled key: the
        // replacement must split on a bit rather than sample equality again.
        for ends in [[len, len, len, len], [0, 0, len, len]] {
            assert!(matches!(
                replan(&ends),
                Some(NodePlan::Digit(_) | NodePlan::Runs { .. })
            ));
        }
        // Any separated band, including an empty equal band, is progress.
        for ends in [[1, 1, len, len], [0, 1, len, len], [len - 1, len, len, len]] {
            assert!(replan(&ends).is_none());
        }
    }

    #[test]
    fn locally_ordered_nodes_keep_the_initial_split_width() {
        // Neighbouring records share their top digit. A sample of adjacent
        // keys would under-count occupancy and widen past what it can test.
        let records: Vec<u64> = (0..65536).collect();
        let sample = KeySample::collect(&records, &mut Direct);
        assert_eq!(choose_split_digit(records.len(), 16, 512, &sample).width, 8);
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
    fn digit_plan_finishes_short_plans_optimally() {
        for bits in 1..256u128 {
            for width in 1..=5 {
                let optimum = optimal_digit_cost(bits, width);
                match plan_digits(bits, width) {
                    TilePlan::Lsd { digits, passes } => {
                        assert!(optimum.0 <= 2);
                        assert_eq!(
                            (
                                passes,
                                digits[..passes].iter().map(|digit| 1 << digit.width).sum()
                            ),
                            optimum
                        );
                        let mut mask = 0;
                        for digit in &digits[..passes] {
                            assert_eq!(mask & digit.mask::<u128>(), 0);
                            mask |= digit.mask::<u128>();
                        }
                        assert_eq!(mask & bits, bits);
                    }
                    TilePlan::Msd(digit) => {
                        assert!(optimum.0 > 2);
                        assert_ne!(bits & digit.mask::<u128>(), 0);
                    }
                    TilePlan::Insertion => unreachable!(),
                }
            }
        }
        for shift in [0, 17, 96] {
            let TilePlan::Lsd { digits, passes } = plan_digits(0xffff_ffffu128 << shift, 16) else {
                panic!("two-pass plan expected")
            };
            assert_eq!(passes, 2);
            assert_eq!(digits[0].width, 16);
            assert_eq!(digits[1].width, 16);
        }
        let TilePlan::Lsd { digits, passes } = plan_digits((1 << 127) | 1, 14) else {
            panic!("two sparse passes expected")
        };
        assert_eq!(passes, 2);
        assert_eq!(
            digits[0].mask::<u128>() | digits[1].mask::<u128>(),
            (1 << 127) | 1
        );
        assert!(matches!(
            plan_tile::<u64>(48, u64::MAX as u128),
            TilePlan::Insertion
        ));
    }
}
