// SPDX-License-Identifier: BSD-2-Clause

use super::*;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Reverse;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[derive(Clone, Debug)]
struct DropTracker {
    _id: u64,
    counter: Arc<AtomicUsize>,
}

impl DropTracker {
    fn new(id: u64, counter: &Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self {
            _id: id,
            counter: Arc::clone(counter),
        }
    }
}

impl Drop for DropTracker {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}

#[test]
fn leaf_insert_remove_split_boundary_keys_and_bounds() {
    let mut map: TesseraMap<u64, u64> = TesseraMap::new();
    let mut oracle: BTreeMap<u64, u64> = BTreeMap::new();
    let extra_n = if cfg!(miri) { 140 } else { 500 };

    let mut keys = alloc::vec![0, u64::MAX, 1, u64::MAX - 1];
    let mut rng = Rng::new(0x1234_5678_9abc_def0);
    for _ in 0..extra_n {
        keys.push(rng.next_u64());
    }

    for (i, &k) in keys.iter().enumerate() {
        let v = i as u64 * 17 + 3;
        assert_eq!(map.insert(k, v), oracle.insert(k, v));
        assert_eq!(map.len(), oracle.len());
    }

    assert_eq!(map.first(), oracle.first_key_value().map(|(&k, v)| (k, v)));
    assert_eq!(map.last(), oracle.last_key_value().map(|(&k, v)| (k, v)));

    for (&k, &v) in &oracle {
        assert_eq!(map.get(k), Some(&v));
    }

    // Exclusive and unbounded range checks
    let mut got_excl = Vec::new();
    map.range(1..u64::MAX, |k, &v| got_excl.push((k, v)));
    let exp_excl: Vec<(u64, u64)> = oracle.range(1..u64::MAX).map(|(&k, &v)| (k, v)).collect();
    assert_eq!(got_excl, exp_excl);

    for &k in keys.iter().step_by(2) {
        assert_eq!(map.insert(k, k ^ 0x55), oracle.insert(k, k ^ 0x55));
    }
    for &k in keys.iter().step_by(2) {
        assert_eq!(map.remove(k), oracle.remove(&k));
        assert_eq!(map.get(k), None);
        assert_eq!(map.len(), oracle.len());
    }

    let mut got = Vec::new();
    map.range(.., |k, &v| got.push((k, v)));
    let expected: Vec<(u64, u64)> = oracle.into_iter().collect();
    assert_eq!(got, expected);
}

#[test]
fn differential_fuzz_insert_remove_get_range() {
    let mut rng = Rng::new(0xdead_beef_cafe_babe);
    let mut map: TesseraMap<u64, u64> = TesseraMap::new();
    let mut oracle: BTreeMap<u64, u64> = BTreeMap::new();
    let steps = if cfg!(miri) { 400 } else { 12_000 };

    for step in 0..steps {
        let k = if step % 3 == 0 {
            rng.next_u64() % 400
        } else {
            rng.next_u64()
        };
        let op = rng.next_u64() % 10;
        if op < 5 {
            assert_eq!(map.insert(k, step), oracle.insert(k, step));
        } else if op < 8 {
            assert_eq!(map.remove(k), oracle.remove(&k));
        } else {
            assert_eq!(map.get(k), oracle.get(&k));
        }
        assert_eq!(map.len(), oracle.len());
    }

    for _ in 0..20 {
        let a = rng.next_u64();
        let b = rng.next_u64();
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let mut got = Vec::new();
        map.range(lo..=hi, |k, &v| got.push((k, v)));
        let expected: Vec<(u64, u64)> = oracle.range(lo..=hi).map(|(&k, &v)| (k, v)).collect();
        assert_eq!(got, expected);
    }

    let all_keys: Vec<u64> = oracle.keys().copied().collect();
    for k in all_keys {
        assert_eq!(map.remove(k), oracle.remove(&k));
    }
    assert!(map.is_empty());
    assert_eq!(map.stats().leaves, 0);
    assert_eq!(map.stats().internals, 0);
}

#[test]
fn transparent_lazy_cracking_range_and_select_nth() {
    let mut rng = Rng::new(0x9e37_79b9_7f4a_7c15);
    let n = if cfg!(miri) {
        LAZY_THRESHOLD + 256
    } else {
        25_000usize
    };
    let mut raw = Vec::with_capacity(n);
    let mut oracle: BTreeMap<u64, u64> = BTreeMap::new();

    for i in 0..n {
        let k = rng.next_u64();
        let v = i as u64;
        raw.push((k, v));
        oracle.insert(k, v);
    }
    assert_eq!(oracle.len(), n);
    let sorted_oracle: Vec<(u64, u64)> = oracle.iter().map(|(&k, &v)| (k, v)).collect();

    let mut map = TesseraMap::from_unique(raw.clone());
    assert_eq!(map.stats().unrefined, 1);
    assert_eq!(map.stats().live_slabs, 1);
    // `len()` on `from_unique` does NOT force cracking!
    assert_eq!(map.len(), n);
    assert_eq!(map.stats().unrefined, 1);

    // Immutable `&self` lookups transparently crack ONLY touched buckets!
    let step = (n / 100).max(1);
    for idx in (0..n).step_by(step) {
        let (k, v) = raw[idx];
        assert_eq!(map.get(k), Some(&v));
        assert_eq!(map.get(k ^ 1), oracle.get(&(k ^ 1)));
    }
    let mid_stats = map.stats();
    assert!(mid_stats.radixes >= 1, "expected radix cracking");
    assert!(
        mid_stats.unrefined > 0,
        "expected untouched buckets to remain unrefined"
    );

    // `select_nth(&self)` transparently cracks only buckets along each rank!
    for r in [0, 1, 7, 100, 1024, 5000, n - 2, n - 1] {
        let (k, &v) = map.select_nth(r).expect("valid rank");
        assert_eq!((k, v), sorted_oracle[r], "rank {r} mismatch");
    }

    // `range(&self)` transparently cracks only overlapping buckets!
    let lo = sorted_oracle[3000].0;
    let hi = sorted_oracle[3500].0;
    let mut got_range = Vec::new();
    map.range(lo..=hi, |k, &v| got_range.push((k, v)));
    assert_eq!(got_range, sorted_oracle[3000..=3500]);

    // Insert boundary keys outside prefix into partially-cracked map
    assert_eq!(map.insert(0, 999_000), oracle.insert(0, 999_000));
    assert_eq!(
        map.insert(u64::MAX, 999_001),
        oracle.insert(u64::MAX, 999_001)
    );
    assert_eq!(map.first(), Some((0, &999_000)));
    assert_eq!(map.last(), Some((u64::MAX, &999_001)));

    // Full range refines every remaining bucket and frees slabs automatically!
    let mut all_got = Vec::new();
    map.range(.., |k, &v| all_got.push((k, v)));
    let all_exp: Vec<(u64, u64)> = oracle.into_iter().collect();
    assert_eq!(all_got, all_exp);
    assert_eq!(map.stats().unrefined, 0);
    assert_eq!(map.stats().live_slabs, 0);
}

