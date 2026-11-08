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

/// Length below one tile where ordered inputs take the order-probe routes.
const SHORT: usize = 1_000;
/// Length spanning several tiles in the continuous suite.
const MID: usize = 65_536;
/// Length used for measured engine decisions.
const LARGE: usize = 1_000_000;
/// Length exercising the out-of-cache block partition.
const HUGE: usize = 10_000_000;

/// Registers the continuous suite's fixed case IDs and allocation modes.
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

    // Below one tile, an ordered input is confirmed or reversed before any
    // tile routing, so this pair isolates the order probe and the reversal
    // from the routes the cases above measure. Both are a screen only: code
    // placement moves short cases, and an alert needs the protocol's controls.
    for distribution in ["sorted", "reverse"] {
        keys::<u64>(c, distribution, SHORT, false);
    }

    // PR #40: the low-fanout tile route benefits short, heavily skewed inputs.
    // Keep a random control for both direct keys and stable keyed rows.
    for distribution in ["random", "skew99"] {
        keys::<u64>(c, distribution, SHORT, false);
        rows::<u64>(c, distribution, SHORT, false);
    }

    // Eight tiles: one block-partition level and every root and node route.
    keys::<u64>(c, "random", MID, true);
    for distribution in [
        // Root shortcuts: confirmed order, stable reversal, budgeted
        // insertion, constant keys.
        "sorted",
        "reverse",
        "sorted99",
        "equal",
        // One pair of far-apart positions swapped per hundred records: the
        // probe must decline before it moves them.
        "far_swaps",
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
    // Measured magnitudes rather than arbitrary bits: both signs, magnitudes
    // within 2^-23..2^7, no NaN or infinity.
    keys::<f64>(c, "finite_signed", MID, false);
    rows::<u64>(c, "random", MID, true);
    rows::<u64>(c, "d16", MID, false);
    // Half the records repeat one key and the rest are distinct: the equality
    // partition must keep their payload order stable.
    rows::<u64>(c, "hot50", MID, false);
    // A 128-byte record: its byte size, not its key, sets the tile capacity
    // and the cursor-table cap for records wider than a machine word.
    records(c, "random", MID, true);
    // The sparse digits reach every byte of a record much wider than the
    // cursor table entry that moves it.
    records(c, "sparse_digits", MID, false);

    // The size at which most engine decisions were measured.
    for distribution in ["random", "zipf2", "skew99", "two_values", "sorted99"] {
        keys::<u64>(c, distribution, LARGE, false);
    }
    // High-cardinality inputs that are not uniform, where the split sample
    // decides the digit width: four random bits per byte never fill a sampled
    // window, two clusters a machine word apart leave the top window with two
    // values, and half a node repeating one key reaches the equality partition.
    for distribution in ["sparse_digits", "bimodal", "hot50", "d4096"] {
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

/// Measures a scalar-key case with the public API and optional workspace.
fn keys<K: Key>(c: &mut Criterion, distribution: &str, len: usize, workspace: bool) {
    let id = format!("rust/{}/{distribution}/{len}/s1", K::NAME);
    if !experiment_group(&id, workspace) {
        return;
    }
    let input = generate::<K>(distribution, len);
    let mut expected = input.clone();
    expected.sort_by(K::order);
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

/// Measures a keyed-row case while checking stable payload order.
fn rows<K: Key>(c: &mut Criterion, distribution: &str, len: usize, workspace: bool) {
    let id = format!("rust/row_{}/{distribution}/{len}/s1", K::NAME);
    if !experiment_group(&id, workspace) {
        return;
    }
    let input: Vec<Row<K>> = generate::<K>(distribution, len)
        .into_iter()
        .zip(0..)
        .map(|(key, row)| Row { key, row })
        .collect();
    let mut expected = input.clone();
    expected.sort_by(|a, b| a.key.order(&b.key));
    let same = |a: &Row<K>, b: &Row<K>| a.key.same(&b.key) && a.row == b.row;
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
    /// Creates a wide record carrying its original index.
    fn new(key: u64, index: usize) -> Self {
        let mut payload = [0u8; 120];
        payload[..8].copy_from_slice(&(index as u64).to_le_bytes());
        Self { key, payload }
    }
}

/// Times records much wider than their key, so record size rather than key
/// size decides the tile capacity and the block size.
fn records(c: &mut Criterion, distribution: &str, len: usize, workspace: bool) {
    let id = format!("rust/rec128_u64/{distribution}/{len}/s1");
    if !experiment_group(&id, workspace) {
        return;
    }
    let input: Vec<Wide> = generate::<u64>(distribution, len)
        .into_iter()
        .enumerate()
        .map(|(index, key)| Wide::new(key, index))
        .collect();
    let mut expected = input.clone();
    expected.sort_by_key(|record| record.key);
    let same = |a: &Wide, b: &Wide| a.key == b.key && a.payload == b.payload;
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

/// Selects whole groups before input generation for the isolated experiment.
fn experiment_group(id: &str, workspace: bool) -> bool {
    if std::env::var_os("TESSERA_EXPERIMENT_LIST").is_some() {
        println!("{id}/api");
        if workspace {
            println!("{id}/workspace");
        }
        return false;
    }
    std::env::var("TESSERA_EXPERIMENT_GROUPS")
        .map_or(true, |groups| groups.lines().any(|group| group == id))
}

/// Runs the Criterion suite with command-line case selection.
fn main() {
    let mut criterion = Criterion::default().without_plots().configure_from_args();
    suite(&mut criterion);
    criterion.final_summary();
}
