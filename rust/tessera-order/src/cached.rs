// SPDX-License-Identifier: BSD-2-Clause

//! Sorting with one key evaluation per record.
//!
//! Every encoded key is cached once, in input order. The cached keys reveal
//! what a key function cannot promise in advance: whether the input is already
//! ordered and which bits vary. When the varying bits fit one stable
//! distribution, as with category keys, the records move once by those bits
//! and the distribution replays the cached keys instead of calling the key
//! function. Otherwise the keys are sorted as (key, input index) pairs and the
//! records follow the sorted indices.

use crate::{
    engine,
    key::{ByKey, RadixKey, Unsigned, bit_len},
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
    // A key panic propagates here, before any record has moved.
    let keys: Vec<K::Encoded> = values.iter().map(|record| key(record).encode()).collect();
    let first = keys[0].to_u128();
    let mut varying = 0;
    let mut ordered = true;
    for pair in keys.windows(2) {
        varying |= pair[1].to_u128() ^ first;
        ordered &= pair[0] <= pair[1];
    }
    if ordered {
        return;
    }
    // Bits outside the window are equal in every key, so the window alone
    // orders the records.
    let shift = varying.trailing_zeros() as usize;
    let width = bit_len(varying) - shift;
    if width <= MAX_DISTRIBUTION_BITS {
        let mask = (1 << width) - 1;
        // The distribution classifies each record exactly once, in input
        // order, so its `n`th call belongs to the `n`th cached key.
        let mut replay = keys.iter();
        partition::distribute(
            values,
            width,
            ByKey(|_: &T| {
                let key = replay.next().expect("one classification per record");
                (key.to_u128() >> shift) as u16 & mask
            }),
        );
    } else if values.len() <= u32::MAX as usize {
        sort_by_index::<_, _, u32>(values, keys);
    } else {
        sort_by_index::<_, _, usize>(values, keys);
    }
}

/// Pairs every cached key with its input index `I`, sorts the pairs stably by
/// key and moves each record to its sorted position.
///
/// `keys[i]` is the key of `values[i]`, and `I` holds every index of `values`.
/// Nothing here runs caller code or can panic once records start moving.
fn sort_by_index<T, V: Unsigned, I: Unsigned>(values: &mut [T], keys: Vec<V>) {
    let mut cache: Vec<(V, I)> = keys
        .into_iter()
        .enumerate()
        .map(|(index, key)| (key, I::from_usize(index)))
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
        let keys: Vec<_> = input.iter().map(key).collect();
        let mut narrow = input.clone();
        sort_by_index::<_, _, u32>(&mut narrow, keys.clone());
        let mut wide = input;
        sort_by_index::<_, _, usize>(&mut wide, keys);
        assert_eq!(narrow, expected);
        assert_eq!(wide, expected);
    }
}