#[test]
fn concurrent_shared_reads_and_lazy_cracking() {
    // Under Miri, exceed `TILE_REFINE_THRESHOLD` (1024) with a forced lazy root so concurrent
    // threads genuinely exercise multi-bucket `RadixNode` cracking and `&V` stability.
    let n = if cfg!(miri) { 1_152 } else { 40_000usize };
    let mut rng = Rng::new(0xabcd_ef01_2345_6789);
    let mut raw = Vec::with_capacity(n);
    let mut oracle = BTreeMap::new();
    for i in 0..n {
        let k = rng.next_u64();
        raw.push((k, i as u64));
        oracle.insert(k, i as u64);
    }
    let map = if cfg!(miri) {
        TesseraMap::from_vec_lazy_for_test(raw.clone(), false)
    } else {
        TesseraMap::from_unique(raw.clone())
    };
    assert!(
        map.stats().unrefined > 0,
        "concurrent fixture must start unrefined"
    );
    let sorted: Vec<(u64, u64)> = oracle.iter().map(|(&k, &v)| (k, v)).collect();

    // 2-8 threads simultaneously reading, ranging, and select_nth-ing on `&map` while it cracks!
    let threads = if cfg!(miri) { 2 } else { 8 };
    std::thread::scope(|s| {
        for tid in 0..threads {
            let map = &map;
            let raw = &raw;
            let oracle = &oracle;
            let sorted = &sorted;
            s.spawn(move || {
                // Hold a `&V` reference across subsequent concurrent cracks on `&map`
                // to verify append-only leaf address stability!
                let (anchor_k, anchor_v) = raw[tid];
                let pinned_ref = map.get(anchor_k).expect("anchor exists");
                assert_eq!(*pinned_ref, anchor_v);

                for idx in (tid..n).step_by(threads * 7) {
                    let (k, v) = raw[idx];
                    assert_eq!(map.get(k), Some(&v));
                    assert_eq!(map.get(k ^ 1), oracle.get(&(k ^ 1)));
                }

                let r = (tid * 37) % n;
                let (rk, &rv) = map.select_nth(r).unwrap();
                assert_eq!((rk, rv), sorted[r]);

                let lo_i = (tid * 19) % (n - 20);
                let mut range_out = Vec::new();
                map.range(sorted[lo_i].0..=sorted[lo_i + 15].0, |k, &v| {
                    range_out.push((k, v))
                });
                assert_eq!(range_out, sorted[lo_i..=lo_i + 15]);

                // Anchor reference acquired BEFORE all the cracks above must still be valid!
                assert_eq!(*pinned_ref, anchor_v);
            });
        }
    });
    assert!(
        map.stats().leaves > 0,
        "concurrent reads must have published leaves"
    );
}

#[test]
fn duplicates_and_extend_vec_exact_len() {
    let mut rng = Rng::new(0x55aa_55aa_1122_3344);
    let mut oracle: BTreeMap<u64, u64> = BTreeMap::new();
    let n = if cfg!(miri) { 500 } else { 20_000usize };
    let mut initial = Vec::with_capacity(n);
    for i in 0..n as u64 {
        let k = 0x0012_3400_0000_0000 | (rng.next_u64() % 5000);
        initial.push((k, i));
        oracle.insert(k, i);
    }

    let mut map = TesseraMap::from(initial);
    // Crack a few keys lazily
    for k_off in [0u64, 500, 1500, 3000, 4999] {
        let k = 0x0012_3400_0000_0000 | k_off;
        assert_eq!(map.get(k), oracle.get(&k));
    }

    // 3 rounds of extend_vec (duplicates + out-of-prefix keys)
    let batch_n = if cfg!(miri) { 200 } else { 5_000u64 };
    for round in 0..3u64 {
        let mut batch = Vec::with_capacity(batch_n as usize);
        for i in 0..batch_n {
            let k = if round == 2 && i < 10 {
                i * 1_000_000_000_000
            } else {
                0x0012_3400_0000_0000 | (rng.next_u64() % 8000)
            };
            let v = 100_000 * (round + 1) + i;
            batch.push((k, v));
            oracle.insert(k, v);
        }
        map.extend_vec(batch);
    }

    // `len()` automatically resolves remaining unverified duplicates for exact length!
    assert_eq!(map.len(), oracle.len());
    let mut got = Vec::new();
    map.range(.., |k, &v| got.push((k, v)));
    let exp: Vec<(u64, u64)> = oracle.into_iter().collect();
    assert_eq!(got, exp);
}

