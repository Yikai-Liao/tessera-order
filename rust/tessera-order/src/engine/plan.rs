// SPDX-License-Identifier: BSD-2-Clause

//! Node partition decisions from key variation, bounded samples and work estimates.
//! The executor receives a complete decision, not sample layout or cost-model steps.

use crate::block;
use crate::key::{Digit, Extract, Unsigned, bit_len};

/// Initial split width; also the block engine's tuned width.
const SPLIT_BITS: usize = 8;

/// Split fan-out is bounded by the block engine's widest digit.
const MAX_SPLIT_BITS: usize = block::MAX_RADIX_BITS;

/// Bounded sample shared by equality and digit-occupancy decisions.
pub(super) const KEY_SAMPLE_LEN: usize = 128;
/// A value occupying this fraction of the sample merits an equality partition.
const MODE_SHARE_DENOMINATOR: usize = 4;

/// The record that a nonempty node of `len` records samples for `stratum`.
///
/// Each of the first `min(len, KEY_SAMPLE_LEN)` equal strata contributes one
/// record. A golden-ratio offset within each stratum avoids aliasing with
/// periodic input, and locally ordered keys still yield independent
/// observations.
pub(super) fn sample_index(len: usize, stratum: usize) -> usize {
    let stride = len / len.min(KEY_SAMPLE_LEN);
    // A fixed-point fraction times `stride` stays below `stride`.
    let fraction = (stratum as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    stratum * stride + ((u128::from(fraction) * stride as u128) >> 64) as usize
}

/// Sorted sampled keys, independent of the digit currently being considered.
/// This is advisory evidence only: execution still scans actual child variation.
struct KeySample<V> {
    keys: [V; KEY_SAMPLE_LEN],
    len: usize,
}

impl<V: Unsigned> KeySample<V> {
    /// Reads one key per stratum of a nonempty input, then sorts them.
    fn collect<T, E: Extract<T, Value = V>>(records: &[T], extractor: &mut E) -> Self {
        let len = records.len().min(KEY_SAMPLE_LEN);
        let mut sample = Self {
            keys: [V::from_usize(0); KEY_SAMPLE_LEN],
            len,
        };
        for (stratum, key) in sample.keys[..len].iter_mut().enumerate() {
            *key = extractor.key(&records[sample_index(records.len(), stratum)]);
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

/// Chooses an initial width targeting half-tile children, then widens if needed.
fn choose_split_digit<V: Unsigned>(
    len: usize,
    top: usize,
    varying: u128,
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
        let wider = (width + 2).min(top);
        // Bits no key varies in cannot occupy another bucket.
        let added = ((1u128 << (wider - width)) - 1) << (top - wider);
        if varying & added == 0 {
            break;
        }
        width = wider;
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
    let mut holds = records
        .iter()
        .map(|record| extractor.key(record).to_u128() & bit != 0);
    let first = holds.next()?;
    // The record that breaks the first run starts the second one.
    let boundary = 1 + holds.position(|held| held != first)?;
    holds.all(|held| held != first).then_some((boundary, first))
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
    let top = bit_len(varying);
    if let Some((boundary, set)) = two_runs(records, 1u128 << (top - 1), extractor) {
        NodePlan::Runs { boundary, set }
    } else {
        let sample = sample.unwrap_or_else(|| KeySample::collect(records, extractor));
        NodePlan::Digit(choose_split_digit(
            records.len(),
            top,
            varying,
            tile_len,
            &sample,
        ))
    }
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
        assert_eq!(
            choose_split_digit(records.len(), 16, 0xffff, 512, &sample).width,
            8
        );
    }

    #[test]
    fn concentrated_windows_do_not_widen_over_constant_bits() {
        // Two clusters 2^63 apart, each uniform over 2^40 keys: every window
        // below the top bit up to bit 40 holds the same two values, so a wider
        // digit adds empty buckets, and with them block scratch, but no split.
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let records: Vec<u64> = (0..65536)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state & (1 << 63)) | (state & ((1 << 40) - 1))
            })
            .collect();
        let varying = (1u128 << 63) | ((1 << 40) - 1);
        let sample = KeySample::collect(&records, &mut Direct);
        assert_eq!(
            choose_split_digit(records.len(), 64, varying, 512, &sample).width,
            8
        );
    }
}
