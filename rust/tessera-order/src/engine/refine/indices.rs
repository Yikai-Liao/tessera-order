// SPDX-License-Identifier: BSD-2-Clause

//! Resolve the stable permutation of immutable keys. Contiguous source spans
//! can move a payload intact. Dense inputs keep source positions and their
//! remaining key prefixes together in index-sized words.

use super::{
    Requirement, Sorter, bands::Bands, bit_len, prepare, spans::SourceOrder, tile_capacity,
};
use crate::engine::{
    key::{ByKey, RadixKey, Unsigned},
    thresholds::TILE_BYTES,
    workspace::Workspace,
};
use alloc::vec::Vec;

enum IndexOrder {
    Spans(SourceOrder),
    Dense(Vec<usize>),
}

pub(crate) fn argsort<T, K: RadixKey>(values: &[T], key: impl FnMut(&T) -> K) -> Vec<usize> {
    match index_order(values, key, size_of::<usize>()) {
        IndexOrder::Spans(order) => order.into_indices(),
        IndexOrder::Dense(indices) => indices,
    }
}

pub(crate) fn order_by_cached_indices<T, V: Unsigned>(values: &mut [T], keys: Vec<V>) {
    let order = index_order(&keys, |v| *v, size_of::<T>());
    drop(keys);
    match order {
        IndexOrder::Spans(order) => order.apply(values),
        IndexOrder::Dense(indices) => apply_cycles(values, indices),
    }
}

fn index_order<T, K: RadixKey>(
    values: &[T],
    mut key: impl FnMut(&T) -> K,
    record_bytes: usize,
) -> IndexOrder {
    let len = values.len();
    let mut indices: Vec<_> = (0..len).collect();
    if len < 2 {
        return IndexOrder::Dense(indices);
    }
    let tile_len = tile_capacity::<usize>(TILE_BYTES);
    let mut extract = ByKey(|i: &usize| key(&values[*i]));
    let Some(varying) = prepare(&mut indices, &mut extract, tile_len) else {
        return IndexOrder::Dense(indices);
    };
    let mut scratch = Workspace::new();
    if len <= tile_len {
        scratch.tile.sort(&mut indices, varying, &mut extract);
        return IndexOrder::Dense(indices);
    }
    if let Some(order) = SourceOrder::for_node(
        values,
        varying,
        Requirement::All,
        &mut ByKey(&mut key),
        record_bytes,
    ) {
        return IndexOrder::Spans(order);
    }
    let digit = Bands::window(varying);
    let partition = Bands::count(values.iter().map(|r| digit.byte(key(r).encode())));
    let lower = varying & ((1u128 << digit.shift) - 1);
    let low = varying.trailing_zeros() as usize;
    let index_bits = bit_len((len - 1) as u128);
    let index_mask = usize::MAX >> (usize::BITS as usize - index_bits);
    // Drop the digit shared by this band. Store as many remaining key bits as
    // fit above a source index, so ordering the band reads compact words.
    let prefix_shift = bit_len(lower)
        .saturating_sub(usize::BITS as usize - index_bits)
        .max(low);
    partition.emit(
        &mut indices,
        values.iter().enumerate().map(|(source, value)| {
            let encoded = key(value).encode();
            let word = if lower == 0 {
                source
            } else {
                ((encoded.to_u128() >> prefix_shift) as usize) << index_bits | source
            };
            (digit.byte(encoded), word)
        }),
    );
    if lower == 0 {
        return IndexOrder::Dense(indices);
    }
    let remaining = lower & ((1u128 << prefix_shift) - 1);
    let mut extract = ByKey(|i: &usize| key(&values[*i]));
    for range in partition.ranges() {
        let band = &mut indices[range];
        let mut prefix = ByKey(|word: &usize| *word >> index_bits);
        if band.len() <= tile_len {
            scratch
                .tile
                .finish_child(band, lower >> prefix_shift, &mut prefix);
        } else {
            Sorter {
                scratch: &mut scratch,
                extractor: &mut prefix,
                tile_len,
            }
            .sort_child(band, lower >> prefix_shift, Requirement::All);
        }
        if remaining == 0 {
            for word in band {
                *word &= index_mask;
            }
            continue;
        }
        for ties in band.chunk_by_mut(|a, b| *a >> index_bits == *b >> index_bits) {
            for word in &mut *ties {
                *word &= index_mask;
            }
            if ties.len() > 1 {
                Sorter {
                    scratch: &mut scratch,
                    extractor: &mut extract,
                    tile_len,
                }
                .sort_child(ties, remaining, Requirement::All);
            }
        }
    }
    IndexOrder::Dense(indices)
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
