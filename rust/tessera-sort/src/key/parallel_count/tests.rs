// SPDX-License-Identifier: BSD-2-Clause

use super::*;
use crate::key::{ByKey, Direct, Extract};

fn data(n: usize, low: usize, width: usize, base: u64, skew: bool) -> Vec<u64> {
    let mask = ((1u64 << width) - 1) << low;
    (0..n)
        .map(|i| {
            let mixed = (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let digit = if skew && i % 101 != 0 { 0 } else { mixed >> 32 };
            (base & !mask) | ((digit << low) & mask)
        })
        .collect()
}

#[test]
fn scalar_count_prefix_rank_fill() {
    // Run the exact scalar kernels under Miri without executing Rayon.
    for width in 1..=8 {
        for low in [0, 17, 64 - width] {
            let input = data(517, low, width, u64::MAX, true);
            let mut expected = input.clone();
            expected.sort();
            let buckets = 1 << width;
            let mut counts = vec![0; 3 * buckets];
            for (row, chunk) in counts.chunks_mut(buckets).zip(input.chunks(173)) {
                count(chunk, row, low);
            }
            let mut ends = vec![0; buckets + 1];
            prefix(&counts, buckets, &mut ends);
            assert_eq!(ends[buckets], input.len());
            for chunk_size in [1, 19, 173, 257, 517] {
                let mut actual = input.clone();
                let base = input[0] & !(((1 << width) - 1) << low);
                for (index, chunk) in actual.chunks_mut(chunk_size).enumerate() {
                    fill(chunk, index * chunk_size, &ends, base, low);
                }
                assert_eq!(actual, expected);
            }
        }
    }
}

#[test]
fn checked_budget_rejects_overflow_and_small_inputs() {
    for width in [0, 9, usize::MAX] {
        assert!(plan(1 << 20, 8, width, 4).is_none());
    }
    assert!(plan(usize::MAX, usize::MAX, 8, 4).is_none());
    assert!(plan(usize::MAX, usize::MAX / MIN_CHUNK, 8, 8).is_none());
    assert!(plan(usize::MAX, 2, 8, usize::MAX).is_none());
    assert!(plan(0, 2, 8, 4).is_none());
    assert!(plan(1 << 20, 1, 8, 4).is_none());
    assert!(plan(32768, 8, 8, 4).is_none()); // Scratch, not chunk limit.
    assert!(plan(65536, 17, 8, 4).is_none()); // Excess workers.
    for workers in [2usize, 3, 4, 8, 64, 1024] {
        let n = (workers * MIN_CHUNK).max(65536);
        assert!(plan(n, workers, 8, 4).is_some());
    }
}

#[cfg(not(miri))]
#[test]
fn intrinsic_window_matrix_and_fallback() {
    for workers in [1, 2, 3, 4, 8] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                for width in 1..=8 {
                    for low in [0, 11, 64 - width] {
                        for base in [0, u64::MAX] {
                            let input = data(65539, low, width, base, width % 2 == 0);
                            let mut expected = input.clone();
                            expected.sort();
                            let mut actual = input.clone();
                            let used = Direct.try_parallel_count_window(&mut actual, low, width);
                            assert_eq!(used, workers > 1);
                            if used {
                                assert_eq!(actual, expected);
                            } else {
                                assert_eq!(actual, input);
                            }
                            if low + width <= 32 {
                                let mut actual: Vec<_> = input.iter().map(|&v| v as u32).collect();
                                let mut expected = actual.clone();
                                expected.sort();
                                let used =
                                    Direct.try_parallel_count_window(&mut actual, low, width);
                                assert_eq!(used, workers > 1);
                                if used {
                                    assert_eq!(actual, expected);
                                }
                            }
                        }
                    }
                }
                for n in [0, 1, 8191, 8192, 32767, 32768, 32769, 65535, 65536, 65537] {
                    let mut actual: Vec<_> = data(n, 24, 8, u64::MAX, false)
                        .into_iter()
                        .map(|v| v as u32)
                        .collect();
                    let mut expected = actual.clone();
                    expected.sort();
                    let used = Direct.try_parallel_count_window(&mut actual, 24, 8);
                    assert_eq!(used, plan(n, workers, 8, 4).is_some());
                    if !used {
                        assert!(Direct.try_count_window(&mut actual, 24, 8));
                    }
                    assert_eq!(actual, expected);
                }
            });
    }
}

