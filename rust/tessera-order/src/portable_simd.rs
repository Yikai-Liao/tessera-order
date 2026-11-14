// SPDX-License-Identifier: BSD-2-Clause

//! Vector scans and prefix sums, with per-operation instruction-set dispatch.
//!
//! Kernels are written once against `fearless_simd`'s [`Simd`] trait;
//! [`dispatch!`] picks the widest instruction set the CPU supports at run
//! time. Passes too short for a vector scan use scalar extraction. Other
//! passes select a backend once before the insertion loop, so dispatch cost
//! does not accumulate over the thousands of scans a sort performs.
//!
//! The same generic code serves every key width and every backend.

use crate::RadixKey;
use crate::key::Direct;
use fearless_simd::prelude::*;
use fearless_simd::{Level, Simd, dispatch, u8x16, u16x8};

/// Shortest slice worth a vector scan; below this the scalar loop wins.
const SIMD_MIN: usize = 64;

/// Pairs checked scalar before the vector kernel is used.
///
/// Most inversions in unsorted input sit within the first few pairs, and the
/// scalar check finds them without paying for a vector block.
const PEEL: usize = 4;

/// Native adjacent-pair comparison for direct keys; also seals [`RadixKey`].
///
/// The scan owns traversal and mask aggregation. Each key owns only the
/// backend's lane count and comparison in its encoded order. One lane denotes
/// a scalar fallback, used by wide and pointer-sized keys.
pub trait SimdKey: Copy {
    /// Number of pairs compared by [`Self::pair_mask`] under this backend,
    /// from one through 64, matching the backend's native vector width.
    fn lanes<S: Simd>() -> usize {
        1
    }

    /// Bit i is set exactly when left[i]'s encoded key exceeds right[i]'s,
    /// for the first [`Self::lanes`] pairs; all higher bits are zero.
    /// Both slices contain at least that many records.
    fn pair_mask<S: Simd>(_simd: S, left: &[Self], right: &[Self]) -> u64
    where
        Self: RadixKey,
    {
        u64::from(left[0].encode() > right[0].encode())
    }
}

/// Sorts `keys` stably by insertion, without a swap limit.
pub(crate) fn finish_insertion<K: RadixKey>(keys: &mut [K]) {
    // Every slice the scan sees is a suffix of `keys`, so a short pass never
    // reaches the vector kernel and skips dispatch.
    if keys.len() < SIMD_MIN {
        crate::order::finish_insertion(keys, &mut Direct, crate::order::by_key);
        return;
    }
    dispatch!(Level::new(), simd => crate::order::finish_insertion(keys, &mut Direct, |values, _| first_inversion_with(simd, values)));
}

/// Tries budgeted insertion, selecting the scan once for the complete pass.
pub(crate) fn try_insertion<K: RadixKey>(keys: &mut [K], tile_len: usize) -> bool {
    if keys.len() < SIMD_MIN {
        return crate::order::try_insertion(keys, tile_len, &mut Direct, crate::order::by_key);
    }
    dispatch!(Level::new(), simd => crate::order::try_insertion(keys, tile_len, &mut Direct, |values, _| first_inversion_with(simd, values)))
}

/// Converts per-bucket counts to per-bucket starts, in place.
///
/// The scalar loop is a dependent chain of one add per bucket; a vector
/// prefix sum shortens the chain to log2(lanes) adds per lane group.
/// Counting passes bound the total count by `u16::MAX`, so every start fits.
pub(crate) fn prefix_sum(counts: &mut [u16]) {
    dispatch!(Level::new(), simd => prefix_sum_with(simd, counts))
}

