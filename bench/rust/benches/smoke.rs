// SPDX-License-Identifier: BSD-2-Clause

//! Continuous Criterion measurements: serial API and exclusive four-thread API.
//!
//! IDs are `rust/<case_id>/<mode>`; input selection precedes generation.

use criterion::{BatchSize, Criterion};
use std::{hint::black_box, sync::OnceLock, time::Duration};
use tessera_bench::{
    case::{Key, Row, generate},
    plan::{InputCase, Plan, criterion_group, criterion_id},
};

/// Fixed prepared element count, independent of measured execution time.
const BATCH_ELEMENTS: usize = 1 << 18;

/// Expands both API selections without multiplying their coverage.
fn suite(c: &mut Criterion) {
    for case in plan().inputs().expect("nonempty benchmark plan") {
        measure_case(c, &case, "api");
    }
    if parallel_enabled() {
        for case in plan().parallel_inputs().expect("valid parallel selection") {
            measure_case(c, &case, "par-4");
        }
    }
}

/// Dispatches an input/API pair before allocating records or pool workers.
fn measure_case(c: &mut Criterion, case: &InputCase, mode: &str) {
    let distribution = &case.distribution;
    let len = case.len;
    match case.ty.as_str() {
        "u32" => keys::<u32>(c, distribution, len, mode),
        "u64" => keys::<u64>(c, distribution, len, mode),
        "u128" => keys::<u128>(c, distribution, len, mode),
        "f64" => keys::<f64>(c, distribution, len, mode),
        "row_u32" => rows::<u32>(c, distribution, len, mode),
        "row_u64" => rows::<u64>(c, distribution, len, mode),
        "rec128_u64" => records(c, distribution, len, mode),
        _ => unreachable!("validated record type"),
    }
}

/// Old trusted supervisors retain serial-only coverage until explicitly upgraded.
fn parallel_enabled() -> bool {
    match std::env::var("TESSERA_BENCH_PARALLEL").as_deref() {
        Ok("4") => cfg!(feature = "rayon"),
        Err(std::env::VarError::NotPresent) => false,
        _ => panic!("TESSERA_BENCH_PARALLEL must be 4"),
    }
}

/// Measures keys against the same stable total-order reference for both APIs.
fn keys<K: Key + Send + Sync>(c: &mut Criterion, distribution: &str, len: usize, mode: &str) {
    let id = criterion_group!("{}", K::NAME; distribution, len);
    if !selected(&id, mode) {
        return;
    }
    let input = generate::<K>(distribution, len);
    let mut expected = input.clone();
    expected.sort_by(K::order);
    if mode == "api" {
        measure(
            c,
            &id,
            mode,
            &input,
            &expected,
            K::same,
            tessera_sort::stable_sort,
        );
    } else {
        #[cfg(feature = "rayon")]
        measure_parallel(
            c,
            &id,
            &input,
            &expected,
            K::same,
            tessera_sort::par_stable_sort,
        );
    }
}