#[test]
fn non_copy_values_zero_leaks_across_all_paths() {
    let live = Arc::new(AtomicUsize::new(0));
    let n = if cfg!(miri) { 400 } else { 20_000u64 };
    {
        let mut raw = Vec::new();
        for i in 0..n {
            let k = (i * 7919) % (n / 2).max(1);
            raw.push((k, DropTracker::new(i, &live)));
        }
        let mut map = TesseraMap::from(raw);
        for k in (0..n / 2).step_by(17) {
            let _ = map.get(k);
        }
        for k in 0..200u64 {
            let _ = map.insert(k, DropTracker::new(k + 100_000, &live));
        }
        for k in (0..200u64).step_by(3) {
            let _ = map.remove(k);
        }
        let batch: Vec<_> = (0..300u64)
            .map(|i| ((i * 3571) % 4000, DropTracker::new(i + 200_000, &live)))
            .collect();
        map.extend_vec(batch);
    }
    assert_eq!(
        live.load(Ordering::Relaxed),
        0,
        "memory leak: DropTracker count must return to 0"
    );
}

#[test]
fn tessera_set_and_signed_float_reverse_keys() {
    // Signed i64 map
    let mut m_i64: TesseraMap<i64, String> = TesseraMap::new();
    for k in [i64::MAX, 0, -1, i64::MIN, 42, -100] {
        m_i64.insert(k, alloc::format!("v={k}"));
    }
    let mut ks = Vec::new();
    m_i64.range(.., |k, _| ks.push(k));
    assert_eq!(ks, alloc::vec![i64::MIN, -100, -1, 0, 42, i64::MAX]);

    // f64 total order (-0.0 < +0.0)
    let mut m_f64: TesseraMap<f64, i32> = TesseraMap::new();
    for (k, v) in [
        (1.5, 1),
        (-0.0, 2),
        (0.0, 3),
        (f64::NEG_INFINITY, 4),
        (f64::INFINITY, 5),
    ] {
        m_f64.insert(k, v);
    }
    assert_eq!(m_f64.get(-0.0), Some(&2));
    assert_eq!(m_f64.get(0.0), Some(&3));

    // TesseraSet with Reverse<u32>
    let mut set: TesseraSet<Reverse<u32>> = (0..1000u32).map(Reverse).collect();
    assert_eq!(set.len(), 1000);
    assert!(set.contains(Reverse(500)));
    assert!(!set.insert(Reverse(500)));
    assert!(set.remove(Reverse(500)));
    assert_eq!(set.len(), 999);
    assert_eq!(set.first(), Some(Reverse(999)));
    assert_eq!(set.last(), Some(Reverse(0)));
    let mut out = Vec::new();
    set.range(Reverse(10)..=Reverse(5), |k| out.push(k.0));
    assert_eq!(out, alloc::vec![10, 9, 8, 7, 6, 5]);
    assert_eq!(set.select_nth(0), Some(Reverse(999)));
}

#[test]
#[cfg_attr(miri, ignore = "large slab compaction stress covered in native test")]
fn sequential_inserts_and_slab_compaction() {
    let mut seq: TesseraMap<u64, u64> = TesseraMap::new();
    for i in 0..5000u64 {
        assert_eq!(seq.insert(i * 2, i), None);
    }
    assert_eq!(seq.len(), 5000);
    assert!(
        seq.stats().leaves <= 85,
        "expected dense sequential leaves, got {}",
        seq.stats().leaves
    );
    for i in 0..5000u64 {
        assert_eq!(seq.get(i * 2), Some(&i));
        assert_eq!(seq.get(i * 2 + 1), None);
    }
    for i in 0..2000u64 {
        assert_eq!(seq.insert(i * 2 + 1, i + 10_000), None);
    }
    for i in (0..5000u64).step_by(2) {
        assert_eq!(seq.remove(i * 2), Some(i));
    }
    assert_eq!(seq.len(), 4500);

    // Slab compaction under `&self` reads!
    let mut rng = Rng::new(0x1122_3344_5566_7788);
    let raw: Vec<(u64, u64)> = (0..25_000u64).map(|i| (rng.next_u64(), i)).collect();
    let map = TesseraMap::from_unique(raw.clone());
    for &(k, v) in &raw[..22_500] {
        assert_eq!(map.get(k), Some(&v));
    }
    for &(k, v) in &raw[22_500..] {
        assert_eq!(map.get(k), Some(&v));
    }
    assert_eq!(map.stats().live_slabs, 0);
}

