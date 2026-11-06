// SPDX-License-Identifier: BSD-2-Clause
//! Vector scans for the `portable-simd` feature.
//!
//! Kernels are written once against `fearless_simd`'s [`Simd`] trait;
//! [`dispatch!`] picks the widest instruction set the CPU supports at run
//! time. Dispatch happens once per insertion sort, not once per scan: the
//! whole loop runs inside the dispatched call, so per-call dispatch overhead
//! does not accumulate over the thousands of scans a sort performs.
//!
//! The same generic code serves every key width and every backend.

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
pub(crate) trait SimdKey: Copy + crate::RadixKey {
    /// Offset of the first descending adjacent pair in `values`.
    fn scan_with<S: Simd>(simd: S, values: &[Self]) -> Option<usize>;
}

/// Stable insertion sort with a swap budget, dispatched once.
///
/// Returns the number of swaps, or `None` when the budget is exhausted; in
/// that case `arr` is a stable permutation of its input.
pub(crate) fn insertion_sort<K: SimdKey>(arr: &mut [K], budget: usize) -> Option<usize> {
    dispatch!(Level::new(), simd => insert_loop(simd, arr, budget))
}

/// The insertion loop, running inside the dispatched call.
///
/// Inlined into the dispatched call so its vector operations compile with the
/// selected target features.
#[inline(always)]
fn insert_loop<S: Simd, K: SimdKey>(simd: S, arr: &mut [K], budget: usize) -> Option<usize> {
    let mut swaps = 0;
    let mut start = 0;
    while let Some(offset) = first_inversion_with(simd, &arr[start..]) {
        let index = start + offset + 1;
        let (prefix, tail) = arr.split_at_mut(index);
        let (mut current, prefix) = prefix.split_last_mut()?;
        // The scan already compared this pair, so swap without re-reading keys.
        core::mem::swap(current, &mut tail[0]);
        swaps += 1;
        if swaps > budget {
            return None;
        }
        // The record being inserted keeps its key while it moves left.
        let current_key = current.encode();
        for before in prefix.iter_mut().rev() {
            if before.encode() <= current_key {
                break;
            }
            core::mem::swap(before, current);
            current = before;
            swaps += 1;
            if swaps > budget {
                return None;
            }
        }
        start = index;
    }
    Some(swaps)
}

/// Converts per-bucket counts to per-bucket starts, in place.
///
/// The scalar loop is a dependent chain of one add per bucket; a vector
/// prefix sum shortens the chain to log2(lanes) adds per lane group.
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

/// Offset of the first adjacent pair that descends in key order.
#[cfg(test)]
pub(crate) fn first_inversion<K: SimdKey>(values: &[K]) -> Option<usize> {
    dispatch!(Level::new(), simd => first_inversion_with(simd, values))
}

/// Scan under an already dispatched backend.
#[inline(always)]
fn first_inversion_with<S: Simd, K: SimdKey>(simd: S, values: &[K]) -> Option<usize> {
    if values.len() < SIMD_MIN {
        return scan_encoded(values);
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

/// Scalar scan in encoded order, for keys without a vector kernel.
#[inline]
fn scan_encoded<T: crate::RadixKey + Copy>(values: &[T]) -> Option<usize> {
    let mut values = values.iter();
    let mut previous = values.next()?.encode();
    for (index, value) in values.enumerate() {
        let current = value.encode();
        if previous > current {
            return Some(index);
        }
        previous = current;
    }
    None
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
                scan_encoded(values)
            }
        }
    )*};
}
wide_key!(u128, usize, i128, isize);

impl<K: SimdKey> SimdKey for core::cmp::Reverse<K> {
    #[inline(always)]
    fn scan_with<S: Simd>(_simd: S, values: &[Self]) -> Option<usize> {
        scan_encoded(values)
    }
}

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

    fn reference<T: Ord + Copy>(values: &[T]) -> Option<usize> {
        values.windows(2).position(|pair| pair[0] > pair[1])
    }

    #[test]
    fn integer_scans_find_the_first_inversion() {
        for length in [0usize, 1, 2, 31, 63, 64, 65, 127, 129, 1000] {
            let mut seed = 0x9e37_79b9u64;
            let mut words: Vec<u32> = (0..length)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    (seed >> 32) as u32
                })
                .collect();
            assert_eq!(first_inversion(&words), reference(&words));
            let wide: Vec<u64> = words.iter().map(|&value| u64::from(value) << 32).collect();
            assert_eq!(first_inversion(&wide), reference(&wide));
            if length > 2 {
                words.sort_unstable();
                words.swap(length - 1, length - 2);
            }
            assert_eq!(first_inversion(&words), reference(&words));
            let wide: Vec<u64> = words.iter().map(|&value| u64::from(value)).collect();
            assert_eq!(first_inversion(&wide), reference(&wide));
        }
    }

    #[test]
    fn float_scans_follow_total_order() {
        let values = [1.0f64, 0.5, -0.0, 0.0, f64::NAN, -f64::NAN];
        assert_eq!(first_inversion(&values), Some(0));
        let ascending = [-f64::NAN, -1.0, -0.0, 0.0, 1.0, f64::NAN];
        assert_eq!(first_inversion(&ascending), None);
        let mut inverted = ascending;
        inverted.swap(2, 3);
        assert_eq!(first_inversion(&inverted), Some(2));
    }

    #[test]
    fn insertion_sort_sorts_and_respects_the_budget() {
        let mut keys: Vec<u32> = (0..500).map(|index| (index * 37) % 500).collect();
        assert!(insertion_sort(&mut keys, usize::MAX).is_some());
        assert!(keys.windows(2).all(|pair| pair[0] <= pair[1]));
        let mut keys: Vec<u32> = (0..500).rev().collect();
        assert!(insertion_sort(&mut keys, 10).is_none());
    }
}
