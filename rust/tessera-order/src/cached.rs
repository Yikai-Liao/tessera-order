// SPDX-License-Identifier: BSD-2-Clause

//! Sorting with one key evaluation per record.
//!
//! The key type decides the route. A key whose encoding fits one stable
//! distribution is its own category: the records move once, keyed by IDs the
//! distribution already caches, and no key is evaluated again. Wider keys are
//! cached as (encoded key, input index) pairs, the pairs are sorted, and the
//! records follow the sorted indices.

use crate::{
    engine,
    key::{ByKey, RadixKey, Unsigned},
    partition::{self, MAX_DISTRIBUTION_BITS},
};
use alloc::vec::Vec;

/// Sorts `values` stably by `key`, evaluating it at most once per record.
pub(crate) fn sort<T, K: RadixKey, F: FnMut(&T) -> K>(values: &mut [T], mut key: F) {
    // A slice of zero-sized records has one observable order, and fewer than
    // two records have no other; neither owes a key evaluation.
    if values.len() < 2 || size_of::<T>() == 0 {
        return;
    }
    let bits = <K::Encoded as Unsigned>::BITS;
    if bits <= MAX_DISTRIBUTION_BITS {
        // Ascending categories are ascending encoded keys, each in input order.
        partition::distribute(values, bits, ByKey(|record: &T| key(record).encode()));
    } else if values.len() <= u32::MAX as usize {
        sort_by_index::<_, _, u32>(values, |record| key(record).encode());
    } else {
        sort_by_index::<_, _, usize>(values, |record| key(record).encode());
    }
}

/// Caches every encoded key with its input index `I`, sorts the pairs stably by
/// key and moves each record to its sorted position.
///
/// `I` holds every index of `values`. A key panic leaves `values` untouched:
/// records move only after all keys are cached, and nothing after that point
/// runs caller code or can panic.
fn sort_by_index<T, V: Unsigned, I: Unsigned>(values: &mut [T], mut key: impl FnMut(&T) -> V) {
    let mut cache: Vec<(V, I)> = values
        .iter()
        .enumerate()
        .map(|(index, record)| (key(record), I::from_usize(index)))
        .collect();
    engine::sort(&mut cache, ByKey(|entry: &(V, I)| entry.0));
    // The sorted position `j` takes the record at input index `cache[j].1`.
    // Each cycle of that permutation is applied with swaps; a finished
    // position records itself as its source, so later starts skip it.
    for start in 0..values.len() {
        let mut position = start;
        loop {
            let source = cache[position].1.to_usize();
            cache[position].1 = I::from_usize(position);
            if source == start {
                break;
            }
            // `values[position]` holds the cycle's starting record and the
            // unfinished `values[source]` still holds its input record.
            values.swap(position, source);
            position = source;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sort_by_index;
    use alloc::{string::ToString, vec::Vec};

    /// Both index widths apply the same permutation; `usize` serves inputs
    /// longer than `u32::MAX` records, which tests cannot allocate.
    #[test]
    fn index_widths_apply_the_same_permutation() {
        let len = if cfg!(miri) { 65 } else { 1000 };
        let input: Vec<_> = (0..len).map(|id| (id, id.to_string())).collect();
        let key = |record: &(usize, _)| (record.0.wrapping_mul(37) % 11) as u16;
        let mut expected = input.clone();
        expected.sort_by_key(key);
        let mut narrow = input.clone();
        sort_by_index::<_, _, u32>(&mut narrow, key);
        let mut wide = input;
        sort_by_index::<_, _, usize>(&mut wide, key);
        assert_eq!(narrow, expected);
        assert_eq!(wide, expected);
    }
}
