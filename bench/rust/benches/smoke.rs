// SPDX-License-Identifier: BSD-2-Clause

//! Smoke suite for continuous benchmarking on Bencher Bare Metal.
//!
//! Benchmark IDs are `rust/<case_id>/<alloc_mode>`, where `case_id` is
//! `type/distribution/length/seed` as in `docs/development/benchmarking.md`.
//! Runners in other languages use the same case IDs under their own prefix.

use criterion::{BatchSize, Criterion};
use std::hint::black_box;
use std::time::Duration;
use tessera_bench::case::{Key, Row, generate};
use tessera_sort::Workspace;
use tessera_sort::thresholds::{INSERTION_MAX, TILE_BYTES};

/// Elements prepared per Criterion batch, so the cache footprint of a batch
/// is the same for every length and does not depend on measured timings.
const BATCH_ELEMENTS: usize = 1 << 18;

const MID: usize = 65_536;
const LARGE: usize = 1_000_000;
const HUGE: usize = 10_000_000;

fn suite(c: &mut Criterion) {
    // Insertion limit, one past it, and one past the tile capacity for 8- and
    // 16-byte records. Cases this short move with code placement, so only the
    // boundaries themselves are kept.
    keys::<u64>(c, "random", INSERTION_MAX, false);
    for len in [INSERTION_MAX + 1, TILE_BYTES / 8 + 1] {
        keys::<u64>(c, "random", len, true);
        rows::<u32>(c, "random", len, false);
    }
    rows::<u64>(c, "random", TILE_BYTES / 16 + 1, true);

    // Eight tiles: one block-partition level and every root and node route.
    keys::<u64>(c, "random", MID, true);
    for distribution in [
        // Root shortcuts: confirmed order, stable reversal, budgeted
        // insertion, constant keys.
        "sorted",
        "reverse",
        "sorted99",
        "equal",
        // At most 8 varying bits: one block partition.
        "d16",
        // A repeated key: the equality partition.
        "zipf2",
        "skew99",
        // A short end run separated by the top bit: the outlier run.
        "sorted_tail",
        // Mixed ascending and descending runs, as from merged sources: the
        // insertion probe must give up early.
        "runs",
    ] {
        keys::<u64>(c, distribution, MID, false);
    }
    keys::<u32>(c, "random", MID, false);
    keys::<u32>(c, "sorted99", MID, false);
    keys::<u128>(c, "random", MID, false);
    keys::<f64>(c, "random", MID, false);
    keys::<f64>(c, "sorted", MID, false);
    keys::<f64>(c, "sorted99", MID, false);
    rows::<u64>(c, "random", MID, true);
    rows::<u64>(c, "d16", MID, false);
    // A 128-byte record: its byte size, not its key, sets the tile capacity
    // and the cursor-table cap for records wider than a machine word.
    records(c, "random", MID, true);

    // The size at which most engine decisions were measured.
    for distribution in ["random", "zipf2", "skew99", "two_values", "sorted99"] {
        keys::<u64>(c, distribution, LARGE, false);
    }
    keys::<u32>(c, "random", LARGE, false);
    keys::<u128>(c, "random", LARGE, false);
    // Eight keys in 256 top-digit buckets: digit widening.
    keys::<u128>(c, "sparse8", LARGE, false);
    rows::<u64>(c, "random", LARGE, false);

    // Ten million records leave the caches and go through the block partition
    // on every level, which is where the block size and the scratch working set
    // decide. Random is the worst case for routing and Zipf the common skew.
    keys::<u64>(c, "random", HUGE, false);
    keys::<u64>(c, "zipf2", HUGE, false);
}

fn keys<K: Key>(c: &mut Criterion, distribution: &str, len: usize, workspace: bool) {
    let input = generate::<K>(distribution, len);
    let mut expected = input.clone();
    expected.sort_by(K::order);
    let id = format!("rust/{}/{distribution}/{len}/s1", K::NAME);
    measure(
        c,
        &id,
        "api",
        &input,
        &expected,
        K::same,
        tessera_sort::stable_sort,
    );
    if workspace {
        let mut ws = Workspace::new();
        measure(c, &id, "workspace", &input, &expected, K::same, |v| {
            ws.stable_sort(v)
        });
    }
}

