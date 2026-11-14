// SPDX-License-Identifier: BSD-2-Clause

//! Complete stable category distribution for algorithm adapters.
//! Classification, movement and boundary output complete within one invocation; sorting's order probes, rank requirements
//! and recursive finishing are not involved.

use super::{
    block,
    key::{Extract, KeyDigit},
    thresholds::TILE_BYTES,
    tile,
};
use alloc::vec::Vec;

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
/// `extractor` assigns, evaluating it exactly once per record, in input order.
/// Cached-key sorting relies on that order to replay keys by call count.
///
/// Returns the cumulative end of every category, including empty ones. Keys
/// must be below `2^radix`. An extractor panic propagates after every record
/// is restored to `values`, in unspecified order.
pub(crate) fn distribute<T, E: Extract<T>>(
    values: &mut [T],
    radix: usize,
    mut extractor: E,
) -> Vec<usize> {
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
        )
    }
}
