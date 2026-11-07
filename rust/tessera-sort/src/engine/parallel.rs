// SPDX-License-Identifier: BSD-2-Clause

//! Parallel completion of fully materialized, disjoint radix buckets.

use super::*;

// A task needs several tiles to amortize scheduling and independent scratch.
const MIN_TASK_BYTES: usize = 256 * 1024;
// At 512 KiB, the first measured schedule did not consistently beat the
// sequential call; keep at least sixteen tiles before paying for task setup.
const MIN_PARALLEL_BYTES: usize = 1024 * 1024;

/// Selects the serial entry point before adapting a user key to a shared borrow.
#[inline]
pub(crate) fn use_serial<T>(len: usize) -> bool {
    len < 2
        || core::mem::size_of::<T>() == 0
        || len.saturating_mul(core::mem::size_of::<T>()) < MIN_PARALLEL_BYTES
        || rayon::current_num_threads() == 1
}

pub(crate) fn sort<T: Send, E: Extract<T> + Clone + Send, const CACHED: bool>(
    arr: &mut [T],
    mut key: E,
) {
    let record_bytes = core::mem::size_of::<T>();
    let bytes = arr.len().saturating_mul(record_bytes);
    if arr.len() < 2
        || record_bytes == 0
        || bytes < MIN_PARALLEL_BYTES
        || rayon::current_num_threads() == 1
    {
        super::sort::<T, E, CACHED>(arr, &mut key, None);
        return;
    }
    let tile_bytes = TILE_BYTES
        .max(record_bytes)
        .min(MAX_TILE_RECORDS.saturating_mul(record_bytes));
    let Some(varying) = prepare_sort(arr, &mut key, tile_bytes) else {
        return;
    };
    // Keep 128-bit keys on the existing 64-bit kernels when only one half varies.
    if E::Value::BITS > 64 {
        if varying >> 64 == 0 {
            sort_prepared::<T, _, CACHED>(arr, Narrow::<E, 0>(key), varying, tile_bytes);
            return;
        }
        if varying & ((1u128 << 64) - 1) == 0 {
            sort_prepared::<T, _, CACHED>(arr, Narrow::<E, 64>(key), varying >> 64, tile_bytes);
            return;
        }
    }
    sort_prepared::<T, E, CACHED>(arr, key, varying, tile_bytes);
}

#[derive(Clone)]
struct Narrow<E, const SHIFT: usize>(E);
impl<T, E: Extract<T>, const SHIFT: usize> Extract<T> for Narrow<E, SHIFT> {
    type Value = u64;
    #[inline]
    fn key(&mut self, record: &T) -> u64 {
        (self.0.key(record).to_u128() >> SHIFT) as u64
    }
}

fn sort_prepared<T: Send, E: Extract<T> + Clone + Send, const CACHED: bool>(
    arr: &mut [T],
    mut key: E,
    varying: u128,
    tile_bytes: usize,
) {
    let bytes = arr.len().saturating_mul(core::mem::size_of::<T>());
    let mut workspace = Workspace::new();
    let low = varying.trailing_zeros() as usize;
    let top = bit_len(varying);
    // A single digit has no unfinished children. Preserve the adaptive serial
    // equality and outlier routes until there is useful independent work.
    let mut keys = [KeyValue::<T, E>::from_usize(0); MODE_SAMPLE_LEN];
    sample_keys(arr, &mut keys, &mut key);
    let equal = sample_mode(&mut keys);
    let bit = KeyValue::<T, E>::from_usize(1) << KeyValue::<T, E>::from_usize(top - 1);
    if top - low <= SPLIT_BITS
        || equal.is_some()
        || outlier_run(arr, &keys, bit, &mut key).is_some()
    {
        Sorter::<T, E, CACHED> {
            workspace: &mut workspace,
            key: &mut key,
            tile_bytes,
        }
        .sort_node(arr, varying);
        return;
    }
    let digit = choose_split_digit(arr, top, tile_bytes, &mut key);
    let candidate = varying & !digit.mask::<E::Value>().to_u128();
    let ends = block::partition_digit_with_ends(
        arr,
        digit.width,
        digit.shift,
        &mut key,
        workspace.block_buffers(),
        &mut |_, _| {},
    );
    // Partitioning has restored every live record before any task starts.
    // Moving workspaces is safe; their cached pointers are never dereferenced
    // between partition calls. There is one workspace per bounded worker, not
    // one per bucket or Rayon job.
    let workers = rayon::current_num_threads()
        .min((bytes / MIN_TASK_BYTES).max(1))
        .min(ends.len());
    let mut workspaces: Vec<_> = (1..workers).map(|_| Workspace::new()).collect();
    workspaces.push(workspace);
    finish_buckets::<T, E, CACHED>(arr, &ends, 0, candidate, key, &mut workspaces, tile_bytes);
}

