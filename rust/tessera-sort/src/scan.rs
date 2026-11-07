// SPDX-License-Identifier: BSD-2-Clause
//! The default adjacent-pair scan.
//!
//! Direct extraction uses this without the `portable-simd` feature. It has no
//! dependency and lets the compiler auto-vectorize at the ambient target
//! width. The optional backend dispatches the insertion pass as a whole.

use crate::RadixKey;

/// Returns the offset of the first adjacent pair whose encoded keys descend.
///
/// Scans 32 pairs per mask update. The inner loop has a fixed trip count and
/// unchecked indexing, allowing LLVM to vectorize encodings supported by the
/// ambient target. Building the mask locates the first inversion without a
/// second pass over the block.
#[inline(always)]
pub(crate) fn first_inversion<K: RadixKey>(keys: &[K]) -> Option<usize> {
    const BLOCK: usize = 32;
    // Most inversions in unsorted input sit within the first few pairs, and a
    // scalar check finds them without paying for a full block.
    const PEEL: usize = 4;
    let pairs = keys.len().saturating_sub(1);
    let mut index = 0;
    while index < PEEL.min(pairs) {
        if keys[index].encode() > keys[index + 1].encode() {
            return Some(index);
        }
        index += 1;
    }
    while index + BLOCK <= pairs {
        let mut mask = 0u32;
        for offset in 0..BLOCK {
            // SAFETY: index + BLOCK <= pairs = len - 1, so both indices stay
            // below len.
            let (before, after) = unsafe {
                (
                    keys.get_unchecked(index + offset),
                    keys.get_unchecked(index + offset + 1),
                )
            };
            mask |= ((before.encode() > after.encode()) as u32) << offset;
        }
        if mask != 0 {
            return Some(index + mask.trailing_zeros() as usize);
        }
        index += BLOCK;
    }
    while index < pairs {
        if keys[index].encode() > keys[index + 1].encode() {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// Scans one pair at a time.
///
/// Used for encodings whose per-element cost outweighs the wider comparison,
/// because the baseline vector width cannot compare 64-bit lanes.
#[inline(always)]
pub(crate) fn scalar<K: RadixKey>(keys: &[K]) -> Option<usize> {
    let mut keys = keys.iter();
    let first = keys.next()?;
    let mut previous = first.encode();
    for (index, key) in keys.enumerate() {
        let current = key.encode();
        if previous > current {
            return Some(index);
        }
        previous = current;
    }
    None
}
