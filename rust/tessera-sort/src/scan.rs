// SPDX-License-Identifier: BSD-2-Clause

//! Scalar and blocked adjacent-pair scans for `portable_simd`.
//!
//! Short scans and key types wider than a vector register use the scalar
//! scan. `Reverse<K>` uses the blocked scan, which the compiler vectorizes at
//! the width of the backend selected by the caller.

use crate::RadixKey;

/// Returns the offset of the first adjacent pair whose encoded keys descend.
///
/// Scans 32 pairs per mask update. The inner loop has a fixed trip count and
/// unchecked indexing, allowing LLVM to vectorize encodings supported by the
/// ambient target. Building the mask locates the first inversion without a
/// second pass over the block. The caller has already checked the first few
/// pairs one at a time.
#[inline(always)]
pub(crate) fn blocked<K: RadixKey>(keys: &[K]) -> Option<usize> {
    const BLOCK: usize = 32;
    let pairs = keys.len().saturating_sub(1);
    let mut index = 0;
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

/// Returns the offset of the first adjacent pair whose encoded keys descend,
/// scanning one pair at a time.
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
