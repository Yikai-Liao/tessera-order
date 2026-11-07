// SPDX-License-Identifier: BSD-2-Clause

#[cfg(test)]
use control::Config;
use control::{ChildMode, Record, Row16, Row128};
use std::fmt::Debug;

pub const MODES: [ChildMode; 3] = [ChildMode::Public, ChildMode::Full, ChildMode::Masked];

pub fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

pub fn keys(len: usize, pattern: usize) -> Vec<u64> {
    let mut state = 19;
    let mut result: Vec<_> = (0..len)
        .map(|index| {
            let random = next(&mut state);
            match pattern {
                0..=2 => random,
                3 => 7,
                4 => (random % 17) * (u64::MAX / 16),
                5 => random & 0x8100_0040_0008_0001,
                6 => (random & 0xfff) << 24,
                7 => {
                    if index + 1 == len {
                        1 << 53
                    } else {
                        random & 0xffff
                    }
                }
                _ => unreachable!(),
            }
        })
        .collect();
    if pattern == 1 || pattern == 2 {
        result.sort();
    }
    if pattern == 2 {
        result.reverse();
    }
    result
}

pub trait Fixture: Record + Eq + Debug {
    fn from_key(key: u64, id: u64) -> Self;
}
impl Fixture for u64 {
    fn from_key(key: u64, _: u64) -> Self {
        key
    }
}
impl Fixture for Row16 {
    fn from_key(key: u64, id: u64) -> Self {
        Self { key, id }
    }
}
impl Fixture for Row128 {
    fn from_key(key: u64, id: u64) -> Self {
        Self {
            key,
            id,
            payload: core::array::from_fn(|lane| {
                id.wrapping_mul(0x9e3779b97f4a7c15) ^ (lane as u64).rotate_left(17) ^ key
            }),
        }
    }
}

pub fn records<T: Fixture>(keys: &[u64]) -> Vec<T> {
    keys.iter()
        .enumerate()
        .map(|(id, &key)| T::from_key(key, id as u64))
        .collect()
}

#[cfg(all(test, feature = "child-scan-diagnostics"))]
fn take_reports() -> Vec<tessera::child_mask::diagnostics::ChildReport> {
    let mut reports = tessera::child_mask::diagnostics::take_reports();
    reports.sort_by_key(|report| (report.fixed_bits, report.len));
    reports
}

/// Compares actual preparation and downstream entry across all modes.
#[cfg(all(test, feature = "child-scan-diagnostics"))]
fn equivalent_reports(
    expected: &[tessera::child_mask::diagnostics::ChildReport],
    actual: &[tessera::child_mask::diagnostics::ChildReport],
) {
    assert_eq!(expected.len(), actual.len());
    for (before, after) in expected.iter().zip(actual) {
        let mut normalized = after.clone();
        normalized.mode = before.mode;
        normalized.scan_keys = before.scan_keys;
        normalized.scan_early_return = before.scan_early_return;
        assert_eq!(*before, normalized);
        assert_eq!(before.scan_keys.is_some(), after.scan_keys.is_some());
        if let (Some(full), Some(examined)) = (before.scan_keys, after.scan_keys) {
            assert!(examined <= full);
            assert_eq!(
                full, before.len,
                "a valid high-root child cannot saturate u64::MAX"
            );
            if after.mode == ChildMode::Full {
                assert_eq!(examined, full);
            }
        }
    }
}

#[cfg(test)]
pub fn check_child<T: Fixture>(keys: &[u64], candidate: u128) {
    let input = records::<T>(keys);
    let mut expected = input.clone();
    expected.sort_by_key(|record| record.key());
    #[cfg(feature = "child-scan-diagnostics")]
    let mut baseline = Vec::new();
    for mode in MODES {
        let mut actual = input.clone();
        T::sort_child_control(&mut tessera::Workspace::new(), &mut actual, mode, candidate);
        assert_eq!(actual, expected);
        #[cfg(feature = "child-scan-diagnostics")]
        {
            let reports = take_reports();
            assert_eq!(reports.len(), 1);
            if mode == ChildMode::Public {
                baseline = reports;
            } else {
                equivalent_reports(&baseline, &reports);
            }
        }
    }
}

#[cfg(test)]
pub fn check_root<T: Fixture>(keys: &[u64], config: Config) {
    let input = records::<T>(keys);
    let mut expected = input.clone();
    expected.sort_by_key(|record| record.key());
    #[cfg(feature = "child-scan-diagnostics")]
    let mut baseline = Vec::new();
    for mode in MODES {
        let mut actual = input.clone();
        control::sort_with_mode(&mut actual, config, mode);
        assert_eq!(actual, expected);
        #[cfg(feature = "child-scan-diagnostics")]
        {
            let reports = take_reports();
            assert_eq!(reports.len(), 1 << config.radix_bits);
            assert_eq!(
                reports.iter().map(|report| report.len).sum::<usize>(),
                input.len()
            );
            let candidate = (u64::MAX >> config.radix_bits) as u128;
            assert!(reports.iter().all(|report| report.candidate == candidate));
            if mode == ChildMode::Public {
                baseline = reports;
            } else {
                equivalent_reports(&baseline, &reports);
            }
        }
    }
}