#[cfg(not(miri))]
#[test]
fn public_route_witness_and_fastpaths() {
    use std::sync::{Arc, atomic::Ordering::SeqCst};
    use witness::{Hooks, with_hooks};
    for workers in [1, 2, 3, 4, 8] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                for n in [262143, 262144, 262145] {
                    for width in 1..=8 {
                        for low in [0, 32 - width] {
                            let mut actual: Vec<_> = data(n, low, width, u64::MAX, false)
                                .into_iter()
                                .map(|v| v as u32)
                                .collect();
                            let mut expected = actual.clone();
                            expected.sort();
                            let hooks = Arc::new(Hooks::default());
                            with_hooks(hooks.clone(), || crate::par_stable_sort(&mut actual));
                            assert_eq!(actual, expected);
                            assert_eq!(
                                hooks.parallel_count.load(SeqCst),
                                workers > 1 && n >= 262144
                            );
                        }
                    }
                }
                for n in [131071, 131072, 131073] {
                    let mut actual = data(n, 56, 8, u64::MAX, true);
                    let mut expected = actual.clone();
                    expected.sort();
                    let hooks = Arc::new(Hooks::default());
                    with_hooks(hooks.clone(), || crate::par_stable_sort(&mut actual));
                    assert_eq!(actual, expected);
                    assert_eq!(
                        hooks.parallel_count.load(SeqCst),
                        workers > 1 && n >= 131072
                    );
                }
                for kind in 0..4 {
                    let mut actual = match kind {
                        0 => vec![u64::MAX; 131073],
                        1 => (0..131073).map(|i| i as u64).collect(),
                        2 => (0..131073).rev().map(|i| i as u64).collect(),
                        _ => data(131073, 0, 8, 0, false)
                            .into_iter()
                            .enumerate()
                            .map(|(i, v)| v ^ (i as u64).wrapping_mul(0x9e3779b97f4a7c15))
                            .collect(),
                    };
                    let mut expected = actual.clone();
                    expected.sort();
                    let hooks = Arc::new(Hooks::default());
                    with_hooks(hooks.clone(), || crate::par_stable_sort(&mut actual));
                    assert_eq!(actual, expected);
                    assert!(!hooks.parallel_count.load(SeqCst));
                }
                let mut rows: Vec<_> = data(65539, 0, 8, 0, false)
                    .into_iter()
                    .enumerate()
                    .map(|(i, k)| (k, i))
                    .collect();
                let mut expected = rows.clone();
                expected.sort_by_key(|r| r.0);
                let hooks = Arc::new(Hooks::default());
                with_hooks(hooks.clone(), || {
                    crate::par_stable_sort_by_key(&mut rows, |r| r.0)
                });
                assert_eq!(rows, expected);
                assert!(!hooks.parallel_count.load(SeqCst));
            });
    }
}

#[test]
fn unsupported_types_and_payload_extractors_decline() {
    let mut direct = Direct;
    assert!(!direct.try_parallel_count_window(&mut [1u8, 0], 0, 1));
    assert!(!direct.try_parallel_count_window(&mut [1u16, 0], 0, 1));
    assert!(!direct.try_parallel_count_window(&mut [1u128, 0], 0, 1));
    assert!(!direct.try_parallel_count_window(&mut [1i64, 0], 0, 1));
    assert!(!direct.try_parallel_count_window(&mut [1usize, 0], 0, 1));
    assert!(!direct.try_parallel_count_window(&mut [1f64, 0.], 0, 1));
    assert!(!direct.try_parallel_count_window(&mut [core::cmp::Reverse(1u64)], 0, 1));
    assert!(!ByKey(|r: &(u64, usize)| r.0).try_parallel_count_window(&mut [(1, 0), (0, 1)], 0, 1));
    // Invalid shifts decline before querying Rayon or allocating.
    assert!(!direct.try_parallel_count_window(&mut [1u64, 0], usize::MAX, 1));
    assert!(!direct.try_parallel_count_window(&mut [1u32, 0], 31, 2));
}