/// Measures keyed rows, including stable payload order.
fn rows<K: Key + Send + Sync>(c: &mut Criterion, distribution: &str, len: usize, mode: &str) {
    let id = criterion_group!("row_{}", K::NAME; distribution, len);
    if !selected(&id, mode) {
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
    if mode == "api" {
        measure(c, &id, mode, &input, &expected, same, |values| {
            tessera_sort::stable_sort_by_key(values, |record| record.key);
        });
    } else {
        #[cfg(feature = "rayon")]
        measure_parallel(c, &id, &input, &expected, same, |values| {
            tessera_sort::par_stable_sort_by_key(values, |record| record.key);
        });
    }
}

/// A wide record whose payload carries its original index.
#[derive(Clone, Copy)]
struct Wide {
    key: u64,
    payload: [u8; 120],
}

impl Wide {
    /// Creates a wide record with a stable identity.
    fn new(key: u64, index: usize) -> Self {
        let mut payload = [0u8; 120];
        payload[..8].copy_from_slice(&(index as u64).to_le_bytes());
        Self { key, payload }
    }
}

/// Measures records much wider than their keys.
fn records(c: &mut Criterion, distribution: &str, len: usize, mode: &str) {
    let id = criterion_group!("rec128_u64"; distribution, len);
    if !selected(&id, mode) {
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
    if mode == "api" {
        measure(c, &id, mode, &input, &expected, same, |values| {
            tessera_sort::stable_sort_by_key(values, |record| record.key);
        });
    } else {
        #[cfg(feature = "rayon")]
        measure_parallel(c, &id, &input, &expected, same, |values| {
            tessera_sort::par_stable_sort_by_key(values, |record| record.key);
        });
    }
}

/// Warms one pool before verification and joins it before the next case prepares.
#[cfg(feature = "rayon")]
fn measure_parallel<T: Copy + Send>(
    c: &mut Criterion,
    id: &str,
    input: &[T],
    expected: &[T],
    same: impl Fn(&T, &T) -> bool,
    sort: impl Fn(&mut [T]) + Sync,
) {
    // Criterion uses test mode unless --bench is present without --test.
    // Build verification may oversubscribe CPUs, but measured runs may not.
    let args = std::env::args_os().collect::<Vec<_>>();
    let test_mode =
        !args.iter().any(|arg| arg == "--bench") || args.iter().any(|arg| arg == "--test");
    if !test_mode {
        assert!(
            std::thread::available_parallelism().unwrap().get() >= 4,
            "four allowed CPUs are required for par-4"
        );
    }
    let pool = tessera_bench::pool::BenchmarkPool::new(4);
    measure(c, id, "par-4", input, expected, same, |values| {
        pool.install(|| sort(values))
    });
}

/// Verifies a complete sort, then times calls without input restoration or teardown.
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
    assert!(
        values.iter().zip(expected).all(|(a, b)| same(a, b)),
        "{id}/{mode}: output differs from the reference"
    );
    #[cfg(feature = "layout-study")]
    if let Ok(count) = std::env::var("TESSERA_BENCH_FIXED_SORTS") {
        let count: usize = count.parse().expect("integer fixed sort count");
        assert!(
            (1..=1024).contains(&count),
            "fixed sort count must be 1..1024"
        );
        let mut buffers = vec![input.to_vec(); count];
        // Callgrind toggles collection at this stable symbol. Preparation,
        // restoration and verification stay outside its dynamic call tree.
        tessera_counted_sorts(&mut || {
            for values in &mut buffers {
                sort(black_box(values.as_mut_slice()));
            }
        });
        assert!(
            buffers
                .iter()
                .all(|values| { values.iter().zip(expected).all(|(a, b)| same(a, b)) })
        );
        println!(
            "{}",
            serde_json::json!({ "id": criterion_id!(id, mode), "sorts": count })
        );
        return;
    }
    let batch = (BATCH_ELEMENTS / input.len()).max(1) as u64;
    let mut group = c.benchmark_group(id);
    let settings = tessera_bench::plan::revision_sampling(input.len());
    if std::env::var_os("TESSERA_BENCH_PAIRED").is_some() {
        tessera_bench::sampling::serve(
            &criterion_id!(id, mode),
            input,
            expected,
            same,
            sort,
            settings,
        );
        return;
    }
    group
        .warm_up_time(Duration::from_millis(settings.warm_ms))
        .measurement_time(Duration::from_millis(settings.window_ms))
        .sample_size(settings.samples);
    group.bench_function(mode, |b| {
        b.iter_batched_ref(
            || input.to_vec(),
            |values| sort(black_box(values.as_mut_slice())),
            BatchSize::NumIterations(batch),
        );
    });
    group.finish();
}

/// Marks one prepared batch for Callgrind's function-scoped collection.
#[cfg(feature = "layout-study")]
#[inline(never)]
#[unsafe(no_mangle)]
fn tessera_counted_sorts(run: &mut dyn FnMut()) {
    run();
    black_box(());
}

/// Reads the compiled suite unless a local reproduction supplies a saved plan.
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

/// Selects exact timed IDs; legacy group selection supports the pre-upgrade tracker.
fn selected(id: &str, mode: &str) -> bool {
    let full = criterion_id!(id, mode);
    if std::env::var_os("TESSERA_BENCH_LIST").is_some() {
        println!("{full}");
        return false;
    }
    if let Ok(ids) = std::env::var("TESSERA_BENCH_IDS") {
        return ids.lines().any(|selected| selected == full);
    }
    std::env::var("TESSERA_BENCH_GROUPS")
        .map_or(true, |groups| groups.lines().any(|group| group == id))
}

/// Runs Criterion with its standard command-line controls.
fn main() {
    if std::env::var_os("TESSERA_BENCH_PAIRED").is_some() {
        let mut request = String::new();
        std::io::stdin()
            .read_line(&mut request)
            .expect("read start request");
        assert_eq!(request.trim(), "start");
    }
    let mut criterion = Criterion::default().without_plots().configure_from_args();
    suite(&mut criterion);
    criterion.final_summary();
}
