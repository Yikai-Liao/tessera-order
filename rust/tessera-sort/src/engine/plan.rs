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

    /// Estimates one partition and its children in sampled-record pass units.
    ///
    /// Repeated groups estimate population and remaining variation together;
    /// a sample-constant group needs a confirming scan, not more sorting passes.
    /// A singleton cannot certify constancy: use the remaining candidate bits
    /// and a uniform bucket population for that unobserved part of the sample.
    /// Nonterminal groups compare flat work with the same estimate on their
    /// remaining bits. Recursion only projects this retained sample: it neither
    /// reads more records nor allocates, and every level consumes a digit.
    /// All facts are advisory; actual child scans still decide completion.
    fn partition_work(
        sample: &[V],
        records: usize,
        varying: u128,
        digit: Digit,
        record_bytes: usize,
    ) -> usize {
        let mask = digit.mask::<V>();
        let remaining = varying & !mask.to_u128();
        let singleton_len = records.div_ceil(1 << digit.width);
        let singleton_passes = if singleton_len <= INSERTION_MAX {
            0
        } else {
            digit_passes(remaining, tile_digit_bits(singleton_len, record_bytes))
        };
        // All records pay a partition and either insertion or a confirming scan.
        // Only children with work beyond that minimum need further estimation.
        let mut work = 2 * sample.len();
        let mut start = 0;
        while start < sample.len() {
            let bucket = sample[start] & mask;
            let mut end = start + 1;
            while end < sample.len() && sample[end] & mask == bucket {
                end += 1;
            }
            let keys = &sample[start..end];
            start = end;
            if keys.len() == 1 {
                work += singleton_passes;
                continue;
            }
            let child_len = (records * keys.len()).div_ceil(sample.len());
            if child_len <= INSERTION_MAX || keys.first() == keys.last() {
                continue;
            }
            let first = keys[0].to_u128();
            let varying = keys
                .iter()
                .fold(0, |bits, key| bits | (key.to_u128() ^ first))
                & remaining;
            let width = tile_digit_bits(child_len, record_bytes);
            let passes = digit_passes(varying, width);
            let flat = keys.len() * passes;
            let continuation = if passes > 2 {
                let (digit, _) = top_window(varying, width);
                flat.min(Self::partition_work(
                    keys,
                    child_len,
                    varying,
                    digit,
                    record_bytes,
                ))
            } else {
                flat
            };
            work += continuation;
        }
        work
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

/// A minimum-pass plan cannot exceed the number of windows needed for all bits.
const MAX_DIGITS: usize = (u128::BITS as usize).div_ceil(MIN_TILE_DIGIT_BITS);

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
    Window(Digit),
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
    let low = varying.trailing_zeros() as usize;
    let span = bit_len(varying) - low;
    if span <= SPLIT_BITS {
        return NodePlan::Window(Digit {
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

/// Selects a bit-consuming partition after an equality split failed to progress.
/// Sample reuse stays private; the executor needs only the progress guarantee.
pub(super) fn plan_progress<T, E: Extract<T>>(
    records: &[T],
    varying: u128,
    tile_len: usize,
    extractor: &mut E,
) -> NodePlan<E::Value> {
    plan_digit(records, varying, tile_len, extractor, None)
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
#[allow(
    clippy::large_enum_variant,
    reason = "The bounded plan stays on the stack; boxing would allocate for every LSD tile"
)]
pub(super) enum TilePlan {
    Insertion,
    Msd(Digit),
    Lsd {
        digits: [Digit; MAX_DIGITS],
        passes: usize,
    },
}

/// Chooses a leaf for a nonconstant, nonempty tile without allocating scratch.
pub(super) fn plan_tile<T, E: Extract<T>>(
    records: &[T],
    varying: u128,
    extractor: &mut E,
) -> TilePlan {
    let len = records.len();
    if len <= INSERTION_MAX {
        return TilePlan::Insertion;
    }
    // Widest counting table this tile can afford; LSD spreads it over passes.
    let table_width = tile_digit_bits(len, core::mem::size_of::<T>());
    let passes = digit_passes(varying, table_width);
    // One partition plus child confirmation or insertion costs at least two
    // passes in this estimate, so short flat plans need no sample.
    if passes > 2 {
        let (digit, _) = top_window(varying, table_width);
        let sample = KeySample::collect(records, extractor);
        if KeySample::partition_work(
            &sample.keys[..sample.len],
            len,
            varying,
            digit,
            core::mem::size_of::<T>(),
        ) < sample.len * passes
        {
            return TilePlan::Msd(digit);
        }
    }
    let mut digits = [Digit { shift: 0, width: 0 }; MAX_DIGITS];
    let passes = plan_digits(varying, table_width, &mut digits);

    TilePlan::Lsd { digits, passes }
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
    fn locally_ordered_nodes_keep_the_initial_split_width() {
        // Neighbouring records share their top digit. A sample of adjacent
        // keys would under-count occupancy and widen past what it can test.
        let records: Vec<u64> = (0..65536).collect();
        let sample = KeySample::collect(&records, &mut Direct);
        assert_eq!(choose_split_digit(records.len(), 16, 512, &sample).width, 8);
    }

    #[test]
    fn sampled_terminal_groups_generalize_low_fanout() {
        for groups in [1, 2, 3, 4, 8, 16, 32, 64] {
            let records: Vec<u64> = (0..1024)
                .map(|index| ((index % groups) as u64) << 56)
                .collect();
            let sample = KeySample::collect(&records, &mut Direct);
            let digit = Digit {
                shift: 52,
                width: 12,
            };
            // Constant children pay only their confirmation, regardless of how
            // many groups the sample contains. No low-fanout cutoff is needed.
            assert_eq!(
                KeySample::partition_work(
                    &sample.keys[..sample.len],
                    records.len(),
                    u64::MAX as u128,
                    digit,
                    8
                ),
                2 * sample.len
            );
            assert!(matches!(
                plan_tile(&records, u64::MAX as u128, &mut Direct),
                TilePlan::Msd(_)
            ));
        }
    }

    #[test]
    fn concentrated_nonterminal_groups_keep_remaining_work() {
        let records: Vec<u64> = (0..1024u64)
            .map(|index| index.wrapping_mul(0x9e3779b97f4a7c15) & ((1 << 52) - 1))
            .collect();
        let sample = KeySample::collect(&records, &mut Direct);
        let digit = Digit {
            shift: 52,
            width: 12,
        };
        assert!(
            KeySample::partition_work(
                &sample.keys[..sample.len],
                records.len(),
                u64::MAX as u128,
                digit,
                8
            ) > 2 * sample.len
        );
        // Known-completed bits do not contribute to estimated child work.
        assert_eq!(
            KeySample::partition_work(
                &sample.keys[..sample.len],
                records.len(),
                digit.mask::<u128>(),
                digit,
                8
            ),
            2 * sample.len
        );
        assert!(matches!(
            plan_tile(&records, u64::MAX as u128, &mut Direct),
            TilePlan::Msd(_)
        ));
        // One key in a bucket does not establish constancy when the estimated
        // child still exceeds the insertion bound.
        let singleton = KeySample {
            keys: [0u64; KEY_SAMPLE_LEN],
            len: 1,
        };
        assert!(
            KeySample::partition_work(
                &singleton.keys[..singleton.len],
                8192,
                u64::MAX as u128,
                Digit {
                    shift: 63,
                    width: 1
                },
                8
            ) > 2
        );
    }

    #[test]
    fn sampled_continuation_reuses_deeper_partition_opportunities() {
        for groups in [2u64, 4, 8] {
            let records: Vec<u64> = (0..1024u64)
                .map(|index| {
                    ((index % groups) << 60)
                        | (index.wrapping_mul(0x9e3779b97f4a7c15) & ((1 << 40) - 1))
                })
                .collect();
            let varying = records.iter().fold(0u128, |bits, key| {
                bits | (*key as u128 ^ records[0] as u128)
            });
            let sample = KeySample::collect(&records, &mut Direct);
            let width = tile_digit_bits(records.len(), 8);
            let (digit, _) = top_window(varying, width);
            let work = KeySample::partition_work(
                &sample.keys[..sample.len],
                records.len(),
                varying,
                digit,
                8,
            );
            assert!(work > 2 * sample.len);
            assert!(work < digit_passes(varying, width) * sample.len);
            assert!(matches!(
                plan_tile(&records, varying, &mut Direct),
                TilePlan::Msd(_)
            ));
        }
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
