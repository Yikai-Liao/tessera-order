// SPDX-License-Identifier: BSD-2-Clause

//! Stable permutations for index output and cached-key record movement.

use super::{
    Order, Requirement, Sorter, bit_len, insertion_probe, prepare_with, probe_insertion_budget,
    reverse_stably, spans::SourceOrder, tile_capacity,
};
use crate::engine::{
    key::{ByKey, Direct, Extract, Unsigned},
    thresholds::TILE_BYTES,
    workspace::Workspace,
};
use alloc::vec::Vec;

const BANDS: usize = 1 << u8::BITS;

pub(crate) fn argsort<T, E: Extract<T>>(values: &[T], extractor: E) -> Vec<usize> {
    let mut indices = Vec::new();
    if let Some(order) = order_indices(values, extractor, size_of::<usize>(), None, &mut indices) {
        order.write_indices(&mut indices);
    }
    indices
}

pub(crate) fn order_by_cached_indices<T, V: Unsigned>(
    values: &mut [T],
    keys: Vec<V>,
    varying: u128,
) {
    let mut indices = Vec::new();
    let spans = order_indices(&keys, Direct, size_of::<T>(), Some(varying), &mut indices);
    drop(keys);
    if let Some(order) = spans {
        drop(indices);
        order.apply(values);
    } else {
        apply_cycles(values, indices);
    }
}

fn fill_ordered<T, E: Extract<T>>(
    values: &[T],
    order: Order,
    extractor: &mut E,
    indices: &mut Vec<usize>,
) -> bool {
    match order {
        Order::Ascending => indices.extend(0..values.len()),
        Order::Descending { strict: true } => indices.extend((0..values.len()).rev()),
        Order::Descending { strict: false } => {
            indices.extend(0..values.len());
            reverse_stably(indices, &mut ByKey(|i: &usize| extractor.key(&values[*i])));
        }
        Order::Flat | Order::Unsorted => return false,
    }
    true
}

/// Probes budgeted insertion, reusing `indices` on tile-sized slices and
/// deferring `0..len` materialization on long slices until the swap rate fits.
fn probe_insertion<T, E: Extract<T>>(
    values: &[T],
    extractor: &mut E,
    tile_len: usize,
    indices: &mut Vec<usize>,
) -> bool {
    let len = values.len();
    if len <= tile_len {
        indices.extend(0..len);
        let mut extract = ByKey(|i: &usize| extractor.key(&values[*i]));
        return insertion_probe(indices, &mut extract, tile_len);
    }
    let Some(budget) = probe_insertion_budget(values, extractor, tile_len) else {
        return false;
    };
    indices.extend(0..len);
    ByKey(|i: &usize| extractor.key(&values[*i]))
        .insertion_sort(indices, budget)
        .is_some()
}

/// Fill `indices` with the stable permutation, or return source spans that
/// the caller can expand or apply directly to its records.
fn order_indices<T, E: Extract<T>>(
    values: &[T],
    mut extractor: E,
    record_bytes: usize,
    known_varying: Option<u128>,
    indices: &mut Vec<usize>,
) -> Option<SourceOrder> {
    let len = values.len();
    indices.clear();
    // Reject unrepresentable index outputs (possible for zero-sized `T`) before
    // extracting any keys, without eagerly allocating for valid lengths.
    assert!(
        len <= isize::MAX as usize / size_of::<usize>(),
        "capacity overflow"
    );
    if len < 2 {
        indices.extend(0..len);
        return None;
    }
    let tile_len = tile_capacity::<usize>(TILE_BYTES);
    // Prepare on `values` directly: order probing, ascending/strictly-descending
    // confirmation and varying-bit scans read source keys directly, while
    // budgeted insertion and descending tie repair materialize `0..len` on
    // demand (if whole-slice insertion exhausts its budget, `argsort` reuses
    // that buffer and cached-key sparse fallback drops it before moving records).
    let Some(varying) = prepare_with(
        values,
        &mut extractor,
        known_varying,
        indices,
        |v, order, e, idx| fill_ordered(v, order, e, idx),
        |v, e, idx| probe_insertion(v, e, tile_len, idx),
    ) else {
        if indices.is_empty() {
            indices.extend(0..len);
        }
        return None;
    };
    let mut scratch = Workspace::new();
    if len <= tile_len {
        if indices.is_empty() {
            indices.extend(0..len);
        }
        scratch.tile.sort(
            indices,
            varying,
            &mut ByKey(|i: &usize| extractor.key(&values[*i])),
        );
        return None;
    }
    if let Some(order) = SourceOrder::for_node(
        values,
        varying,
        Requirement::All,
        &mut extractor,
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
    indices.resize(len, 0);
    let ends = scatter_indices(
        values,
        indices,
        |r| extractor.key(r).to_u128(),
        digit_shift,
        |key, source| {
            if prefix_mask == 0 {
                source
            } else {
                ((key >> prefix_shift) as usize) << index_bits | source
            }
        },
    );
    if lower == 0 {
        return None;
    }
    let remaining = lower & ((1u128 << prefix_shift) - 1);
    let mut extract = ByKey(|i: &usize| extractor.key(&values[*i]));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn oversized_zst_argsort_rejects_before_key_calls() {
        let oversized_len = isize::MAX as usize / size_of::<usize>() + 1;
        // SAFETY: `()` is zero-sized, so any valid non-null aligned pointer
        // forms a valid slice of any `len <= isize::MAX` (0 total bytes).
        let oversized: &[()] = unsafe {
            core::slice::from_raw_parts(core::ptr::NonNull::dangling().as_ptr(), oversized_len)
        };
        let mut calls = 0usize;
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            let _ = argsort(
                oversized,
                ByKey(|_: &()| {
                    calls += 1;
                    0u64
                }),
            );
        }))
        .is_err();
        assert!(panicked);
        assert_eq!(calls, 0);

        let small = [(); 5];
        assert_eq!(
            argsort(&small, ByKey(|_: &()| 7u64)),
            alloc::vec![0, 1, 2, 3, 4]
        );
    }
}
