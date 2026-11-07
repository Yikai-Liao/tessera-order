// SPDX-License-Identifier: BSD-2-Clause

use virtual_bucket_prototype::{Config, Row16, VirtualSorter};

fn check(keys: Vec<u64>, config: Config) {
    let mut actual: Vec<_> = keys
        .into_iter()
        .enumerate()
        .map(|(i, key)| Row16 {
            key,
            ordinal: i as u64,
        })
        .collect();
    // Verification storage is deliberately separate and never passed to the kernel.
    let mut expected = actual.clone();
    expected.sort_by_key(|row| row.key);
    let n = actual.len();
    let mut sorter = VirtualSorter::new(&mut actual, config);
    let roots = sorter.scatter_root();
    sorter.verify_directories(&roots);
    let sorted = sorter.sort_adopted_roots(roots);
    sorter.verify_directories(std::slice::from_ref(&sorted));
    let packed = sorter.repack_global(sorted);
    sorter.verify_directories(std::slice::from_ref(&packed));
    assert_eq!(packed.blocks.len(), n.div_ceil(config.block_records));
    let stats = sorter.restore(packed);
    assert_eq!(actual, expected, "config={config:?}");
    let r = 1usize << config.digit_bits;
    assert_eq!(stats.scratch_blocks, (config.root_shards + 3) * r + 8);
    assert!(stats.maximum_live_blocks <= n / config.block_records + stats.scratch_blocks + 1);
    assert_eq!(stats.original_input_bytes, n * 16);
    assert_eq!(
        stats.auxiliary_record_bytes,
        (stats.scratch_blocks * config.block_records + 2 * config.tile_records) * 16
    );
}

fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn config(block_records: usize, digit_bits: u32, tile_records: usize, shards: usize) -> Config {
    Config {
        block_records,
        digit_bits,
        tile_records,
        msd_levels: 2,
        root_shards: shards,
    }
}

#[test]
fn full_stable_reference_matrix() {
    for b in [1, 2, 3, 7, 16, 31] {
        for n in [0, 1, 2, 7, 16, 31, 64, 127, 257, 1025] {
            let mut seed = 0x1234_5678_1234_5678;
            let cases: Vec<Vec<u64>> = vec![
                (0..n).map(|_| random(&mut seed)).collect(),
                (0..n).map(|i| i as u64).collect(),
                (0..n).map(|i| (n - i) as u64).collect(),
                vec![u64::MAX; n],
                vec![0; n],
                (0..n).map(|i| (i % 5) as u64).collect(),
                (0..n)
                    .map(|i| if i % 127 == 0 { u64::MAX } else { i as u64 % 3 })
                    .collect(),
                (0..n).map(|i| u64::MAX - (i % 17) as u64).collect(),
                (0..n)
                    .map(|i| ((i % 7) as u64) << 56 | ((i * 11 % 31) as u64))
                    .collect(),
            ];
            for keys in cases {
                check(keys, config(b, 4, 31, 4));
            }
        }
    }
}

#[test]
fn exhaustive_binary_stability_and_tail_ownership() {
    for n in 0..=9 {
        for mask in 0..1usize << n {
            let keys: Vec<_> = (0..n)
                .map(|i| if mask & (1 << i) == 0 { 0 } else { u64::MAX })
                .collect();
            for b in [1, 3, 7] {
                check(keys.clone(), config(b, 1, 2, 3));
            }
        }
    }
}

#[test]
fn all_radices_full_width_max_and_msd_depths() {
    for bits in [1, 2, 3, 4, 5, 6, 7, 8] {
        for levels in [1, 2] {
            let mut seed = 0x91c3_73af_9831_d7ba;
            let keys: Vec<_> = (0..5003)
                .map(|i| match i % 5 {
                    0 => u64::MAX,
                    1 => 0,
                    2 => random(&mut seed),
                    3 => 0xaaaa_aaaa_0000_0000 | (random(&mut seed) & 65535),
                    _ => i % 13,
                })
                .collect();
            let mut c = config(17, bits, 127, 8);
            c.msd_levels = levels;
            check(keys, c);
        }
    }
}

#[test]
fn oversized_skew_leaves_require_bounded_lsd() {
    let n = 20003;
    let mut seed = 0xb351_7ad9_82df_15ba;
    // The outliers force two high MSD splits; nearly all rows still reach one
    // oversized low subtree. Repeated low digits create old and new LSD tails.
    let keys: Vec<_> = (0..n)
        .map(|i| {
            if i == 0 {
                u64::MAX
            } else if i == 1 {
                1 << 48
            } else {
                random(&mut seed) & ((1u64 << 40) - 1)
            }
        })
        .collect();
    check(keys, config(31, 8, 64, 8));
}

#[test]
fn many_fragmented_adopted_root_tails() {
    let keys: Vec<_> = (0..32771)
        .map(|i| {
            (((i * 37) % 256) as u64) << 56
                | (((i * 17) % 256) as u64) << 48
                | ((i * 101) % 65536) as u64
        })
        .collect();
    check(keys, config(67, 8, 257, 16));
}

#[test]
fn miri_smoke_recycled_input_scratch_and_short_tail() {
    for n in [0, 1, 2, 3, 4, 5, 7, 13] {
        check((0..n).map(|i| (i % 3) as u64).collect(), config(3, 2, 7, 3));
    }
    let keys: Vec<_> = (0..79)
        .map(|i| match i % 7 {
            0 => u64::MAX,
            1 => 0,
            2 => (i % 5) as u64,
            _ => ((i * 37 % 257) as u64) << 32 | (i % 3) as u64,
        })
        .collect();
    check(keys, config(3, 2, 7, 3));
}

#[test]
fn u64_full_stable_reference_cases() {
    let mut seed = 0x73a1_489f_168b_3cd1;
    let n = 4097;
    let cases: Vec<Vec<u64>> = vec![
        (0..n).map(|_| random(&mut seed)).collect(),
        (0..n).map(|i| i as u64).collect(),
        (0..n).map(|i| (n - i) as u64).collect(),
        vec![u64::MAX; n],
        (0..n).map(|i| (i % 5) as u64).collect(),
        (0..n)
            .map(|i| if i % 127 == 0 { u64::MAX } else { i as u64 % 3 })
            .collect(),
    ];
    for mut actual in cases {
        let mut expected = actual.clone();
        expected.sort();
        let stats = virtual_bucket_prototype::sort(&mut actual, config(17, 8, 64, 4));
        assert_eq!(actual, expected);
        assert_eq!(stats.original_input_bytes, n * 8);
    }
}

#[test]
fn miri_smoke_u64_short_tail() {
    let mut actual = vec![u64::MAX, 1, 7, 1, 0, u64::MAX, 3, 9, 3, 7, 0];
    let mut expected = actual.clone();
    expected.sort();
    virtual_bucket_prototype::sort(&mut actual, config(3, 2, 2, 3));
    assert_eq!(actual, expected);
}