fn finish_buckets<T: Send, E: Extract<T> + Clone + Send, const CACHED: bool>(
    arr: &mut [T],
    ends: &[usize],
    offset: usize,
    candidate: u128,
    mut key: E,
    workspaces: &mut [Workspace<T>],
    tile_bytes: usize,
) {
    if arr.len() < 2 || candidate == 0 {
        return;
    }
    if workspaces.len() == 1 || ends.len() == 1 {
        let mut start = 0;
        for &end in ends {
            let end = end - offset;
            let child = &mut arr[start..end];
            if child.len() > 1 {
                let varying = scan_varying_bits(child, candidate, &mut key);
                Sorter::<T, E, CACHED> {
                    workspace: &mut workspaces[0],
                    key: &mut key,
                    tile_bytes,
                }
                .sort_node(child, varying);
            }
            start = end;
        }
        return;
    }
    // Split only at a bucket boundary. Equal-key records can therefore never
    // cross tasks, and slice splitting proves that tasks cannot alias records.
    let middle = offset + arr.len() / 2;
    let split = ends
        .partition_point(|&end| end < middle)
        .clamp(1, ends.len() - 1);
    let boundary = ends[split - 1];
    let records = arr.len();
    let (left, right) = arr.split_at_mut(boundary - offset);
    let left_workers = (((workspaces.len() as u128 * left.len() as u128) / records as u128)
        as usize)
        .clamp(1, workspaces.len() - 1);
    let (left_workspaces, right_workspaces) = workspaces.split_at_mut(left_workers);
    let left_key = key.clone();
    rayon::join(
        || {
            finish_buckets::<T, E, CACHED>(
                left,
                &ends[..split],
                offset,
                candidate,
                left_key,
                left_workspaces,
                tile_bytes,
            )
        },
        || {
            finish_buckets::<T, E, CACHED>(
                right,
                &ends[split..],
                boundary,
                candidate,
                key,
                right_workspaces,
                tile_bytes,
            )
        },
    );
}

/// Keeps the serial adaptive checks without changing the sequential entry point.
fn prepare_sort<T, E: Extract<T>>(arr: &mut [T], key: &mut E, tile_bytes: usize) -> Option<u128> {
    let record_bytes = core::mem::size_of::<T>();
    let sampled = probe_order(arr, key);
    match sampled {
        Order::Ascending => return None,
        Order::Descending => {
            reverse_stably(arr, key);
            return None;
        }
        Order::Flat | Order::Unsorted => {}
    }
    if sampled == Order::Unsorted {
        let probe = (tile_bytes / record_bytes).max(2).min(arr.len());
        let swaps = if probe == arr.len() {
            key.insertion_sort(arr, probe)
        } else {
            // No retained workspace is exposed by the parallel API, so these
            // Copy keys can use a typed temporary without raw scratch casts.
            let mut keys: Vec<_> = arr[..probe].iter().map(|record| key.key(record)).collect();
            match Direct.insertion_sort(&mut keys, probe) {
                Some(0) => None,
                Some(swaps) => key.insertion_sort(arr, swaps * arr.len() / probe),
                None => None,
            }
        };
        if swaps.is_some() {
            return None;
        }
    }
    let varying = block::varying_key_bits(arr, key);
    if varying == 0 {
        return None;
    }
    if sampled == Order::Flat {
        match confirm_order(arr, key) {
            Order::Ascending => return None,
            Order::Descending => {
                reverse_stably(arr, key);
                return None;
            }
            _ => {}
        }
    }
    Some(varying)
}
