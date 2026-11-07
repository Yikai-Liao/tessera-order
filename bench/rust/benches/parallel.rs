// SPDX-License-Identifier: BSD-2-Clause

//! Focused scaling screen for the optional Rayon entry points.

use criterion::{BatchSize, Criterion};
use std::hint::black_box;
use std::time::Duration;
use tessera_sort::{par_stable_sort_by_key, stable_sort_by_key};

const DEFAULT_LEN: usize = 1_000_000;

fn mix(value: u64) -> u64 {
    let mut value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn input(distribution: &str, len: usize) -> Vec<(u64, u32)> {
    let keys: Vec<_> = (0..64).map(mix).collect();
    (0..len)
        .map(|index| {
            let key = match distribution {
                "random" => mix(index as u64),
                "sparse64" => keys[(mix(index as u64) % keys.len() as u64) as usize],
                _ => unreachable!(),
            };
            (key, index as u32)
        })
        .collect()
}

fn suite(criterion: &mut Criterion, len: usize) {
    for distribution in ["random", "sparse64"] {
        let original = input(distribution, len);
        let mut expected = original.clone();
        expected.sort_by_key(|row| row.0);
        let mut serial_check = original.clone();
        stable_sort_by_key(&mut serial_check, |row| row.0);
        assert_eq!(serial_check, expected);

        let mut group = criterion.benchmark_group(format!("rayon/{distribution}/{len}"));
        group
            .warm_up_time(Duration::from_secs(1))
            .measurement_time(Duration::from_secs(3))
            .sample_size(20);

        group.bench_function("serial", |bench| {
            bench.iter_batched_ref(
                || original.clone(),
                |rows| {
                    stable_sort_by_key(black_box(rows.as_mut_slice()), |row| row.0);
                },
                BatchSize::LargeInput,
            );
        });

        for threads in [1, 2, 4, 6] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            let mut check = original.clone();
            pool.install(|| par_stable_sort_by_key(&mut check, |row| row.0));
            assert_eq!(check, expected);

            group.bench_function(format!("rayon-{threads}"), |bench| {
                bench.iter_batched_ref(
                    || original.clone(),
                    |rows| {
                        pool.install(|| {
                            par_stable_sort_by_key(black_box(rows.as_mut_slice()), |row| row.0);
                        });
                    },
                    BatchSize::LargeInput,
                );
            });
        }
        group.finish();
    }
}

fn main() {
    let len = std::env::var("TESSERA_PARALLEL_LEN")
        .map(|value| {
            value
                .parse()
                .expect("TESSERA_PARALLEL_LEN must be an integer")
        })
        .unwrap_or(DEFAULT_LEN);
    let mut criterion = Criterion::default().without_plots().configure_from_args();
    suite(&mut criterion, len);
    criterion.final_summary();
}
