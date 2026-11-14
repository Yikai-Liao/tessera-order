// SPDX-License-Identifier: BSD-2-Clause

//! One-shot stable category distribution. Validation, classifier evaluation,
//! movement and boundary output complete within one invocation; sorting's
//! order probes, rank requirements and recursive finishing are not involved.

use crate::{
    block,
    key::{ByKey, Extract, KeyDigit},
    thresholds::TILE_BYTES,
    tile,
};
use alloc::{vec, vec::Vec};

/// Largest category count accepted by [`stable_partition_by_bucket`].
///
/// Derived from block fan-out, tile table capacity and category-ID storage.
/// Counts need not be powers of two; padding adds no public categories.
pub const MAX_PARTITION_BUCKETS: usize = {
    let mut limit = 1usize << u16::BITS;
    if 1usize << block::MAX_RADIX_BITS < limit {
        limit = 1usize << block::MAX_RADIX_BITS;
    }
    if tile::MAX_DIGIT_BUCKETS < limit {
        limit = tile::MAX_DIGIT_BUCKETS;
    }
    limit
};

/// Stably places matching records before nonmatching records.
///
/// Returns the matching count: `values[..count]` contains the matching
/// records and the suffix contains the others, each in original input order.
/// Evaluates `predicate` exactly once per record, in input order on success,
/// including zero-sized records. Records need not be `Clone`, `Send` or `Sync`.
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
/// let count = tessera_order::stable_partition(&mut rows, |row| row.0 % 2 == 0);
/// assert_eq!(count, 2);
/// assert_eq!(rows, [(2, "a"), (4, "c"), (1, "b"), (3, "d")]);
/// ```
pub fn stable_partition<T, F: FnMut(&T) -> bool>(values: &mut [T], mut predicate: F) -> usize {
    stable_partition_by_bucket(values, 2, |record| usize::from(!predicate(record)))[1]
}

/// Stably groups records by categories `0..bucket_count`.
///
/// Returns `bucket_count + 1` offsets, starting at zero and ending at
/// `values.len()`. Category `i` occupies `values[offsets[i]..offsets[i + 1]]`
/// in original input order; empty categories have repeated offsets.
///
/// Evaluates `classify` exactly once per record, in input order on success,
/// including singleton inputs, one category and zero-sized records. Categories
/// are assigned by those calls; no subsequent classifier evaluation is needed.
/// Records need not implement `Clone`, `Send` or `Sync`.
///
/// # Panics
/// A count outside `1..=MAX_PARTITION_BUCKETS` panics before allocation,
/// mutation or classifier calls. An out-of-range classification or classifier
/// panic propagates after every record is restored, in unspecified order.
/// Callback side effects are not rolled back.
///
/// # Memory
/// The offsets are O(R) output for R categories. Auxiliary space is
/// O(√(nR) + R) for a fixed record size, plus bounded tile storage. For a
/// fixed category count this is O(√n), not a category-independent promise.
/// All storage except the returned offsets is released on return or unwind.
///
/// # Examples
/// ```
/// let mut rows = [(2, "a"), (0, "b"), (2, "c"), (0, "d")];
/// let offsets = tessera_order::stable_partition_by_bucket(&mut rows, 3, |r| r.0);
/// assert_eq!(offsets, [0, 2, 2, 4]);
/// assert_eq!(rows, [(0, "b"), (0, "d"), (2, "a"), (2, "c")]);
/// ```
pub fn stable_partition_by_bucket<T, F: FnMut(&T) -> usize>(
    values: &mut [T],
    bucket_count: usize,
    mut classify: F,
) -> Vec<usize> {
    assert!(
        (1..=MAX_PARTITION_BUCKETS).contains(&bucket_count),
        "unsupported category count"
    );
    let mut offsets = vec![0; bucket_count + 1];
    let mut category = |record: &T| {
        let bucket = classify(record);
        assert!(bucket < bucket_count, "category out of bounds");
        bucket as u16
    };
    if values.len() < 2 || size_of::<T>() == 0 || bucket_count == 1 {
        // These cases still owe every classifier call and ID validation, but
        // need no physical movement. In particular, ZSTs are not sort no-ops.
        for record in values.iter() {
            offsets[category(record) as usize + 1] += 1;
        }
        for bucket in 0..bucket_count {
            offsets[bucket + 1] += offsets[bucket];
        }
        return offsets;
    }
    let radix = bucket_count.next_power_of_two().trailing_zeros() as usize;
    let ends = distribute(values, radix, ByKey(category));
    // Validation ran before masking, so padded buckets are empty. Both complete
    // movement operations return cumulative ends, including empty categories.
    offsets[1..].copy_from_slice(&ends[..bucket_count]);
    offsets
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
