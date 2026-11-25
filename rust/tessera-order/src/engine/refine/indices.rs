// SPDX-License-Identifier: BSD-2-Clause

//! Stable permutations for index output and cached-key record movement.

use super::{Requirement, Sorter, bit_len, prepare, spans::SourceOrder, tile_capacity};
use crate::engine::{
    key::{BitWidth, ByKey, RadixKey, Unsigned},
    thresholds::TILE_BYTES,
    workspace::Workspace,
};
use alloc::vec::Vec;

const BANDS: usize = 1 << u8::BITS;

pub(crate) fn argsort<T, K: RadixKey>(values: &[T], key: impl FnMut(&T) -> K) -> Vec<usize> {
    let mut indices = Vec::new();
    if let Some(order) = order_indices(values, key, size_of::<usize>(), &mut indices) {
        order.write_indices(&mut indices);
    }
    indices
}

pub(crate) fn order_by_cached_indices<T, V: Unsigned>(values: &mut [T], keys: Vec<V>) {
    let mut indices = Vec::new();
    let spans = order_indices(&keys, |v| *v, size_of::<T>(), &mut indices);
    drop(keys);
    if let Some(order) = spans {
        drop(indices);
        order.apply(values);
    } else {
        apply_cycles(values, indices);
    }
}

/// Fill `indices` with the stable permutation, or return source spans that
/// the caller can expand in the same buffer or apply directly to its records.
fn order_indices<T, K: RadixKey>(
    values: &[T],
    mut key: impl FnMut(&T) -> K,
    record_bytes: usize,
    indices: &mut Vec<usize>,
) -> Option<SourceOrder> {
    let len = values.len();
    indices.clear();
    indices.extend(0..len);
    if len < 2 {
        return None;
    }
    let tile_len = tile_capacity::<usize>(TILE_BYTES);
    let mut extract = ByKey(|i: &usize| key(&values[*i]));
    let varying = prepare::<true, _, _>(indices, K::Width::MASK, &mut extract, tile_len)?;
    let mut scratch = Workspace::new();
    if len <= tile_len {
        scratch.tile.sort(indices, varying, &mut extract);
        return None;
    }
    if let Some(order) = SourceOrder::for_node(
        values,
        varying,
        Requirement::All,
        &mut ByKey(&mut key),
        record_bytes,
    ) {
        return Some(order);
    }
    let low = varying.trailing_zeros() as usize;
    let digit_shift = bit_len(varying).saturating_sub(u8::BITS as usize).max(low);
    let lower = varying & ((1u128 << digit_shift) - 1);
    let index_bits = bit_len((len - 1) as u128);
    let index_mask = usize::MAX >> (usize::BITS as usize - index_bits);
    // When a high-byte band averages at most one entry per next-byte value,
    // finish its short index list from source keys. Denser bands amortize
    // packing another prefix and decoding its ties.
    let prefix_bits = if len.div_ceil(BANDS) <= BANDS {
        0
    } else {
        index_bits.min(usize::BITS as usize - index_bits)
    };
    let prefix_shift = bit_len(lower).saturating_sub(prefix_bits).max(low);
    let prefix_mask = lower >> prefix_shift;
    // Each band has the same high digit. Use the spare bits of its index
    // words for the next key prefix, preserving source order on ties.
    let encoded = |r: &T| key(r).encode().to_u128();
    let ends = scatter_indices(values, indices, encoded, digit_shift, |key, source| {
        if prefix_mask == 0 {
            source
        } else {
            ((key >> prefix_shift) as usize) << index_bits | source
        }
    });
    if lower == 0 {
        return None;
    }
    let remaining = lower & ((1u128 << prefix_shift) - 1);
    let mut extract = ByKey(|i: &usize| key(&values[*i]));
    let mut start = 0;
    for end in ends {
        if end == start {
            continue;
        }
        let band = &mut indices[start..end];
        start = end;
        if prefix_mask != 0 {
            Sorter {
                scratch: &mut scratch,
                extractor: &mut ByKey(|word: &usize| *word >> index_bits),
                tile_len,
            }
            .sort_child(band, prefix_mask, Requirement::All);
        }
        let mut sorter = Sorter {
            scratch: &mut scratch,
            extractor: &mut extract,
            tile_len,
        };
        if prefix_mask == 0 {
            sorter.sort_child(band, remaining, Requirement::All);
        } else {
            for ties in band.chunk_by_mut(|a, b| *a >> index_bits == *b >> index_bits) {
                for word in &mut *ties {
                    *word &= index_mask;
                }
                sorter.sort_child(ties, remaining, Requirement::All);
            }
        }
    }
    None
}

/// Emit words into stable high-byte bands. Changed extractor results may
/// invalidate the counts, so verify every cursor before exposing the order.
fn scatter_indices<T>(
    records: &[T],
    words: &mut [usize],
    mut key: impl FnMut(&T) -> u128,
    shift: usize,
    pack: impl Fn(u128, usize) -> usize,
) -> [usize; BANDS] {
    let band = |key: u128| usize::from((key >> shift) as u8);
    let mut ends = [0; BANDS];
    for record in records {
        ends[band(key(record))] += 1;
    }
    let mut start = 0;
    let mut next = ends;
    for (cursor, end) in next.iter_mut().zip(&mut ends) {
        *cursor = start;
        start += *end;
        *end = start;
    }
    for (source, record) in records.iter().enumerate() {
        let encoded = key(record);
        let slot = &mut next[band(encoded)];
        words[*slot] = pack(encoded, source);
        *slot += 1;
    }
    assert!(next == ends, "inconsistent radix partition");
    ends
}

/// Mark finished positions in the permutation so the cycles take linear time together.
fn apply_cycles<T>(values: &mut [T], mut indices: Vec<usize>) {
    for start in 0..values.len() {
        let mut position = start;
        loop {
            let source = core::mem::replace(&mut indices[position], position);
            if source == start {
                break;
            }
            values.swap(position, source);
            position = source;
        }
    }
}
