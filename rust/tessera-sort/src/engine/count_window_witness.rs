// SPDX-License-Identifier: BSD-2-Clause

//! Observe the real sampling helpers without duplicating their index formulas.

use super::*;
use crate::key::ByKey;

pub(crate) fn inspected(records: &[u64]) -> (Vec<usize>, Vec<usize>, Vec<u64>, usize) {
    let indexed: Vec<_> = records.iter().copied().enumerate().collect();
    let mut order_indices = Vec::new();
    let order = probe_order(
        &indexed,
        &mut ByKey(|r: &(usize, u64)| {
            order_indices.push(r.0);
            r.1
        }),
    );
    assert!(matches!(order, Order::Unsorted));
    let mut mode_indices = Vec::new();
    let mut sampled = vec![0u64; MODE_SAMPLE_LEN];
    sample_keys(
        &indexed,
        &mut sampled,
        &mut ByKey(|r: &(usize, u64)| {
            mode_indices.push(r.0);
            r.1
        }),
    );
    let record_bytes = core::mem::size_of::<u64>();
    let probe_len = TILE_BYTES
        .max(record_bytes)
        .min(MAX_TILE_RECORDS.saturating_mul(record_bytes))
        / record_bytes;
    (order_indices, mode_indices, sampled, probe_len)
}