#[test]
#[cfg_attr(miri, ignore = "radix routing stress covered in native test")]
fn out_of_prefix_inserts_use_leaf_splits() {
    const BASE: u64 = 1 << 40;
    const PREFIX_END: u64 = BASE + (1 << 15);
    const INSERTED: usize = 1792;
    let initial: Vec<_> = (0..20_000)
        .map(|i| (BASE + (i * 7919) % 20_000, i))
        .collect();
    let mut rng = Rng::new(0x1122_3344_5566_7788);
    for below in [true, false] {
        for order in ["ascending", "descending", "shuffled", "nested batch"] {
            let mut records = initial.clone();
            let nested = order == "nested batch";
            if nested {
                records.push((1 << 55, 0));
            }
            let mut oracle: BTreeMap<_, _> = records.iter().copied().collect();
            let mut map = TesseraMap::from_unique(records);
            assert_eq!(map.get(BASE), oracle.get(&BASE));
            let before = map.stats();
            assert!(before.radixes >= if nested { 2 } else { 1 });

            let start = if below {
                BASE - INSERTED as u64
            } else {
                PREFIX_END
            };
            let mut keys: Vec<_> = (start..start + INSERTED as u64).collect();
            if order == "descending" {
                keys.reverse();
            } else if order == "shuffled" {
                for i in (1..keys.len()).rev() {
                    keys.swap(i, rng.next_u64() as usize % (i + 1));
                }
            }
            oracle.extend(keys.iter().map(|&k| (k, k)));
            if nested {
                map.extend_vec(keys.into_iter().map(|k| (k, k)).collect());
            } else {
                for k in keys {
                    assert_eq!(map.insert(k, k), None);
                }
            }

            let after = map.stats();
            let bound = INSERTED.div_ceil(28) + 2;
            let leaves = after.leaves - before.leaves;
            let internals = after.internals - before.internals;
            assert!(
                leaves <= bound,
                "{order}, below={below}: {leaves} leaves > {bound}"
            );
            assert!(
                internals <= bound,
                "{order}, below={below}: {internals} internals > {bound}"
            );
            assert_eq!(map.len(), oracle.len());
            let mut got = Vec::new();
            map.range(.., |k, v| got.push((k, *v)));
            assert!(got.into_iter().eq(oracle));
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "native thread stack limit")]
fn large_values_split_on_small_stack() {
    const VALUE_BYTES: usize = 4096;

    struct LargeValue {
        bytes: [u8; VALUE_BYTES],
        tracker: DropTracker,
    }

    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let live = Arc::new(AtomicUsize::new(0));
            let value_for = |key: u32| LargeValue {
                bytes: [key as u8; VALUE_BYTES],
                tracker: DropTracker::new(u64::from(key), &live),
            };
            {
                let mut map = TesseraMap::new();
                for key in (1..=64u32).chain([0]).chain(65..=128) {
                    assert!(map.insert(key, value_for(key)).is_none());
                }
                assert_eq!(map.len(), 129);
                assert!(map.stats().leaves >= 3);
                assert_eq!(live.load(Ordering::Relaxed), 129);
                for key in 0..=128 {
                    let value = map.get(key).unwrap();
                    assert_eq!(value.tracker._id, u64::from(key));
                    assert!(value.bytes.iter().all(|&byte| byte == key as u8));
                }
            }
            assert_eq!(live.load(Ordering::Relaxed), 0);

            for lazy in [false, true] {
                let records = (0..129).map(|key| (key, value_for(key))).collect();
                let map = if lazy {
                    TesseraMap::from_vec_lazy_for_test(records, false)
                } else {
                    TesseraMap::from(records)
                };
                assert_eq!(map.len(), 129);
                for key in 0..129 {
                    let value = map.get(key).unwrap();
                    assert_eq!(value.tracker._id, u64::from(key));
                    assert!(value.bytes.iter().all(|&byte| byte == key as u8));
                }
                assert_eq!(live.load(Ordering::Relaxed), 129);
                drop(map);
                assert_eq!(live.load(Ordering::Relaxed), 0);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn slab_slots_reused_with_unrefined_bucket() {
    let anchor = 15u64 << 56;
    let records = (0..600).flat_map(|i| [(i, i), (anchor + i, i)]).collect();
    let mut map = TesseraMap::from_vec_lazy_for_test(records, false);
    assert_eq!(map.get(0), Some(&0));
    assert_eq!(map.stats().unrefined, 1);
    let empty_key = 2 << 56;

    for cycle in 0..64 {
        map.extend_vec((0..256).map(|i| (empty_key, cycle + i)).collect());
        assert_eq!(map.stats().unrefined, 2);
        assert_eq!(map.remove(empty_key), Some(cycle + 255));
        assert_eq!(map.stats().unrefined, 1);
        let slots = map.refine_mut().slabs.len();
        assert!(slots <= 2, "cycle {cycle}: {slots} slab slots");
    }
    assert_eq!(map.get(anchor + 599), Some(&599));
}

fn generic_lookup<K: crate::RadixKey, V>(map: &TesseraMap<K, V>, key: K) -> Option<&V> {
    map.get(key)
}

#[repr(align(128))]
#[derive(Debug, PartialEq, Eq)]
struct OverAligned(u64);

#[test]
fn generic_radix_key_api_and_exotic_key_types() {
    // u8 boundary keys (0 and u8::MAX)
    let m_u8: TesseraMap<u8, u8> = (0..=u8::MAX).rev().map(|k| (k, k ^ 0xaa)).collect();
    assert_eq!(m_u8.len(), 256);
    assert_eq!(generic_lookup(&m_u8, 0), Some(&0xaa));
    assert_eq!(generic_lookup(&m_u8, u8::MAX), Some(&0x55));
    assert_eq!(m_u8.first(), Some((0, &0xaa)));
    assert_eq!(m_u8.last(), Some((u8::MAX, &0x55)));

    // u128, i128, bool, char, NonZero, tuple keys, f64::NAN, and over-aligned values
    let mut m_u128: TesseraMap<u128, OverAligned> = TesseraMap::new();
    for k in [0u128, 1, u128::MAX / 2, u128::MAX - 1, u128::MAX] {
        m_u128.insert(k, OverAligned(k as u64));
    }
    assert_eq!(
        generic_lookup(&m_u128, u128::MAX),
        Some(&OverAligned(u64::MAX))
    );
    assert_eq!(m_u128.first().map(|(k, _)| k), Some(0));
    assert_eq!(m_u128.last().map(|(k, _)| k), Some(u128::MAX));

    let m_i128: TesseraMap<i128, i8> =
        TesseraMap::from(alloc::vec![(i128::MAX, 1), (i128::MIN, -1), (0, 0)]);
    assert_eq!(m_i128.select_nth(0), Some((i128::MIN, &-1)));
    assert_eq!(m_i128.select_nth(2), Some((i128::MAX, &1)));

    let mut s_bool = TesseraSet::from(alloc::vec![true, false, true]);
    assert_eq!(s_bool.len(), 2);
    s_bool.extend_vec(alloc::vec![false, true]);
    assert_eq!(s_bool.first(), Some(false));
    assert_eq!(s_bool.last(), Some(true));

    let m_char: TesseraMap<char, u32> =
        TesseraMap::from(alloc::vec![('z', 1), ('a', 2), ('🦀', 3)]);
    assert_eq!(generic_lookup(&m_char, '🦀'), Some(&3));

    let nz1 = core::num::NonZero::<u32>::new(1).unwrap();
    let nz9 = core::num::NonZero::<u32>::new(9).unwrap();
    let m_nz: TesseraMap<core::num::NonZero<u32>, u32> =
        TesseraMap::from(alloc::vec![(nz9, 90), (nz1, 10)]);
    assert_eq!(m_nz.first(), Some((nz1, &10)));

    let m_tup: TesseraMap<(u16, i16), usize> =
        TesseraMap::from(alloc::vec![((1, 5), 1), ((1, -5), 2), ((0, 100), 3)]);
    assert_eq!(m_tup.select_nth(0), Some(((0, 100), &3)));
    assert_eq!(m_tup.select_nth(1), Some(((1, -5), &2)));

    let mut m_nan: TesseraMap<f64, u32> = TesseraMap::new();
    m_nan.insert(f64::NAN, 7);
    m_nan.insert(-f64::NAN, 8);
    m_nan.insert(0.0, 9);
    assert_eq!(m_nan.get(f64::NAN), Some(&7));
    assert_eq!(m_nan.get(-f64::NAN), Some(&8));
    assert_eq!(m_nan.first().map(|(_, &v)| v), Some(8));
    assert_eq!(m_nan.last().map(|(_, &v)| v), Some(7));
}

#[test]
#[cfg_attr(miri, ignore = "large lazy-cracking regression covered in native test")]
fn extend_vec_newly_varying_bits_into_unrefined_bucket() {
    // All initial keys have bit 0 == 0 (even), so initial candidate_mask clears bit 0.
    let raw: Vec<(u64, u64)> = (0..20_000u64)
        .map(|i| (((i.wrapping_mul(2654435761)) & !1u64), i))
        .collect();
    let mut map = TesseraMap::from(raw.clone());
    // Crack root once into a RadixNode with unrefined buckets.
    assert!(map.get(raw[0].0).is_some());
    assert!(map.stats().radixes >= 1 && map.stats().unrefined > 0);

    // Extend with odd keys (bit 0 == 1) landing in unrefined buckets.
    let odd_batch: Vec<(u64, u64)> = (0..1000u64)
        .map(|i| (((i.wrapping_mul(2654435761)) | 1u64), i + 100_000))
        .collect();
    map.extend_vec(odd_batch.clone());

    let mut prev: Option<u64> = None;
    let mut count = 0usize;
    map.range(.., |k, _| {
        if let Some(p) = prev {
            assert!(
                p < k,
                "keys must remain strictly ascending after extend_vec"
            );
        }
        prev = Some(k);
        count += 1;
    });
    assert_eq!(count, map.len());
    for &(k, v) in &odd_batch {
        assert_eq!(map.get(k), Some(&v));
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "large multi-level cracking regression covered in native test"
)]
fn nested_radix_duplicate_cracking_keeps_ancestor_counts_exact() {
    // 40k records with top-level and nested radix levels + duplicates in bucket 0.
    let mut raw: Vec<(u64, u64)> = Vec::with_capacity(40_000);
    // Concentrate 10k records (with 50% duplicates!) into prefix 0 so bucket 0 cracks into a nested RadixNode.
    for i in 0..10_000u64 {
        raw.push(((i % 5_000) << 4, i));
    }
    // Spread remaining 30k unique records across high prefixes.
    for i in 0..30_000u64 {
        let k = ((i % 255) + 1) << 56 | (i * 17);
        raw.push((k, i + 50_000));
    }
    let mut ref_map = BTreeMap::new();
    for &(k, v) in &raw {
        ref_map.insert(k, v);
    }
    let map = TesseraMap::from(raw);
    // Crack all entries via `get()` ONLY (never calling `len()` beforehand) so `has_unverified_dups`
    // transitions to `false` through normal cracking!
    for (&k, &v) in &ref_map {
        assert_eq!(map.get(k), Some(&v));
    }
    assert_eq!(map.stats().unrefined, 0);
    assert_eq!(map.len(), ref_map.len());

    let sorted: Vec<(u64, u64)> = ref_map.into_iter().collect();
    for rank in [0, 1, 2500, 4999, 5000, 10_000, sorted.len() - 1] {
        assert_eq!(
            map.select_nth(rank).map(|(k, &v)| (k, v)),
            Some(sorted[rank]),
            "select_nth mismatch at rank {rank}"
        );
    }
}

#[test]
#[cfg_attr(miri, ignore = "large lazy-cracking regression covered in native test")]
fn out_of_prefix_insert_wrapping_unrefined_radix_and_resolve() {
    let live = Arc::new(AtomicUsize::new(0));
    {
        // Bucket 0 has 6k records all sharing bits 32..56 == 0, while other buckets span top byte.
        let mut raw: Vec<(u64, DropTracker)> = Vec::new();
        for i in 0..6_000u64 {
            raw.push((i * 13, DropTracker::new(i, &live)));
        }
        for i in 0..14_000u64 {
            let k = (((i % 200) + 1) << 56) | i;
            raw.push((k, DropTracker::new(i + 10_000, &live)));
        }
        let mut map = TesseraMap::from(raw);
        // Crack bucket 0 once so R0.children[0] becomes nested RadixNode R1 with narrow prefix
        // (top bits 0..56 zero) and unrefined children inside R1.
        assert!(map.get(0).is_some());
        // Insert a key inside R0's bucket 0 (top byte == 0) but OUTSIDE R1's narrow prefix!
        let outside_r1 = 1u64 << 48;
        assert!(
            map.insert(outside_r1, DropTracker::new(999, &live))
                .is_none()
        );
        // Now R0.children[0] is TAG_INTERNAL wrapping R1 (which still has unrefined children).
        assert_eq!(map.len(), 20_001);
        assert_eq!(map.stats().unrefined, 0);
        assert_eq!(map.get(outside_r1).map(|d| d._id), Some(999));
        assert_eq!(map.get(13 * 5999).map(|d| d._id), Some(5999));
    }
    assert_eq!(live.load(Ordering::Relaxed), 0);
}

#[test]
fn removing_all_keys_from_radix_unlinks_and_preserves_first_last() {
    // 1. Root RadixNode (>= 4096 entries in narrow prefix): remove all, then insert outside prefix.
    let n = if cfg!(miri) { 64u64 } else { 5_000u64 };
    let raw: Vec<(u64, u64)> = (0..n).map(|i| (0x0010_0000 + i * 3, i)).collect();
    let mut map = TesseraMap::from(raw.clone());
    assert_eq!(map.len(), n as usize);
    for &(k, v) in &raw {
        assert_eq!(map.remove(k), Some(v));
    }
    assert!(map.is_empty());
    assert_eq!(map.first(), None);
    assert_eq!(map.last(), None);
    assert_eq!(map.stats().radixes, 0);

    // Inserting outside the old prefix works cleanly and first/last return the new key.
    map.insert(u64::MAX, 42);
    assert_eq!(map.first(), Some((u64::MAX, &42)));
    assert_eq!(map.last(), Some((u64::MAX, &42)));
    assert_eq!(map.remove(u64::MAX), Some(42));
    assert!(map.is_empty());

    // 2. Nested R0 -> R1: empty R1 while R0 still holds keys in another bucket.
    if !cfg!(miri) {
        let mut nested_raw: Vec<(u64, u64)> = Vec::new();
        for i in 0..2_000u64 {
            nested_raw.push((i, i));
        }
        for i in 0..16_000u64 {
            nested_raw.push((((i % 200) + 1) << 56 | i, i + 10_000));
        }
        let mut m2 = TesseraMap::from(nested_raw);
        // Crack bucket 0 into nested RadixNode R1 and remove every key in bucket 0.
        for i in 0..2_000u64 {
            assert_eq!(m2.remove(i), Some(i));
        }
        assert_eq!(m2.first().map(|(k, _)| k), Some(1u64 << 56));
        assert!(m2.last().is_some());
    }
}

#[test]
#[cfg_attr(miri, ignore = "deep u128 stress covered in native test")]
fn deep_skewed_u128_exceeds_inline_path_depth() {
    // Construct a skewed u128 distribution where each radix crack peels off a few bits,
    // forcing depth > INLINE_PATH_CAP (16) and > 24.
    let mut raw: Vec<(u128, u64)> = Vec::new();
    let mut id = 0u64;
    for shift in (12..124).step_by(4) {
        let base = 1u128 << shift;
        for j in 0..1_200u128 {
            raw.push((base | j, id));
            id += 1;
        }
    }
    let first_key = 1u128 << 12;
    let mut map = TesseraMap::from(raw);
    // Probe deepest keys near `first_key` to force repeated cracking all the way down.
    assert_eq!(map.get(first_key), Some(&0));
    assert_eq!(map.insert(0u128, 999_999), None);
    assert_eq!(map.first(), Some((0u128, &999_999)));
    assert_eq!(map.remove(0u128), Some(999_999));
    assert_eq!(map.get(first_key), Some(&0));
}

#[test]
fn all_identical_duplicates_from_unique_and_panic_safety() {
    // 1. All-identical keys (tests `varying == 0` path in lazy cracking and eager dedup).
    let n = if cfg!(miri) { 128u64 } else { 20_000u64 };
    let dup_raw: Vec<(u64, u64)> = (0..n).map(|i| (42, i)).collect();
    let map = TesseraMap::from(dup_raw);
    assert_eq!(map.get(42), Some(&(n - 1)));
    assert_eq!(map.len(), 1);
    assert_eq!(map.select_nth(0), Some((42, &(n - 1))));
    assert_eq!(map.select_nth(1), None);

    // 2. `from_unique` given duplicates (< LAZY_THRESHOLD) must still preserve Leaf invariants.
    let u_small = TesseraMap::from_unique(alloc::vec![(5u64, 1), (5, 2), (1, 10), (5, 3)]);
    assert_eq!(u_small.len(), 2);
    assert_eq!(u_small.get(5), Some(&3));
    assert_eq!(u_small.first(), Some((1, &10)));

    // 3. Panicking `range` callback unwinds cleanly without poisoning or leaking.
    let map2 = TesseraMap::from(alloc::vec![(1u64, 10), (2, 20), (3, 30)]);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        map2.range(.., |k, _| {
            if k == 2 {
                panic!("callback abort");
            }
        });
    }));
    assert!(res.is_err());
    assert_eq!(map2.get(3), Some(&30));
    assert_eq!(map2.len(), 3);

    // 4. Concurrent `TesseraMap::from` with duplicates: simultaneous get/range/len/select_nth.
    let cn = if cfg!(miri) { 128u64 } else { 24_000u64 };
    let mut c_raw = Vec::with_capacity(cn as usize);
    let mut ref_m = BTreeMap::new();
    for i in 0..cn {
        let k = (i * 7919) % (cn / 2);
        c_raw.push((k, i));
        ref_m.insert(k, i);
    }
    let cmap = if cfg!(miri) {
        TesseraMap::from_vec_lazy_for_test(c_raw, true)
    } else {
        TesseraMap::from(c_raw)
    };
    assert!(cmap.stats().unrefined > 0);
    let expected_len = ref_m.len();
    let expected_first = ref_m.first_key_value().map(|(&k, &v)| (k, v));
    std::thread::scope(|s| {
        let m = &cmap;
        let h1 = s.spawn(move || assert_eq!(m.len(), expected_len));
        let h2 = s.spawn(move || assert_eq!(m.select_nth(0).map(|(k, &v)| (k, v)), expected_first));
        let h3 = s.spawn(move || {
            let mut cnt = 0usize;
            m.range(0..100, |_, _| cnt += 1);
            assert!(cnt > 0);
        });
        let h4 = s.spawn(move || {
            if let Some((fk, fv)) = expected_first {
                assert_eq!(m.get(fk), Some(&fv));
            }
        });
        h1.join().unwrap();
        h2.join().unwrap();
        h3.join().unwrap();
        h4.join().unwrap();
    });
}

