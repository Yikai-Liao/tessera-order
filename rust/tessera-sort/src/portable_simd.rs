// SPDX-License-Identifier: BSD-2-Clause

//! Vector scans for the `portable-simd` feature.
//!
//! Kernels are written once against `fearless_simd`'s [`Simd`] trait;
//! [`dispatch!`] picks the widest instruction set the CPU supports at run
//! time. Passes too short for a vector scan use scalar extraction. Other
//! passes select a backend once before the insertion loop, so dispatch cost
//! does not accumulate over the thousands of scans a sort performs.
//!
//! The same generic code serves every key width and every backend.

use crate::key::{ByKey, Extract};
use fearless_simd::prelude::*;
use fearless_simd::{Level, Simd, dispatch, u8x16, u16x8};

/// Pairs examined between mask checks.
const BLOCK: usize = 32;

/// Shortest slice worth a vector scan; below this the scalar loop wins.
const SIMD_MIN: usize = 64;

/// Pairs checked scalar before the vector kernel is used.
///
/// Most inversions in unsorted input sit within the first few pairs, and the
/// scalar check finds them without paying for a vector block.
const PEEL: usize = 4;

/// A key type whose slice can be scanned with `fearless_simd` vectors.
pub(crate) trait SimdKey: crate::RadixKey {
    /// Offset of the first descending adjacent pair in `values`.
    fn scan_with<S: Simd>(simd: S, values: &[Self]) -> Option<usize>;
}

/// Sorts `keys` stably by insertion, without a swap limit.
pub(crate) fn finish_insertion<K: SimdKey>(keys: &mut [K]) {
    // Every slice the scan sees is a suffix of `keys`, so a short pass never
    // reaches the vector kernel and skips dispatch.
    if keys.len() < SIMD_MIN {
        ByKey(|key: &K| *key).finish_insertion(keys);
        return;
    }
    dispatch!(Level::new(), simd => crate::engine::finish_insertion(keys, &mut VectorKeys(simd)));
}

/// Sorts `keys` stably by insertion within `budget` swaps, dispatched once.
///
/// Returns the number of swaps, or `None` when the budget is exhausted; in
/// that case `keys` is a stable permutation of its input.
pub(crate) fn insertion_sort<K: SimdKey>(keys: &mut [K], budget: usize) -> Option<usize> {
    if keys.len() < SIMD_MIN {
        return ByKey(|key: &K| *key).insertion_sort(keys, budget);
    }
    dispatch!(Level::new(), simd => crate::engine::insertion_loop(keys, budget, &mut VectorKeys(simd)))
}

/// Direct extraction with a backend already selected for this insertion pass.
struct VectorKeys<S>(S);

impl<S: Simd, K: SimdKey> crate::key::Extract<K> for VectorKeys<S> {
    type Value = K::Encoded;

    #[inline(always)]
    fn key(&mut self, record: &K) -> Self::Value {
        record.encode()
    }

    #[inline(always)]
    fn first_inversion(&mut self, values: &[K]) -> Option<usize> {
        first_inversion_with(self.0, values)
    }
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
    let mut chunks = counts.chunks_exact_mut(8);
    for chunk in &mut chunks {
        let mut scan = u16x8::<S>::from_slice(simd, chunk);
        scan = scan + scan.swizzle_dyn_precise(shift1);
        scan = scan + scan.swizzle_dyn_precise(shift2);
        scan = scan + scan.swizzle_dyn_precise(shift4);
        let starts = scan.swizzle_dyn_precise(shift1) + u16x8::<S>::splat(simd, carry);
        starts.store_slice(chunk);
        carry = carry.wrapping_add(scan.to_array()[7]);
    }
    let mut offset = carry;
    for slot in chunks.into_remainder() {
        let count = *slot;
        *slot = offset;
        offset = offset.wrapping_add(count);
    }
}

/// Confirms ascending direct keys after an already encoded predecessor.
pub(crate) fn confirm_ascending<K: SimdKey>(values: &[K], previous: K::Encoded) -> bool {
    if values.len() < SIMD_MIN {
        return crate::engine::confirm_ascending(values, &mut ByKey(|value: &K| *value), previous);
    }
    previous <= values[0].encode() && first_inversion(values).is_none()
}

/// Offset of the first adjacent pair that descends in key order.
pub(crate) fn first_inversion<K: SimdKey>(values: &[K]) -> Option<usize> {
    dispatch!(Level::new(), simd => first_inversion_with(simd, values))
}

/// Scan under an already dispatched backend.
#[inline(always)]
fn first_inversion_with<S: Simd, K: SimdKey>(simd: S, values: &[K]) -> Option<usize> {
    if values.len() < SIMD_MIN {
        return crate::scan::scalar(values);
    }
    let pairs = values.len() - 1;
    let peel = PEEL.min(pairs);
    // Keep the previous encoded key instead of encoding both sides of every
    // pair; float encoding is not free.
    let mut previous = values[0].encode();
    for (index, value) in values[1..=peel].iter().enumerate() {
        let current = value.encode();
        if previous > current {
            return Some(index);
        }
        previous = current;
    }
    K::scan_with(simd, &values[peel..]).map(|offset| offset + peel)
}

