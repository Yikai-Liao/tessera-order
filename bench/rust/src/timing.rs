// SPDX-License-Identifier: BSD-2-Clause

//! Opt-in timing controls for local protocol experiments.

/// Divides warm-up, measurement and sample budgets, retaining Criterion's ten-sample minimum.
///
/// Durations are milliseconds, rounded upward. The minimum sample count can make
/// slow cases exceed the requested window; the runner records actual wall time.
///
/// # Panics
///
/// Panics unless `divisor` is one or six.
///
/// # Examples
///
/// ```
/// use tessera_bench::timing::divide_timing;
/// assert_eq!(divide_timing((500, 1500, 30), 6), (84, 250, 10));
/// ```
pub fn divide_timing(
    (warm_up_ms, measurement_ms, samples): (u64, u64, usize),
    divisor: usize,
) -> (u64, u64, usize) {
    assert!(
        matches!(divisor, 1 | 6),
        "timing divisor must be one or six"
    );
    (
        warm_up_ms.div_ceil(divisor as u64),
        measurement_ms.div_ceil(divisor as u64),
        samples.div_ceil(divisor).max(10),
    )
}

#[cfg(test)]
mod tests {
    use super::divide_timing;

    #[test]
    fn full_windows_preserve_every_preset() {
        for preset in [(500, 1500, 30), (1000, 4000, 40), (1000, 4000, 20)] {
            assert_eq!(divide_timing(preset, 1), preset);
        }
    }

    #[test]
    fn divided_windows_round_up_and_keep_the_sample_minimum() {
        assert_eq!(divide_timing((500, 1500, 30), 6), (84, 250, 10));
        assert_eq!(divide_timing((1000, 4000, 40), 6), (167, 667, 10));
        assert_eq!(divide_timing((1000, 4000, 20), 6), (167, 667, 10));
    }

    #[test]
    #[should_panic(expected = "timing divisor")]
    fn unsupported_divisor_is_rejected() {
        divide_timing((500, 1500, 30), 0);
    }
}
