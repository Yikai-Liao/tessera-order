// SPDX-License-Identifier: BSD-2-Clause

//! A radix partition of source positions. Its prefix counts locate a rank or
//! assign stable destinations before the caller commits to record movement.

use super::{Digit, Unsigned, bit_len};
use core::ops::Range;

/// One byte per position is enough to describe a partition into these bands.
pub(super) const FANOUT: usize = 1 << u8::BITS;

pub(super) struct Bands {
    ends: [usize; FANOUT],
}

impl Bands {
    /// Use the high byte of the varying window. Omit constant low bits when
    /// that lets this single partition resolve the entire order.
    pub(super) fn window(varying: u128) -> Digit {
        let top = bit_len(varying);
        let shift = top
            .saturating_sub(u8::BITS as usize)
            .max(varying.trailing_zeros() as usize);
        Digit {
            shift,
            width: top - shift,
        }
    }

    pub(super) fn count(digits: impl Iterator<Item = u8>) -> Self {
        let mut ends = [0; FANOUT];
        for digit in digits {
            ends[usize::from(digit)] += 1;
        }
        let mut sum = 0;
        for end in &mut ends {
            sum += *end;
            *end = sum;
        }
        Self { ends }
    }

    /// Generate output directly at stable destinations. If a user callback
    /// changes a classification since counting, only return a permutation
    /// when every disjoint band was filled exactly once.
    pub(super) fn emit(&self, output: &mut [usize], words: impl Iterator<Item = (u8, usize)>) {
        let mut next = self.ends;
        next.rotate_right(1);
        next[0] = 0;
        for (digit, word) in words {
            let slot = &mut next[usize::from(digit)];
            output[*slot] = word;
            *slot += 1;
        }
        assert!(next == self.ends, "inconsistent radix partition");
    }

    pub(super) fn ranges(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        let mut start = 0;
        self.ends
            .iter()
            .map(move |&end| core::mem::replace(&mut start, end)..end)
            .filter(|r| !r.is_empty())
    }
}

impl Digit {
    #[inline]
    pub(super) fn byte<V: Unsigned>(self, key: V) -> u8 {
        ((key >> V::from_usize(self.shift)).to_usize() & ((1 << self.width) - 1)) as u8
    }
}
