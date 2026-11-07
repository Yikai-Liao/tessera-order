// SPDX-License-Identifier: BSD-2-Clause

//! Smoke suite for continuous benchmarking on Bencher Bare Metal.
//!
//! Benchmark IDs are `rust/<case_id>/<alloc_mode>`, where `case_id` is
//! `type/distribution/length/seed` as in `docs/development/benchmarking.md`.
//! Runners in other languages use the same case IDs under their own prefix.

use criterion::{BatchSize, Criterion};
use std::cmp::Ordering;
use std::hint::black_box;
use std::time::Duration;
use tessera_sort::thresholds::{INSERTION_MAX, TILE_BYTES};
use tessera_sort::{RadixKey, Workspace};

/// Elements prepared per Criterion batch, so the cache footprint of a batch
/// is the same for every length and does not depend on measured timings.
const BATCH_ELEMENTS: usize = 1 << 18;

const MID: usize = 65_536;
const LARGE: usize = 1_000_000;

fn suite(c: &mut Criterion) {
    // Insertion limit, one past it, and one past the tile capacity.
    for len in [INSERTION_MAX, INSERTION_MAX + 1, TILE_BYTES / 8 + 1] {
        let workspace = len > INSERTION_MAX;
        keys::<u64>(c, "random", len, workspace);
        rows::<u32>(c, "random", len, false);
    }
    rows::<u64>(c, "random", INSERTION_MAX + 1, false);
    rows::<u64>(c, "random", TILE_BYTES / 16 + 1, true);

    // Eight tiles: one block-partition level and every root shortcut.
    keys::<u64>(c, "random", MID, true);
    for distribution in ["d16", "zipf2", "sorted", "reverse", "sorted99", "equal"] {
        keys::<u64>(c, distribution, MID, false);
    }
    keys::<u32>(c, "random", MID, false);
    keys::<u32>(c, "sorted99", MID, false);
    keys::<u128>(c, "random", MID, false);
    keys::<f64>(c, "random", MID, false);
    keys::<f64>(c, "sorted99", MID, false);
    rows::<u64>(c, "random", MID, true);
    rows::<u64>(c, "d16", MID, false);

    // The size at which most engine decisions were measured.
    for distribution in ["random", "zipf2", "sorted99"] {
        keys::<u64>(c, distribution, LARGE, false);
    }
    keys::<u32>(c, "random", LARGE, false);
    keys::<u128>(c, "random", LARGE, false);
    rows::<u64>(c, "random", LARGE, false);
}

/// A key type with a name for case IDs and an exact equality for validation.
trait Key: RadixKey + Copy {
    const NAME: &'static str;
    /// Draws from the type's whole bit range.
    fn draw(rng: &mut SplitMix64) -> Self;
    /// Converts a value below `2^32` exactly.
    fn small(value: u64) -> Self;
    fn order(&self, other: &Self) -> Ordering;
    fn same(&self, other: &Self) -> bool;
}

macro_rules! int_key {
    ($t:ty, $draw:expr) => {
        impl Key for $t {
            const NAME: &'static str = stringify!($t);
            fn draw(rng: &mut SplitMix64) -> Self {
                $draw(rng)
            }
            fn small(value: u64) -> Self {
                value as $t
            }
            fn order(&self, other: &Self) -> Ordering {
                self.cmp(other)
            }
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        }
    };
}

int_key!(u32, |rng: &mut SplitMix64| rng.next() as u32);
int_key!(u64, |rng: &mut SplitMix64| rng.next());
int_key!(u128, |rng: &mut SplitMix64| {
    (u128::from(rng.next()) << 64) | u128::from(rng.next())
});

impl Key for f64 {
    const NAME: &'static str = "f64";
    // Arbitrary bit patterns include NaNs, infinities and subnormals.
    fn draw(rng: &mut SplitMix64) -> Self {
        f64::from_bits(rng.next())
    }
    fn small(value: u64) -> Self {
        value as f64
    }
    fn order(&self, other: &Self) -> Ordering {
        self.total_cmp(other)
    }
    fn same(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

/// A row: a key and the record's original index, so validation also checks
/// that records are permuted and that equal keys keep their order.
#[derive(Clone, Copy)]
struct Row<K> {
    key: K,
    row: u32,
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// Generates seed-1 inputs with integer arithmetic only.
fn generate<K: Key>(distribution: &str, len: usize) -> Vec<K> {
    let mut rng = SplitMix64(1);
    let n = len as u64;
    match distribution {
        "random" => (0..len).map(|_| K::draw(&mut rng)).collect(),
        "d16" => (0..len).map(|_| K::small(rng.next() % 16)).collect(),
        "zipf2" => {
            // Zipf with s = 2 over ranks 0..len: rank k has weight
            // floor(2^62 / (k + 1)^2). Rank 0 holds about 61% of the keys.
            let mut total = 0;
            let cdf: Vec<u64> = (1..=n)
                .map(|k| {
                    total += (1 << 62) / (k * k);
                    total
                })
                .collect();
            (0..len)
                .map(|_| {
                    let r = ((u128::from(rng.next()) * u128::from(total)) >> 64) as u64;
                    K::small(cdf.partition_point(|&c| c <= r) as u64)
                })
                .collect()
        }
        "sorted" => (0..n).map(K::small).collect(),
        "reverse" => (0..n).rev().map(K::small).collect(),
        "sorted99" => {
            // Ascending, with len / 100 evenly spaced adjacent swaps.
            let mut values: Vec<u64> = (0..n).collect();
            let step = (len / (len / 100).max(1)).max(2);
            for i in (0..len.saturating_sub(1)).step_by(step) {
                values.swap(i, i + 1);
            }
            values.into_iter().map(K::small).collect()
        }
        "equal" => vec![K::small(1); len],
        _ => unreachable!("unknown distribution {distribution}"),
    }
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
    c.bench_function(&format!("{id}/{mode}"), |b| {
        // Restoring the input and dropping buffers are not timed.
        b.iter_batched_ref(
            || input.to_vec(),
            |values| sort(black_box(values.as_mut_slice())),
            BatchSize::NumIterations(batch),
        );
    });
}

fn main() {
    let mut criterion = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_millis(250))
        .measurement_time(Duration::from_millis(750))
        .without_plots()
        .configure_from_args();
    suite(&mut criterion);
    criterion.final_summary();
}
