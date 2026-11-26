// SPDX-License-Identifier: BSD-2-Clause

//! Sorting with one key evaluation per record.
//!
//! Every encoded key is cached once, in input order. The cached keys reveal
//! what a key function cannot promise in advance: whether the input is already
//! ordered and which bits vary. When the varying bits fit one stable
//! distribution, as with category keys, the records move once by those bits
//! and the distribution replays the cached keys instead of calling the key
//! function. Otherwise the engine orders the source indices by the cached keys,
//! then moves each record to its destination.

use crate::engine::{self, MAX_PARTITION_BUCKETS, RadixKey, Unsigned, bit_len};
use alloc::vec::Vec;

/// Sorts records by a key, evaluating the key function at most once per record
/// and keeping records with equal keys in input order.
///
/// Use this instead of [`crate::sort_by_key`] when `key` is expensive or must
/// not run twice for a record, as with [`slice::sort_by_cached_key`]. Records
/// of any type are moved, never cloned or dropped.
///
/// # Key function
///
/// On success, `key` is called exactly once per record, in input order, unless
/// `values` has fewer than two records or `T` is zero-sized, in which case it
/// is not called. The returned key for each record is used as-is; `key` need
/// not be repeatable.
///
/// # Panics
///
/// If `key` unwinds, the panic propagates with every original record still in
/// `values` exactly once, in an unspecified order. Changes made through
/// interior mutability remain visible; side effects are not rolled back.
/// Recovery does not call `key`.
///
/// # Memory
///
/// Caches every key, O(n) as in the standard library, also for ordered
/// inputs, whose order is known only once every key is evaluated. Keys that
/// differ only in a narrow bit window, such as category IDs, then move the
/// records once within the [`crate::sort_by_key`] scratch bound; other keys sort
/// source indices with [`crate::argsort`] workspace.
///
/// # Examples
///
/// ```
/// let mut words = ["delta", "Alpha", "charlie", "Bravo"];
/// let mut calls = 0;
/// tessera_order::sort_by_cached_key(&mut words, |word| {
///     calls += 1;
///     word.to_lowercase().as_bytes()[0]
/// });
/// assert_eq!(words, ["Alpha", "Bravo", "charlie", "delta"]);
/// assert_eq!(calls, 4);
/// ```
pub fn sort_by_cached_key<T, K, F>(values: &mut [T], mut key: F)
where
    K: RadixKey,
    F: FnMut(&T) -> K,
{
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
        engine::distribute(values, width, |_: &T| {
            let digit = (keys[next] >> shift) & mask;
            next += 1;
            digit
        });
    } else {
        engine::order_by_cached_indices(values, keys);
    }
}
