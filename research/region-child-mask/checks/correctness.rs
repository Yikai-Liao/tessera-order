// SPDX-License-Identifier: BSD-2-Clause

mod fixtures;
#[cfg(feature = "child-scan-diagnostics")]
use control::{ChildMode, Record};
use control::{Config, Row16, Row128};
use fixtures::*;

#[test]
fn child_modes_preserve_payloads() {
    for bits in [1, 8, 12] {
        let candidate = u64::MAX >> bits;
        let prefix = ((1u64 << bits) - 1) << (64 - bits);
        for len in [
            0, 1, 2, 3, 31, 32, 33, 64, 65, 127, 128, 129, 1024, 1025, 1026, 4097, 8193,
        ] {
            for pattern in 0..8 {
                let input: Vec<_> = keys(len, pattern)
                    .into_iter()
                    .map(|key| prefix | (key & candidate))
                    .collect();
                check_child::<u64>(&input, candidate as u128);
                check_child::<Row16>(&input, candidate as u128);
                check_child::<Row128>(&input, candidate as u128);
            }
        }
    }
}

#[test]
fn root_tails_preserve_payloads() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(3)
        .build()
        .unwrap();
    pool.install(|| {
        for parallel in [false, true] {
            for bits in [1, 8, 12] {
                for (len, block, shards) in [
                    (0, 7, 3),
                    (1, 1, 1),
                    (2, 3, 3),
                    (31, 7, 3),
                    (32, 7, 3),
                    (33, 7, 3),
                    (65, 7, 3),
                    (129, 7, 3),
                    (1025, 7, 3),
                ] {
                    for pattern in 0..8 {
                        let input = keys(len, pattern);
                        let config = Config {
                            shards,
                            radix_bits: bits,
                            shift: 64 - bits,
                            block_size: Some(block),
                            parallel,
                            parallel_restore: true,
                            path_restore: true,
                            broker_restore: false,
                            capture_plan: false,
                        };
                        check_root::<u64>(&input, config);
                        check_root::<Row16>(&input, config);
                        check_root::<Row128>(&input, config);
                    }
                }
            }
        }
    });
}

#[test]
fn exact_masks_handle_late_variation() {
    let mut input: Vec<_> = (0..4097)
        .map(|index| 0xa700_0000_0000_0000 | if index % 2 == 0 { 1 } else { 0 })
        .collect();
    input[4096] |= 1 << 20;
    let candidate = ((1u64 << 20) | 1) as u128;
    check_child::<u64>(&input, candidate);
    check_child::<Row16>(&input, candidate);
    check_child::<Row128>(&input, candidate);
    #[cfg(feature = "child-scan-diagnostics")]
    {
        let mut actual = records::<Row16>(&input);
        Row16::sort_child_control(
            &mut tessera::Workspace::new(),
            &mut actual,
            ChildMode::Masked,
            candidate,
        );
        let report = tessera::child_mask::diagnostics::take_reports()
            .pop()
            .unwrap();
        assert_eq!(report.scan_keys, Some(input.len()));
        assert!(!report.scan_early_return);
    }
    for len in [0, 1, 2, 1026, 4097] {
        let equal = vec![0xa700_0000_0000_0042; len];
        check_child::<u64>(&equal, 0);
        check_child::<Row16>(&equal, 0);
        check_child::<Row128>(&equal, 0);
    }
}

#[test]
fn mask_saturates_at_public_boundary() {
    let candidate = u64::MAX >> 8;
    let input: Vec<_> = keys(4097, 0)
        .into_iter()
        .map(|key| 0x9600_0000_0000_0000 | (key & candidate))
        .collect();
    check_child::<u64>(&input, candidate as u128);
    check_child::<Row16>(&input, candidate as u128);
    check_child::<Row128>(&input, candidate as u128);
    #[cfg(feature = "child-scan-diagnostics")]
    {
        for mode in MODES {
            let mut actual = records::<Row16>(&input);
            Row16::sort_child_control(
                &mut tessera::Workspace::new(),
                &mut actual,
                mode,
                candidate as u128,
            );
            let report = tessera::child_mask::diagnostics::take_reports()
                .pop()
                .unwrap();
            assert_eq!(
                report.scan_keys,
                Some(if mode == ChildMode::Masked {
                    1025
                } else {
                    input.len()
                })
            );
            assert_eq!(report.scan_early_return, mode == ChildMode::Masked);
        }
    }
}

#[test]
fn restored_children_reach_scanner() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(3)
        .build()
        .unwrap();
    pool.install(|| {
        let config = Config {
            shards: 3,
            radix_bits: 1,
            shift: 63,
            path_restore: true,
            parallel_restore: true,
            ..Config::default()
        };
        let input = keys(16_387, 0);
        check_root::<u64>(&input, config);
        check_root::<Row16>(&input, config);
        check_root::<Row128>(&input, config);
    });
}
