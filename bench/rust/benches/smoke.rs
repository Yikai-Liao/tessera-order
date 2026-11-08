// SPDX-License-Identifier: BSD-2-Clause

//! Smoke suite for continuous benchmarking on Bencher Bare Metal.
//!
//! Benchmark IDs are `rust/<case_id>/<mode>`, where `case_id` is
//! `type/distribution/length/seed` as in `docs/development/benchmarking.md`.
//! Runners in other languages use the same case IDs under their own prefix.

use criterion::{BatchSize, Criterion};
use std::hint::black_box;
use std::sync::OnceLock;
use std::time::Duration;
use tessera_bench::case::{Key, Row, generate};
use tessera_bench::plan::Plan;

/// Elements prepared per Criterion batch, so the cache footprint of a batch
/// is the same for every length and does not depend on measured timings.
const BATCH_ELEMENTS: usize = 1 << 18;

/// Length used for measured engine decisions.
const LARGE: usize = 1_000_000;
/// Length exercising the out-of-cache block partition.
const HUGE: usize = 10_000_000;

/// Expands the experiment before generating any inputs.
fn suite(c: &mut Criterion) {
    for case in plan().inputs().expect("nonempty benchmark plan") {
        let distribution = &case.distribution;
        let len = case.len;
        match case.ty.as_str() {
            "u32" => keys::<u32>(c, distribution, len),
            "u64" => keys::<u64>(c, distribution, len),
            "u128" => keys::<u128>(c, distribution, len),
            "f64" => keys::<f64>(c, distribution, len),
            "row_u32" => rows::<u32>(c, distribution, len),
            "row_u64" => rows::<u64>(c, distribution, len),
            "rec128_u64" => records(c, distribution, len),
            _ => unreachable!("validated record type"),
        }
    }
}

/// Measures a scalar-key case with the public API.
fn keys<K: Key>(c: &mut Criterion, distribution: &str, len: usize) {
    let id = format!("rust/{}/{distribution}/{len}/s1", K::NAME);
    if !selected(&id) {
        return;
    }
    let input = generate::<K>(distribution, len);
    let mut expected = input.clone();
    expected.sort_by(K::order);
    measure(
        c,
        &id,
        &input,
        &expected,
        K::same,
        tessera_sort::stable_sort,
    );
}

/// Measures a keyed-row case while checking stable payload order.
fn rows<K: Key>(c: &mut Criterion, distribution: &str, len: usize) {
    let id = format!("rust/row_{}/{distribution}/{len}/s1", K::NAME);
    if !selected(&id) {
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
    measure(c, &id, &input, &expected, same, |values| {
        tessera_sort::stable_sort_by_key(values, |record| record.key);
    });
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
fn records(c: &mut Criterion, distribution: &str, len: usize) {
    let id = format!("rust/rec128_u64/{distribution}/{len}/s1");
    if !selected(&id) {
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
    measure(c, &id, &input, &expected, same, |values| {
        tessera_sort::stable_sort_by_key(values, |record| record.key);
    });
}

/// Checks a sort against the stable reference, then times it.
fn measure<T: Copy>(
    c: &mut Criterion,
    id: &str,
    input: &[T],
    expected: &[T],
    same: impl Fn(&T, &T) -> bool,
    mut sort: impl FnMut(&mut [T]),
) {
    let mut values = input.to_vec();
    sort(&mut values);
    let ok = values.iter().zip(expected).all(|(a, b)| same(a, b));
    assert!(ok, "{id}/api: output differs from the reference");

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
    group.bench_function("api", |b| {
        // Restoring the input and dropping buffers are not timed.
        b.iter_batched_ref(
            || input.to_vec(),
            |values| sort(black_box(values.as_mut_slice())),
            BatchSize::NumIterations(batch),
        );
    });
    group.finish();
}

/// Reads the compiled continuous preset unless a local caller supplies a plan.
fn plan() -> &'static Plan {
    static PLAN: OnceLock<Plan> = OnceLock::new();
    PLAN.get_or_init(|| {
        let text = std::env::var("TESSERA_BENCH_PLAN").map_or_else(
            |_| include_str!("../../plans/continuous.toml").to_owned(),
            |path| std::fs::read_to_string(path).expect("read benchmark plan"),
        );
        Plan::parse(&text, "smoke").expect("valid smoke benchmark plan")
    })
}

/// Selects groups before generation so idle workers create no memory traffic.
fn selected(id: &str) -> bool {
    if std::env::var_os("TESSERA_BENCH_LIST").is_some() {
        println!("{id}/api");
        return false;
    }
    std::env::var("TESSERA_BENCH_GROUPS")
        .map_or(true, |groups| groups.lines().any(|group| group == id))
}

/// Runs the Criterion suite with command-line case selection.
fn main() {
    let mut criterion = Criterion::default().without_plots().configure_from_args();
    suite(&mut criterion);
    criterion.final_summary();
}
