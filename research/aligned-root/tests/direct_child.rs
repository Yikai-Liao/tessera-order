// SPDX-License-Identifier: BSD-2-Clause
//! Native Rayon integration coverage for the direct-u64 child route.
#![cfg(not(miri))]
use tessera_aligned_root_prototype::{Config, sort};
fn next(s: &mut u64) -> u64 {
    *s = s.wrapping_add(0x9e3779b97f4a7c15);
    let mut x = *s;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
#[test]
fn direct_u64_children_match_full_reference() {
    for workers in [1, 4, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        for n in [0, 1, 17, 4097, 20003] {
            for duplicate_keys in [false, true] {
                let mut state = 7;
                let mut values: Vec<_> = (0..n)
                    .map(|_| {
                        let key = next(&mut state);
                        if duplicate_keys { key % 1024 } else { key }
                    })
                    .collect();
                let mut expected = values.clone();
                expected.sort();
                pool.install(|| {
                    sort(
                        &mut values,
                        Config {
                            shards: workers,
                            block_size: Some(31),
                            parallel_restore: true,
                            ..Config::default()
                        },
                    )
                });
                assert_eq!(values, expected);
            }
        }
    }
}