#[cfg(not(miri))]
#[test]
fn nested_public_calls_and_rank_balance_witness() {
    use std::sync::{Arc, atomic::Ordering::SeqCst};
    use witness::{Hooks, with_hooks};
    for workers in [2, 3] {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            let run = || {
                let mut actual = data(1_048_579, 56, 8, u64::MAX, true);
                let mut expected = actual.clone(); expected.sort();
                let hooks = Arc::new(Hooks::default());
                with_hooks(hooks.clone(), || crate::par_stable_sort(&mut actual));
                assert_eq!(actual, expected);
                assert!(hooks.parallel_count.load(SeqCst));
                let work = hooks.count_work.lock().unwrap();
                for phase in [false, true] {
                    let sizes: Vec<_> = work.iter().filter(|w| w.0 == phase).map(|w| w.2).collect();
                    assert_eq!(sizes.len(), workers);
                    assert_eq!(sizes.iter().sum::<usize>(), actual.len());
                    assert!(sizes.iter().max().unwrap() - sizes.iter().min().unwrap() < workers);
                }
                eprintln!("nested P={workers} actual count/fill worker and rank-size witness: {work:?}");
            };
            rayon::join(run, run);
        });
    }
}

#[cfg(not(miri))]
#[test]
fn actual_histogram_allocation_layout_diagnostic() {
    use std::sync::Arc;
    use witness::{Hooks, with_hooks};
    for workers in [2, 3, 8] {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
            for width in 1..=8 {
                let n: usize = 262149;
                let chunk = n.div_ceil(workers);
                // Neighboring rows concentrate on their boundary buckets. This
                // diagnoses layout only; it does not measure false sharing.
                let mut values: Vec<u32> = (0..n).map(|i| if (i / chunk) % 2 == 0 { (1 << width) - 1 } else { 0 }).collect();
                let mut expected = values.clone(); expected.sort();
                let hooks = Arc::new(Hooks::default());
                assert!(with_hooks(hooks.clone(), || Direct.try_parallel_count_window(&mut values, 0, width)));
                assert_eq!(values, expected);
                let (base_mod, p, r) = hooks.count_layout.lock().unwrap().unwrap();
                assert_eq!(p, workers);
                assert_eq!(r, 1 << width);
                let row_bytes = r * size_of::<usize>();
                let rows: Vec<_> = (0..p).map(|row| (row, (base_mod + row * row_bytes) % 64, row * row_bytes..(row + 1) * row_bytes)).collect();
                eprintln!("actual histogram layout: P={p} R={r} base_mod64={base_mod} rows=(index,start_mod64,used_byte_range) {rows:?}");
            }
        });
    }
}

// Only this count helper observes these scoped hooks. No production API or
// scheduler instrumentation is involved.
pub(super) mod witness {
    use std::{
        cell::RefCell,
        sync::{Arc, Mutex, atomic::AtomicBool},
    };
    #[derive(Default)]
    pub(crate) struct Hooks {
        pub parallel_count: AtomicBool,
        pub count_work: Mutex<Vec<(bool, usize, usize)>>,
        pub count_layout: Mutex<Option<(usize, usize, usize)>>,
    }
    std::thread_local! { static CURRENT: RefCell<Option<Arc<Hooks>>> = const { RefCell::new(None) }; }
    pub(crate) fn current() -> Option<Arc<Hooks>> {
        CURRENT.with(|v| v.borrow().clone())
    }
    pub(crate) fn with_hooks<R>(hooks: Arc<Hooks>, f: impl FnOnce() -> R) -> R {
        struct Restore(Option<Arc<Hooks>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                CURRENT.with(|v| *v.borrow_mut() = self.0.take());
            }
        }
        let _restore = Restore(CURRENT.with(|v| v.replace(Some(hooks))));
        f()
    }
}

