// SPDX-License-Identifier: BSD-2-Clause

//! A single stable distribution for cached-key sorting. No order probe or
//! recursive finishing may interrupt the forward replay of cached digits.

use super::{
    block,
    key::{ByKey, KeyDigit, RadixKey},
    thresholds::TILE_BYTES,
    tile,
};
#[cfg(test)]
mod tests;

/// Largest category count one distribution moves, derived from block fan-out,
/// tile table capacity and category-ID storage.
pub(crate) const MAX_PARTITION_BUCKETS: usize = {
    let mut limit = 1usize << u16::BITS;
    if 1usize << block::MAX_RADIX_BITS < limit {
        limit = 1usize << block::MAX_RADIX_BITS;
    }
    if tile::MAX_DIGIT_BUCKETS < limit {
        limit = tile::MAX_DIGIT_BUCKETS;
    }
    limit
};

/// Stably moves at least two sized records into the `2^radix` categories that
/// `category` assigns, evaluating it exactly once per record, in input order.
/// Cached-key sorting relies on that order to replay keys by call count.
///
/// Keys must be below `2^radix`. An extractor panic propagates after every record
/// is restored to `values`, in unspecified order.
pub(crate) fn distribute<T, K: RadixKey>(
    values: &mut [T],
    radix: usize,
    category: impl FnMut(&T) -> K,
) {
    // ByKey retains category IDs; direct-key scatter may evaluate the key again.
    let mut extractor = ByKey(category);
    debug_assert!(values.len() >= 2 && size_of::<T>() != 0);
    debug_assert!(1 << radix <= MAX_PARTITION_BUCKETS);
    if values.len() <= tile::capacity::<T>(TILE_BYTES) {
        tile::Scratch::new().distribute(values, radix, &mut extractor)
    } else {
        block::partition(
            values,
            radix,
            KeyDigit::new(&mut extractor, 0),
            &mut block::Buffers::new(),
            &mut |_, _, _, _| {},
        );
    }
}