struct PanicOnceValue {
    id: u64,
    armed: Option<Arc<AtomicBool>>,
    live: Arc<AtomicUsize>,
}

impl PanicOnceValue {
    fn new(id: u64, armed: Option<Arc<AtomicBool>>, live: &Arc<AtomicUsize>) -> Self {
        live.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            armed,
            live: Arc::clone(live),
        }
    }
}

impl Drop for PanicOnceValue {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::Relaxed);
        if let Some(flag) = &self.armed
            && flag.swap(false, Ordering::SeqCst)
        {
            panic!("intentional single drop panic for id {}", self.id);
        }
    }
}

#[test]
fn duplicate_drop_panic_recovers_and_teardown_drops_all_siblings() {
    // 1. Lazy `get` and `len` recover cleanly if an overwritten duplicate's `Drop` panics once.
    for use_len_first in [false, true] {
        let live = Arc::new(AtomicUsize::new(0));
        let armed = Arc::new(AtomicBool::new(true));
        let mut raw = Vec::new();
        for i in 0..32u64 {
            let trigger = (i == 5).then(|| Arc::clone(&armed));
            raw.push((77u64, PanicOnceValue::new(i, trigger, &live)));
        }
        let map = TesseraMap::from_vec_lazy_for_test(raw, true);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if use_len_first {
                let _ = map.len();
            } else {
                let _ = map.get(77);
            }
        }));
        assert!(res.is_err(), "expected overwritten duplicate drop to panic");
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(77).map(|v| v.id), Some(31));
        assert_eq!(live.load(Ordering::Relaxed), 1);
        drop(map);
        assert_eq!(live.load(Ordering::Relaxed), 0);
    }

    // 2. Teardown across published leaves drops every sibling even when one destructor panics.
    let live_pub = Arc::new(AtomicUsize::new(0));
    let armed_pub = Arc::new(AtomicBool::new(true));
    let mut pub_map = TesseraMap::new();
    for i in 0..150u64 {
        let trigger = (i == 3).then(|| Arc::clone(&armed_pub));
        pub_map.insert(i, PanicOnceValue::new(i, trigger, &live_pub));
    }
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || drop(pub_map)));
    assert!(res.is_err());
    assert_eq!(
        live_pub.load(Ordering::Relaxed),
        0,
        "every published leaf entry must be dropped despite one panicking destructor"
    );

    // 3. Teardown of unrefined slab drops every remaining record even when one destructor panics.
    let live_slab = Arc::new(AtomicUsize::new(0));
    let armed_slab = Arc::new(AtomicBool::new(true));
    let raw_slab: Vec<_> = (0..80u64)
        .map(|i| {
            let trigger = (i == 4).then(|| Arc::clone(&armed_slab));
            (i, PanicOnceValue::new(i, trigger, &live_slab))
        })
        .collect();
    let lazy_map = TesseraMap::from_vec_lazy_for_test(raw_slab, true);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || drop(lazy_map)));
    assert!(res.is_err());
    assert_eq!(
        live_slab.load(Ordering::Relaxed),
        0,
        "every unrefined slab entry must be dropped despite one panicking destructor"
    );
}