#[cfg(not(miri))]
#[test]
fn public_u64_windows_and_unsupported_routes() {
    use std::sync::{Arc, atomic::Ordering::SeqCst};
    use witness::{Hooks, with_hooks};
    for workers in [2, 3, 8] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                for width in 1..=8 {
                    for low in [0, 64 - width] {
                        for base in [0, u64::MAX] {
                            let mut values = data(131075, low, width, base, true);
                            let mut expected = values.clone();
                            expected.sort();
                            let hooks = Arc::new(Hooks::default());
                            with_hooks(hooks.clone(), || crate::par_stable_sort(&mut values));
                            assert_eq!(values, expected);
                            assert!(hooks.parallel_count.load(SeqCst));
                        }
                    }
                }
                macro_rules! negative {
                    ($values:expr, $compare:expr) => {{
                        let mut values = $values;
                        let mut expected = values.clone();
                        expected.sort_by($compare);
                        let hooks = Arc::new(Hooks::default());
                        with_hooks(hooks.clone(), || crate::par_stable_sort(&mut values));
                        assert_eq!(values, expected);
                        assert!(!hooks.parallel_count.load(SeqCst));
                    }};
                }
                let input = data(262147, 0, 8, 0, false);
                negative!(
                    input.iter().map(|&v| v as i64).collect::<Vec<_>>(),
                    Ord::cmp
                );
                negative!(
                    input.iter().map(|&v| v as usize).collect::<Vec<_>>(),
                    Ord::cmp
                );
                negative!(
                    input.iter().map(|&v| (v as u128) << 64).collect::<Vec<_>>(),
                    Ord::cmp
                );
                negative!(
                    input
                        .iter()
                        .map(|&v| core::cmp::Reverse(v))
                        .collect::<Vec<_>>(),
                    Ord::cmp
                );
                negative!(
                    input.iter().map(|&v| v as f64).collect::<Vec<_>>(),
                    f64::total_cmp
                );
                let mut owned: Vec<_> = input
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (v, Box::new(i)))
                    .collect();
                let mut expected = owned.clone();
                expected.sort_by_key(|r| r.0);
                let hooks = Arc::new(Hooks::default());
                with_hooks(hooks.clone(), || {
                    crate::par_stable_sort_by_key(&mut owned, |r| r.0)
                });
                assert_eq!(owned, expected);
                assert!(!hooks.parallel_count.load(SeqCst));
            });
    }
}

#[cfg(not(miri))]
#[test]
fn late_unseen_bit_requires_full_variation_scan() {
    use std::sync::{Arc, atomic::Ordering::SeqCst};
    use witness::{Hooks, with_hooks};
    for workers in [2, 3] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut values = data(262147, 0, 8, 0, false);
                let (order, mode, _, probe) =
                    crate::engine::count_window_witness::inspected(&values);
                let sentinel = (probe.max(1025) + 1..values.len())
                    .find(|i| !order.contains(i) && !mode.contains(i))
                    .unwrap();
                values[sentinel] |= 1u64 << 63;
                let (order, mode, sample, actual_probe) =
                    crate::engine::count_window_witness::inspected(&values);
                assert!(sentinel > actual_probe && sentinel > 1025);
                assert!(!order.contains(&sentinel));
                assert!(!mode.contains(&sentinel));
                assert!(sample.iter().all(|&key| key <= 255));
                let mut expected = values.clone();
                expected.sort();
                let hooks = Arc::new(Hooks::default());
                with_hooks(hooks.clone(), || crate::par_stable_sort(&mut values));
                assert_eq!(values, expected);
                assert!(!hooks.parallel_count.load(SeqCst));
            });
    }
}

#[cfg(not(miri))]
#[test]
fn eligible_float_windows_still_decline() {
    use crate::RadixKey;
    use std::sync::{Arc, atomic::Ordering::SeqCst};
    use witness::{Hooks, with_hooks};
    rayon::ThreadPoolBuilder::new()
        .num_threads(3)
        .build()
        .unwrap()
        .install(|| {
            let input = data(262147, 0, 8, 0, false);
            macro_rules! check {
                ($ty:ty, $bits:ty) => {{
                    let mut values: Vec<$ty> = input
                        .iter()
                        .map(|&v| <$ty>::from_bits((1.0 as $ty).to_bits() | v as $bits))
                        .collect();
                    let first = values[0].encode();
                    let varying = values
                        .iter()
                        .fold(0 as $bits, |bits, &value| bits | (value.encode() ^ first));
                    assert_ne!(varying, 0);
                    assert!(
                        <$bits>::BITS - varying.leading_zeros() - varying.trailing_zeros() <= 8
                    );
                    assert!(values.len() * size_of::<$ty>() >= 1024 * 1024);
                    assert!(values.windows(2).any(|w| w[0] > w[1]));
                    assert!(values.windows(2).any(|w| w[0] < w[1]));
                    let mut expected = values.clone();
                    expected.sort_by(<$ty>::total_cmp);
                    let hooks = Arc::new(Hooks::default());
                    with_hooks(hooks.clone(), || crate::par_stable_sort(&mut values));
                    assert_eq!(values, expected);
                    assert!(!hooks.parallel_count.load(SeqCst));
                }};
            }
            check!(f64, u64);
            check!(f32, u32);
        });
}