#[inline(always)]
fn prefix_sum_with<S: Simd>(simd: S, counts: &mut [u16]) {
    const SHIFT1: [u8; 16] = [0xFF, 0xFF, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
    const SHIFT2: [u8; 16] = [0xFF, 0xFF, 0xFF, 0xFF, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    const SHIFT4: [u8; 16] = [
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0, 1, 2, 3, 4, 5, 6, 7,
    ];
    let shift1 = u8x16::<S>::from_slice(simd, &SHIFT1);
    let shift2 = u8x16::<S>::from_slice(simd, &SHIFT2);
    let shift4 = u8x16::<S>::from_slice(simd, &SHIFT4);
    let mut carry = 0u16;
    let (chunks, remainder) = counts.as_chunks_mut::<8>();
    for chunk in chunks {
        let mut scan = u16x8::<S>::from_slice(simd, chunk);
        scan = scan + scan.swizzle_dyn_precise(shift1);
        scan = scan + scan.swizzle_dyn_precise(shift2);
        scan = scan + scan.swizzle_dyn_precise(shift4);
        let starts = scan.swizzle_dyn_precise(shift1) + u16x8::<S>::splat(simd, carry);
        starts.store_slice(chunk);
        carry = carry.wrapping_add(scan.to_array()[7]);
    }
    let mut offset = carry;
    for slot in remainder {
        let count = *slot;
        *slot = offset;
        offset = offset.wrapping_add(count);
    }
}

/// Confirms ascending direct keys after an already encoded predecessor.
// Keep short scalar suffixes in the caller; see
// docs/design/evidence.md#shared-ordered-finishing-controls.
#[inline]
pub(crate) fn confirm_ascending<K: RadixKey>(values: &[K], previous: K::Encoded) -> bool {
    if values.len() < SIMD_MIN {
        return crate::order::confirm_ascending(values, &mut Direct, previous);
    }
    previous <= values[0].encode() && first_inversion(values).is_none()
}

/// Offset of the first adjacent pair that descends in key order.
pub(crate) fn first_inversion<K: RadixKey>(values: &[K]) -> Option<usize> {
    dispatch!(Level::new(), simd => first_inversion_with(simd, values))
}

/// Scan under an already dispatched backend.
#[inline(always)]
fn first_inversion_with<S: Simd, K: RadixKey>(simd: S, values: &[K]) -> Option<usize> {
    if values.len() < SIMD_MIN {
        return crate::order::by_key(values, &mut Direct);
    }
    let peel = PEEL.min(values.len() - 1);
    if let Some(index) = crate::order::by_key(&values[..=peel], &mut Direct) {
        return Some(index);
    }
    scan_pairs(simd, &values[peel..]).map(|offset| offset + peel)
}

/// One scan for every direct encoding, using explicit fearless SIMD pairs.
#[inline(always)]
fn scan_pairs<S: Simd, K: RadixKey>(simd: S, values: &[K]) -> Option<usize> {
    const BLOCK: usize = 32;
    let lanes = K::lanes::<S>();
    if lanes == 1 {
        return crate::order::by_key(values, &mut Direct);
    }
    let vectors = (BLOCK / lanes).max(1);
    let block_len = vectors * lanes;
    let pairs = values.len().saturating_sub(1);
    let mut index = 0;
    while index + block_len <= pairs {
        // A block holds 32 pairs or one wider native vector, at most 64 pairs.
        // One mask check locates the first inversion without rereading keys.
        let block = &values[index..=index + block_len];
        let mut bits = 0u64;
        for vector in 0..vectors {
            let at = vector * lanes;
            bits |=
                K::pair_mask(simd, &block[at..at + lanes], &block[at + 1..at + 1 + lanes]) << at;
        }
        if bits != 0 {
            return Some(index + bits.trailing_zeros() as usize);
        }
        index += block_len;
    }
    while index < pairs {
        if values[index].encode() > values[index + 1].encode() {
            return Some(index);
        }
        index += 1;
    }
    None
}

macro_rules! integer_key {
    ($($ty:ty),* $(,)?) => {$(
        impl SimdKey for $ty {
            #[inline(always)]
            fn lanes<S: Simd>() -> usize {
                <Self as SimdIntElement>::Native::<S>::LEN
            }

            #[inline(always)]
            fn pair_mask<S: Simd>(simd: S, left: &[Self], right: &[Self]) -> u64 {
                let left = <Self as SimdIntElement>::Native::<S>::from_slice(simd, left);
                let right = <Self as SimdIntElement>::Native::<S>::from_slice(simd, right);
                // Native signed comparison has the same order as sign-flip encoding.
                left.simd_gt(right).to_bitmask()
            }
        }
    )*};
}
integer_key!(u8, u16, u32, u64, i8, i16, i32, i64);

macro_rules! float_key {
    ($(($f:ty, $u:ty)),* $(,)?) => {$(
        impl SimdKey for $f {
            #[inline(always)]
            fn lanes<S: Simd>() -> usize {
                <Self as SimdFloatElement>::Native::<S>::LEN
            }

            #[inline(always)]
            fn pair_mask<S: Simd>(simd: S, left: &[Self], right: &[Self]) -> u64 {
                let sign = (1 as $u) << (<$u>::BITS - 1);
                let encode = |values: &[Self]| {
                    let bits = <Self as SimdFloatElement>::Native::<S>::from_slice(simd, values).to_bits();
                    let zero: <<Self as SimdFloatElement>::Native<S> as SimdFloat<S>>::Bits = (0 as $u).simd_into(simd);
                    let sign: <<Self as SimdFloatElement>::Native<S> as SimdFloat<S>>::Bits = sign.simd_into(simd);
                    // The sign is zero or one; subtraction expands it to
                    // zero or all ones, preserving total_cmp including NaN bits.
                    bits ^ ((zero - (bits >> (<$u>::BITS - 1))) | sign)
                };
                encode(left).simd_gt(encode(right)).to_bitmask()
            }
        }
    )*};
}
float_key!((f32, u32), (f64, u64));

macro_rules! scalar_key {
    ($($ty:ty),* $(,)?) => {$(impl SimdKey for $ty {})*};
}
scalar_key!(u128, usize, i128, isize);

impl<K: RadixKey> SimdKey for core::cmp::Reverse<K> {
    #[inline(always)]
    fn lanes<S: Simd>() -> usize {
        K::lanes::<S>()
    }

    #[inline(always)]
    fn pair_mask<S: Simd>(simd: S, left: &[Self], right: &[Self]) -> u64 {
        // SAFETY: Reverse<K> is repr(transparent) over K, so both shared views
        // have the same layout, alignment, validity and lifetimes as the inputs.
        let (left, right) = unsafe {
            (
                core::slice::from_raw_parts(left.as_ptr().cast::<K>(), left.len()),
                core::slice::from_raw_parts(right.as_ptr().cast::<K>(), right.len()),
            )
        };
        // Complementing both encoded keys reverses >, including equal keys.
        K::pair_mask(simd, right, left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_sorted_scans<K: RadixKey>(mut values: Vec<K>) {
        if let Some(first) = values.first() {
            assert!(confirm_ascending(&values, first.encode()));
            if let Some(larger) = values.iter().find(|value| value.encode() > first.encode()) {
                assert!(!confirm_ascending(&values, larger.encode()));
            }
        }
        for level in [Level::baseline(), Level::new()] {
            assert_eq!(
                dispatch!(level, simd => first_inversion_with(simd, &values)),
                None
            );
            assert_eq!(first_inversion(&values), None);
            for index in 0..values.len().saturating_sub(1) {
                // Native tests move an inversion through every pair. Miri
                // keeps the peel, both 32- and 64-pair block edges and last
                // tail pair, for every encoding, backend and Reverse wrapper.
                if cfg!(miri)
                    && ![0, 3, 4, 35, 36, 67, 68, values.len().saturating_sub(2)].contains(&index)
                {
                    continue;
                }
                if values[index].encode() == values[index + 1].encode() {
                    continue;
                }
                values.swap(index, index + 1);
                assert_eq!(
                    dispatch!(level, simd => first_inversion_with(simd, &values)),
                    Some(index),
                    "level={level:?} len={} index={index}",
                    values.len()
                );
                assert_eq!(first_inversion(&values), Some(index));
                if !cfg!(miri) || index == 4 {
                    assert!(!confirm_ascending(&values, values[0].encode()));
                }
                values.swap(index, index + 1);
            }
        }
    }

    #[test]
    fn integer_scans() {
        let lengths: &[usize] = if cfg!(miri) {
            // One long slice reaches even a 64-pair vector plus its scalar tail.
            &[0, 1, 97]
        } else {
            &[0, 1, 2, 31, 63, 64, 65, 97, 129]
        };
        macro_rules! check {
            ($($ty:ty),*) => {$(
                for &length in lengths {
                    let mut seed = 17u64;
                    let mut values: Vec<$ty> = (0..length).map(|_| {
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                        seed as $ty
                    }).collect();
                    values.sort();
                    check_sorted_scans(values);
                }
            )*};
        }
        check!(
            u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
        );
        macro_rules! check_reverse {
            ($($ty:ty),*) => {$(
                for &length in lengths {
                    let mut seed = 43u64;
                    let mut values: Vec<$ty> = (0..length).map(|_| {
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                        seed as $ty
                    }).collect();
                    values.sort();
                    check_sorted_scans(values.iter().rev().copied().map(core::cmp::Reverse).collect());
                    check_sorted_scans(values.into_iter().map(|key| core::cmp::Reverse(core::cmp::Reverse(key))).collect());
                }
            )*};
        }
        check_reverse!(
            u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
        );
    }

    #[test]
    fn float_scans() {
        let lengths: &[usize] = if cfg!(miri) {
            &[0, 1, 97]
        } else {
            &[0, 1, 2, 31, 63, 64, 65, 97, 129]
        };
        macro_rules! check {
            ($f:ty, $u:ty) => {{
                for &length in lengths {
                    let mut seed = 91u64;
                    let mut values: Vec<$f> = (0..length)
                        .map(|_| {
                            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                            <$f>::from_bits(seed as $u)
                        })
                        .collect();
                    values.extend([
                        -<$f>::NAN,
                        <$f>::NEG_INFINITY,
                        -1.0,
                        -0.0,
                        0.0,
                        <$f>::from_bits(1),
                        1.0,
                        <$f>::INFINITY,
                        <$f>::NAN,
                    ]);
                    values.sort_by(<$f>::total_cmp);
                    check_sorted_scans(values.clone());
                    check_sorted_scans(
                        values
                            .iter()
                            .rev()
                            .copied()
                            .map(core::cmp::Reverse)
                            .collect(),
                    );
                    check_sorted_scans(
                        values
                            .into_iter()
                            .map(|key| core::cmp::Reverse(core::cmp::Reverse(key)))
                            .collect(),
                    );
                }
            }};
        }
        check!(f32, u32);
        check!(f64, u64);
    }

    #[test]
    fn earliest_of_multiple_inversions() {
        for level in [Level::baseline(), Level::new()] {
            for (first, later) in [(4, 12), (7, 33), (31, 65), (35, 67), (67, 95)] {
                let mut keys: Vec<u64> = (0..129).collect();
                keys.swap(first, first + 1);
                keys.swap(later, later + 1);
                assert_eq!(
                    dispatch!(level, simd => first_inversion_with(simd, &keys)),
                    Some(first),
                    "level={level:?} first={first} later={later}"
                );
                let keys: Vec<_> = keys
                    .into_iter()
                    .map(|key| core::cmp::Reverse(!key))
                    .collect();
                assert_eq!(
                    dispatch!(level, simd => first_inversion_with(simd, &keys)),
                    Some(first)
                );
            }
        }
    }

    #[test]
    fn prefix_sums() {
        for length in [0, 1, 7, 8, 9, 15, 16, 17, 31, 32, 257, 16384] {
            if cfg!(miri) && length > 257 {
                continue;
            }
            for shape in 0..3 {
                let counts: Vec<u16> = (0..length)
                    .map(|index| match shape {
                        0 => 0,
                        1 => {
                            if index == length / 2 {
                                u16::MAX
                            } else {
                                0
                            }
                        }
                        _ => ((index * 37 + 11) % 4) as u16,
                    })
                    .collect();
                let mut total = 0u32;
                let expected: Vec<u16> = counts
                    .iter()
                    .map(|&count| {
                        let start = total;
                        total += u32::from(count);
                        assert!(total <= u32::from(u16::MAX));
                        start as u16
                    })
                    .collect();
                for level in [Level::baseline(), Level::new()] {
                    let mut actual = counts.clone();
                    dispatch!(level, simd => prefix_sum_with(simd, &mut actual));
                    assert_eq!(
                        actual, expected,
                        "level={level:?} len={length} shape={shape}"
                    );
                }
            }
        }
    }

    #[test]
    fn insertion_encodings() {
        use crate::key::{ByKey, Extract};

        fn check<K: RadixKey>(keys: Vec<K>)
        where
            K::Encoded: core::fmt::Debug,
        {
            let encoded = |keys: &[K]| keys.iter().copied().map(K::encode).collect::<Vec<_>>();
            for tile_len in [2, 16, 64, keys.len()] {
                let mut direct = keys.clone();
                let mut scalar = keys.clone();
                assert_eq!(
                    try_insertion(&mut direct, tile_len),
                    ByKey(|key: &K| *key).try_insertion(&mut scalar, tile_len),
                );
                assert_eq!(encoded(&direct), encoded(&scalar));
            }
            let mut expected = encoded(&keys);
            expected.sort_unstable();
            let mut finished = keys;
            finish_insertion(&mut finished);
            assert_eq!(encoded(&finished), expected);
        }

        // Cross the dispatch threshold with repeated keys and exact float bits,
        // including both NaN signs and signed zeros. Partial passes must agree
        // with callback extraction as well as complete insertion finishing.
        for length in [63, 64, 65, 97] {
            let bits = [
                0,
                1 << 63,
                u64::MAX,
                0x7ff8_0000_0000_0001,
                1,
                0xfff0_0000_0000_0000,
            ];
            let keys: Vec<_> = (0..length)
                .map(|index| f64::from_bits(bits[(index * 5) % bits.len()]))
                .collect();
            check(keys.clone());
            check(keys.into_iter().map(core::cmp::Reverse).collect());
            check(
                (0..length)
                    .map(|index| ((index % 7) as u128) << 80)
                    .collect(),
            );
        }
    }

    #[test]
    fn insertion_continuation_with_encoded_keys() {
        use crate::key::{ByKey, Extract};

        fn check<K: RadixKey>(mut keys: Vec<K>)
        where
            K::Encoded: core::fmt::Debug,
        {
            keys.sort_by_key(|key| K::encode(*key));
            let expected: Vec<_> = keys.iter().copied().map(K::encode).collect();
            for index in (0..keys.len() - 1).step_by(8) {
                keys.swap(index, index + 1);
            }
            let mut direct = keys.clone();
            let mut scalar = keys;
            assert!(try_insertion(&mut direct, 64));
            assert!(ByKey(|key: &K| *key).try_insertion(&mut scalar, 64));
            assert_eq!(
                direct.iter().copied().map(K::encode).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                scalar.iter().copied().map(K::encode).collect::<Vec<_>>(),
                expected
            );
        }

        let mut floats: Vec<_> = (1..=193).map(f64::from_bits).collect();
        floats[..5].copy_from_slice(&[
            -0.0,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::from_bits(0xfff8_0000_0000_0001),
            f64::from_bits(0x7ff8_0000_0000_0001),
        ]);
        check(floats.clone());
        check(floats.into_iter().map(core::cmp::Reverse).collect());
        check((0..193u128).map(|key| (key << 80) | (key * 31)).collect());
    }

    #[test]
    fn insertion_budget() {
        for length in [0u32, 1, 48, 63, 64, 65, if cfg!(miri) { 97 } else { 500 }] {
            let mut keys: Vec<_> = (0..length).map(|index| (index * 37) % length).collect();
            let mut expected = keys.clone();
            expected.sort_unstable();
            finish_insertion(&mut keys);
            assert_eq!(keys, expected);

            for tile_len in [2, 16, 64, length as usize] {
                let mut keys: Vec<_> = (0..length).rev().collect();
                let finished = try_insertion(&mut keys, tile_len);
                if finished {
                    assert_eq!(keys, (0..length).collect::<Vec<_>>());
                }
                keys.sort_unstable();
                assert_eq!(keys, (0..length).collect::<Vec<_>>());
            }
        }
    }
}