fn rows<K: Key>(c: &mut Criterion, distribution: &str, len: usize, workspace: bool) {
    let input: Vec<Row<K>> = generate::<K>(distribution, len)
        .into_iter()
        .zip(0..)
        .map(|(key, row)| Row { key, row })
        .collect();
    let mut expected = input.clone();
    expected.sort_by(|a, b| a.key.order(&b.key));
    let same = |a: &Row<K>, b: &Row<K>| a.key.same(&b.key) && a.row == b.row;
    let id = format!("rust/row_{}/{distribution}/{len}/s1", K::NAME);
    let api = |v: &mut [Row<K>]| tessera_sort::stable_sort_by_key(v, |r| r.key);
    measure(c, &id, "api", &input, &expected, same, api);
    if workspace {
        let mut ws = Workspace::new();
        let prepared = |v: &mut [Row<K>]| ws.stable_sort_by_key(v, |r| r.key);
        measure(c, &id, "workspace", &input, &expected, same, prepared);
    }
}

/// A record of 128 bytes: a word key and a payload that carries the record's
/// original index.
#[derive(Clone, Copy)]
struct Wide {
    key: u64,
    payload: [u8; 120],
}

impl Wide {
    fn new(key: u64, index: usize) -> Self {
        let mut payload = [0u8; 120];
        payload[..8].copy_from_slice(&(index as u64).to_le_bytes());
        Self { key, payload }
    }
}

/// Times records much wider than their key, so record size rather than key
/// size decides the tile capacity and the block size.
fn records(c: &mut Criterion, distribution: &str, len: usize, workspace: bool) {
    let input: Vec<Wide> = generate::<u64>(distribution, len)
        .into_iter()
        .enumerate()
        .map(|(index, key)| Wide::new(key, index))
        .collect();
    let mut expected = input.clone();
    expected.sort_by_key(|record| record.key);
    let same = |a: &Wide, b: &Wide| a.key == b.key && a.payload == b.payload;
    let id = format!("rust/rec128_u64/{distribution}/{len}/s1");
    let api = |v: &mut [Wide]| tessera_sort::stable_sort_by_key(v, |r| r.key);
    measure(c, &id, "api", &input, &expected, same, api);
    if workspace {
        let mut ws = Workspace::new();
        let prepared = |v: &mut [Wide]| ws.stable_sort_by_key(v, |r| r.key);
        measure(c, &id, "workspace", &input, &expected, same, prepared);
    }
}

/// Checks one entry point against the stable reference, then times it. The
/// check also warms a prepared workspace.
fn measure<T: Copy>(
    c: &mut Criterion,
    id: &str,
    mode: &str,
    input: &[T],
    expected: &[T],
    same: impl Fn(&T, &T) -> bool,
    mut sort: impl FnMut(&mut [T]),
) {
    let mut values = input.to_vec();
    sort(&mut values);
    let ok = values.iter().zip(expected).all(|(a, b)| same(a, b));
    assert!(ok, "{id}/{mode}: output differs from the reference");

    let batch = (BATCH_ELEMENTS / input.len()).max(1) as u64;
    let mut group = c.benchmark_group(id);
    // A sort of 10^6 keys takes milliseconds, so a 1.5 s window gives it only
    // a few hundred sorts; it gets more time and samples to narrow its limits.
    let (warm_up, window, samples) = if input.len() >= HUGE {
        // One sort takes about a fifth of a second, so 40 samples would need
        // eight; 20 fill the same window and the spread per iteration is small.
        (1000, 4000, 20)
    } else if input.len() >= LARGE {
        (1000, 4000, 40)
    } else {
        (500, 1500, 30)
    };
    group
        .warm_up_time(Duration::from_millis(warm_up))
        .measurement_time(Duration::from_millis(window))
        .sample_size(samples);
    group.bench_function(mode, |b| {
        // Restoring the input and dropping buffers are not timed.
        b.iter_batched_ref(
            || input.to_vec(),
            |values| sort(black_box(values.as_mut_slice())),
            BatchSize::NumIterations(batch),
        );
    });
    group.finish();
}

fn main() {
    let mut criterion = Criterion::default().without_plots().configure_from_args();
    suite(&mut criterion);
    criterion.final_summary();
}
