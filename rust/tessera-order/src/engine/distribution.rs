// SPDX-License-Identifier: BSD-2-Clause

//! Complete stable category distribution for algorithm adapters.
//! Classification, movement and boundary output complete within one
//! invocation; sorting's order probes, rank requirements and recursive
//! finishing are not involved.

use super::{
    block::{self, Classify},
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
/// `extractor` assigns. `classified_prefix` retains already evaluated runs as
/// (exclusive record end, bucket); the extractor runs once for each remaining
/// record, in input order. An empty prefix evaluates every record, the contract
/// cached-key sorting uses to replay keys by call count.
///
/// Returns the cumulative end of every category, including empty ones. Keys
/// must be below `2^radix`. An extractor panic propagates after every record
/// is restored to `values`, in unspecified order.
pub(crate) fn distribute<T, E: Extract<T>>(
    values: &mut [T],
    radix: usize,
    classified_prefix: &[(usize, usize)],
    mut extractor: E,
) -> Vec<usize> {
    debug_assert!(values.len() >= 2 && size_of::<T>() != 0);
    debug_assert!(1 << radix <= MAX_PARTITION_BUCKETS);
    let mut previous = 0;
    for &(end, bucket) in classified_prefix {
        assert!(previous <= end && end <= values.len() && bucket < 1 << radix);
        previous = end;
    }
    if values.len() <= tile::capacity::<T>(TILE_BYTES) {
        tile::Scratch::new().distribute(values, radix, classified_prefix, &mut extractor)
    } else {
        block::partition(
            values,
            radix,
            Classified {
                classifier: KeyDigit::new(&mut extractor, 0),
                prefix: classified_prefix,
            },
            &mut block::Buffers::new(),
            &mut |_, _, _, _| {},
        )
    }
}

/// Retains the caller's observed classifications without a replay branch in
/// the classifier's hot suffix. The movement owner consumes whole runs first.
struct Classified<'a, C> {
    classifier: C,
    prefix: &'a [(usize, usize)],
}

impl<T, C: Classify<T>> Classify<T> for Classified<'_, C> {
    fn bucket(&mut self, record: &T) -> usize {
        self.classifier.bucket(record)
    }

    fn classified_prefix(&self) -> &[(usize, usize)] {
        self.prefix
    }
}
