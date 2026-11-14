// SPDX-License-Identifier: BSD-2-Clause

//! Complete tile decisions derived from remaining bits and tile-owned cursor limits.

use super::{Cursor, MAX_DIGIT_BUCKETS};
use crate::engine::key::Digit;
use crate::engine::thresholds::INSERTION_MAX;

/// Widest digit supported by the tile cursor representation and budget.
const MAX_TILE_DIGIT_BITS: usize = MAX_DIGIT_BUCKETS.ilog2() as usize;
/// Narrowest digit a leaf may use.
const MIN_TILE_DIGIT_BITS: usize = 6;

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
    use crate::engine::key::bit_len;

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

    // The recursive oracle checks plan optimality using safe integer arithmetic;
    // interpreting its exhaustive search adds no ownership or aliasing coverage.
    #[test]
    #[cfg_attr(miri, ignore = "exhaustive arithmetic is checked by native tests")]
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