/// Offset of the first descending adjacent pair of integers.
#[inline(always)]
fn scan_int<S: Simd, T: SimdIntElement>(simd: S, values: &[T]) -> Option<usize> {
    let lanes = T::Native::<S>::LEN;
    let vectors = (BLOCK / lanes).max(1);
    let pairs = values.len() - 1;
    let mut index = 0;
    while index + vectors * lanes <= pairs {
        // One mask check per block; the bitmask also locates the lane, so a
        // block with an inversion needs no second pass over the keys.
        let mut bits = 0u64;
        for vector in 0..vectors {
            let at = index + vector * lanes;
            let left = T::Native::<S>::from_slice(simd, &values[at..at + lanes]);
            let right = T::Native::<S>::from_slice(simd, &values[at + 1..at + 1 + lanes]);
            bits |= left.simd_gt(right).to_bitmask() << (vector * lanes);
        }
        if bits != 0 {
            return Some(index + bits.trailing_zeros() as usize);
        }
        index += vectors * lanes;
    }
    while index < pairs {
        if values[index] > values[index + 1] {
            return Some(index);
        }
        index += 1;
    }
    None
}

/// Offset of the first descending adjacent pair of floats in total order.
#[inline(always)]
fn scan_float<S: Simd, T>(simd: S, values: &[T]) -> Option<usize>
where
    T: SimdFloatElement + crate::RadixKey,
    <T as SimdFloatElement>::Native<S>: SimdFloat<S, Element = T>,
{
    let lanes = T::Native::<S>::LEN;
    let vectors = (BLOCK / lanes).max(1);
    let pairs = values.len() - 1;
    let width = core::mem::size_of::<T>() * 8;
    // Flipping the sign bit orders non-negative values; inverting every bit
    // orders negative ones below them. That is exactly `total_cmp`.
    let scalar_sign = T::Bits::try_from(1u8).ok()? << (width - 1);
    let zero: <T::Native<S> as SimdFloat<S>>::Bits = T::Bits::default().simd_into(simd);
    let sign: <T::Native<S> as SimdFloat<S>>::Bits = scalar_sign.simd_into(simd);
    let encode = |value: T::Native<S>| {
        let bits = value.to_bits();
        let negative = bits >> (width as u32 - 1);
        bits ^ ((zero - negative) | sign)
    };
    let mut index = 0;
    while index + vectors * lanes <= pairs {
        let mut mask_bits = 0u64;
        for vector in 0..vectors {
            let at = index + vector * lanes;
            let left = encode(T::Native::<S>::from_slice(simd, &values[at..at + lanes]));
            let right = encode(T::Native::<S>::from_slice(
                simd,
                &values[at + 1..at + 1 + lanes],
            ));
            mask_bits |= left.simd_gt(right).to_bitmask() << (vector * lanes);
        }
        if mask_bits != 0 {
            return Some(index + mask_bits.trailing_zeros() as usize);
        }
        index += vectors * lanes;
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
            fn scan_with<S: Simd>(simd: S, values: &[Self]) -> Option<usize> {
                scan_int(simd, values)
            }
        }
    )*};
}
integer_key!(u8, u16, u32, u64, i8, i16, i32, i64);

/// Keys wider than a vector register reuse the scalar scan.
macro_rules! wide_key {
    ($($ty:ty),* $(,)?) => {$(
        impl SimdKey for $ty {
            #[inline(always)]
            fn scan_with<S: Simd>(_simd: S, values: &[Self]) -> Option<usize> {
                crate::scan::scalar(values)
            }
        }
    )*};
}
wide_key!(u128, usize, i128, isize);

impl SimdKey for f32 {
    #[inline(always)]
    fn scan_with<S: Simd>(simd: S, values: &[Self]) -> Option<usize> {
        scan_float(simd, values)
    }
}

impl SimdKey for f64 {
    #[inline(always)]
    fn scan_with<S: Simd>(simd: S, values: &[Self]) -> Option<usize> {
        scan_float(simd, values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_sorted_scans<K: SimdKey>(mut values: Vec<K>) {
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
            assert_eq!(K::first_inversion(&values), None);
            for index in 0..values.len().saturating_sub(1) {
                // Miri still crosses the peel, vector blocks and scalar tail.
                if cfg!(miri) && index % 16 > 1 && index != 4 && index + 2 != values.len() {
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
                assert_eq!(K::first_inversion(&values), Some(index));
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
            &[0, 1, 64, 65]
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
    }

    #[test]
    fn float_scans() {
        let lengths: &[usize] = if cfg!(miri) {
            &[0, 1, 64, 65]
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
                    check_sorted_scans(values);
                }
            }};
        }
        check!(f32, u32);
        check!(f64, u64);
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
    fn insertion_budget() {
        for length in [0u32, 1, 48, 63, 64, 65, if cfg!(miri) { 97 } else { 500 }] {
            let mut keys: Vec<_> = (0..length).map(|index| (index * 37) % length).collect();
            let mut expected = keys.clone();
            expected.sort_unstable();
            finish_insertion(&mut keys);
            assert_eq!(keys, expected);

            for budget in [0, 10, usize::MAX] {
                let mut keys: Vec<_> = (0..length).rev().collect();
                let swaps = length as usize * length.saturating_sub(1) as usize / 2;
                assert_eq!(
                    insertion_sort(&mut keys, budget),
                    (swaps <= budget).then_some(swaps),
                );
                keys.sort_unstable();
                assert_eq!(keys, (0..length).collect::<Vec<_>>());
            }
        }
    }
}
