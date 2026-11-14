// SPDX-License-Identifier: BSD-2-Clause

//! One-shot stable category distribution, behind binary partition and
//! cached-key sorting. Classifier evaluation, movement and boundary output
//! complete within one invocation; sorting's order probes, rank requirements
//! and recursive finishing are not involved.

use crate::{
    block,
    key::{ByKey, Extract, KeyDigit},
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

/// Moves the records matching `predicate` before the others, keeping input
/// order on both sides, and returns the matching count.
///
/// C++ `std::stable_partition`: `values[..count]` holds the matching records
/// and `values[count..]` the others, so the count is the slice's
/// `partition_point` for the predicate afterwards. Calls `predicate` exactly
/// once per record, in input order, also for zero-sized records. Records need
/// not be `Clone`, `Send` or `Sync`.
///
/// # Panics
/// A predicate panic propagates after every record is restored to the slice,
/// in unspecified order. Callback side effects are not rolled back.
///
/// # Memory
/// O(√n) auxiliary space for a fixed record size, plus bounded tile storage.
/// All private storage is released on return or unwind.
///
/// # Examples
/// ```
/// let mut rows = [(2, "a"), (1, "b"), (4, "c"), (3, "d")];
/// let count = tessera_order::partition(&mut rows, |row| row.0 % 2 == 0);
/// assert_eq!(count, 2);
/// assert_eq!(rows, [(2, "a"), (4, "c"), (1, "b"), (3, "d")]);
/// ```
pub fn partition<T, F: FnMut(&T) -> bool>(values: &mut [T], mut predicate: F) -> usize {
    if values.len() < 2 || size_of::<T>() == 0 {
        // No movement is needed, but every predicate call is still owed.
        return values.iter().filter(|record| predicate(record)).count();
    }
    distribute(values, 1, ByKey(|record: &T| u8::from(!predicate(record))))[0]
}

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