#[test]
fn reentrant_duplicate_drop_does_not_deadlock() {
    use std::sync::{Mutex as StdMutex, Weak};

    type ReentrantMap = TesseraMap<u64, ReentrantVal>;

    struct ReentrantVal {
        id: u64,
        slot: Arc<StdMutex<Weak<ReentrantMap>>>,
        observed_stats: Arc<AtomicUsize>,
    }

    impl Drop for ReentrantVal {
        fn drop(&mut self) {
            let weak = self.slot.lock().unwrap().clone();
            if let Some(map) = weak.upgrade() {
                let stats = map.stats();
                assert!(stats.len >= 1);
                let _ = map.get(42);
                self.observed_stats.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    for via_len in [false, true] {
        let slot: Arc<StdMutex<Weak<ReentrantMap>>> = Arc::new(StdMutex::new(Weak::new()));
        let observed = Arc::new(AtomicUsize::new(0));
        let raw: Vec<_> = (0..16u64)
            .map(|id| {
                (
                    42u64,
                    ReentrantVal {
                        id,
                        slot: Arc::clone(&slot),
                        observed_stats: Arc::clone(&observed),
                    },
                )
            })
            .collect();
        let map = Arc::new(TesseraMap::from_vec_lazy_for_test(raw, true));
        *slot.lock().unwrap() = Arc::downgrade(&map);
        if via_len {
            assert_eq!(map.len(), 1);
        } else {
            assert_eq!(map.get(42).map(|v| v.id), Some(15));
        }
        assert_eq!(observed.load(Ordering::Relaxed), 15);
        *slot.lock().unwrap() = Weak::new();
    }
}

#[test]
fn is_empty_and_spare_capacity_adoption_stay_lazy_o1() {
    // 1. `is_empty()` must not refine an unrefined batch (even of identical keys).
    let raw: Vec<(u64, u64)> = (0..64u64).map(|i| (99, i)).collect();
    let mut map = TesseraMap::from_vec_lazy_for_test(raw, true);
    assert!(!map.is_empty());
    assert_eq!(
        map.stats().unrefined,
        1,
        "is_empty() must not materialize unrefined buckets"
    );
    assert_eq!(map.remove(99), Some(63));
    assert!(map.is_empty());
    map.insert(1, 2);
    assert!(!map.is_empty());
    map.clear();
    assert!(map.is_empty());

    // 2. Adopting a `Vec` with spare capacity retains the original heap buffer without copying.
    let mut spare = Vec::with_capacity(256);
    for i in 0..64u64 {
        spare.push((i, i * 10));
    }
    let orig_ptr = spare.as_ptr();
    let slab = RawSlab::from_vec(spare);
    assert_eq!(slab.data.as_ptr() as *const (u64, u64), orig_ptr);
    assert_eq!(slab.data.capacity(), 256);
    assert_eq!(slab.live_count, 64);
    // SAFETY: all `slab.data.len()` slots are initialized.
    let reclaimed = unsafe { slab.into_vec() };
    assert_eq!(reclaimed.as_ptr(), orig_ptr);
    assert_eq!(reclaimed.len(), 64);
}

#[test]
fn extend_vec_refining_last_cold_descendant_with_later_empty_bucket() {
    // Exercise both bucket orderings (`cold_bucket < empty_bucket` and `cold_bucket > empty_bucket`)
    // so mid-loop convergence (`!state.has_unrefined()`) fires regardless of bucket traversal order.
    for &(cold_hi, empty_hi, anchor_hi) in &[
        (14u64 << 56, 2u64 << 56, 0u64),
        (0u64, 2u64 << 56, 15u64 << 56),
    ] {
        let live = Arc::new(AtomicUsize::new(0));
        {
            // `cold_hi` bucket has > TILE_REFINE_THRESHOLD items so cracking it produces a nested
            // RadixNode with unrefined descendants; `anchor_hi` spans the top prefix; `empty_hi` stays empty.
            let cold_count = if cfg!(miri) { 1_100u64 } else { 3_000u64 };
            let mut raw: Vec<(u64, DropTracker)> = Vec::new();
            let mut oracle: BTreeMap<u64, u64> = BTreeMap::new();
            for i in 0..cold_count {
                let k = cold_hi | (i * 4);
                raw.push((k, DropTracker::new(i, &live)));
                oracle.insert(k, i);
            }
            for i in 0..64u64 {
                let k = anchor_hi | i;
                raw.push((k, DropTracker::new(10_000 + i, &live)));
                oracle.insert(k, 10_000 + i);
            }

            let mut map = TesseraMap::from_vec_lazy_for_test(raw, true);
            // Refine `anchor_hi` completely, and crack `cold_hi` once (leaving unrefined
            // sub-buckets under a published nested TAG_RADIX).
            assert!(map.get(anchor_hi).is_some());
            assert!(map.get(cold_hi).is_some());
            assert!(map.stats().radixes >= 2);

            // Build an extend_vec batch (>= 256 items, all within root prefix) that refines every
            // remaining cold sub-bucket inside `cold_hi` AND targets the empty `empty_hi` bucket.
            let mut batch: Vec<(u64, DropTracker)> = Vec::new();
            let mut id = 50_000u64;
            for step in (0..cold_count).step_by(4) {
                let k = cold_hi | (step * 4 + 1);
                batch.push((k, DropTracker::new(id, &live)));
                oracle.insert(k, id);
                id += 1;
            }
            // Overwrite an existing key in `cold_hi` to verify last-value-wins.
            batch.push((cold_hi, DropTracker::new(id, &live)));
            oracle.insert(cold_hi, id);
            id += 1;
            // Target previously empty `empty_hi` bucket.
            for i in 0..32u64 {
                let k = empty_hi | (i * 3);
                batch.push((k, DropTracker::new(id, &live)));
                oracle.insert(k, id);
                id += 1;
            }
            assert!(batch.len() >= 256);

            map.extend_vec(batch);
            assert_eq!(map.len(), oracle.len());
            let mut got = Vec::new();
            map.range(.., |k, v| got.push((k, v._id)));
            let expected: Vec<(u64, u64)> = oracle.into_iter().collect();
            assert_eq!(got, expected);
        }
        assert_eq!(live.load(Ordering::Relaxed), 0);
    }
}

#[cfg(target_pointer_width = "64")]
#[test]
fn large_batch_ranges_and_radix_counts_do_not_truncate_at_u32_max() {
    let boundary = (u32::MAX as usize) + 1;
    let u = UnrefinedNode {
        slab_id: 0,
        start: 0,
        end: boundary,
        candidate_mask: u128::MAX,
    };
    assert_eq!(u.len(), boundary);

    let rx = RadixNode::<u8>::new(
        0,
        8,
        0,
        1,
        alloc::vec![NodeId::EMPTY; 2],
        alloc::vec![boundary, 7],
    );
    assert_eq!(rx.counts[0].load(Ordering::Relaxed), boundary);
    assert_eq!(rx.counts[1].load(Ordering::Relaxed), 7);
}
