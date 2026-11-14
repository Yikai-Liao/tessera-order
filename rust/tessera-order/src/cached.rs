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

use crate::engine::{self, ByKey, MAX_PARTITION_BUCKETS, RadixKey, Unsigned, bit_len};
use alloc::vec::Vec;

/// Sorts `values` stably by `key`, evaluating it at most once per record.
pub(crate) fn sort<T, K: RadixKey, F: FnMut(&T) -> K>(values: &mut [T], mut key: F) {
    // A slice of zero-sized records has one observable order, and fewer than
    // two records have no other; neither owes a key evaluation.
    if values.len() < 2 || size_of::<T>() == 0 {
        return;
    }
    // A key panic propagates here, before any record has moved. The same pass
    // records which bits differ from the first key and whether keys ascend.
    let mut keys: Vec<K::Encoded> = Vec::with_capacity(values.len());
    let first = key(&values[0]).encode();
    keys.push(first);
    let mut previous = first;
    let mut varying = 0;
    let mut ordered = true;
    for record in &values[1..] {
        let current = key(record).encode();
        keys.push(current);
        varying |= current.to_u128() ^ first.to_u128();
        ordered &= previous <= current;
        previous = current;
    }
    if ordered {
        return;
    }
    // Bits outside the window are equal in every key, so the window alone
    // orders the records.
    let low = varying.trailing_zeros() as usize;
    let width = bit_len(varying) - low;
    if width <= MAX_PARTITION_BUCKETS.ilog2() as usize {
        let shift = <K::Encoded as Unsigned>::from_usize(low);
        let mask = <K::Encoded as Unsigned>::from_usize((1 << width) - 1);
        // The distribution classifies each record exactly once, in input
        // order, so its `n`th call belongs to the `n`th cached key.
        let mut next = 0;
        engine::distribute(
            values,
            width,
            ByKey(|_: &T| {
                let digit = (keys[next] >> shift) & mask;
                next += 1;
                digit
            }),
        );
    } else {
        sort_by_index(values, keys);
    }
}

/// Pairs every cached key with its input index, sorts the pairs stably by key
/// and moves each record to its sorted position. `keys[i]` is the key of
/// `values[i]`. Nothing here runs caller code or can panic.
fn sort_by_index<T, V: Unsigned>(values: &mut [T], keys: Vec<V>) {
    let mut cache: Vec<(V, usize)> = keys.into_iter().zip(0..).collect();
    engine::sort(&mut cache, ByKey(|entry: &(V, usize)| entry.0));
    // The sorted position `j` takes the record at input index `cache[j].1`.
    // Each cycle of that permutation is applied with swaps; a finished
    // position records itself as its source, so later starts skip it.
    for start in 0..values.len() {
        let mut position = start;
        loop {
            let source = cache[position].1;
            cache[position].1 = position;
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
