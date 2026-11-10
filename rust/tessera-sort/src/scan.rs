// SPDX-License-Identifier: BSD-2-Clause

//! Bulk key scans: order checks, one-pass inspection and the vector kernels.
//!
//! [`inspect`] learns a slice's order and its varying key bits in one pass.
//! [`inversion`] locates the first descending adjacent pair, and
//! [`insertion::run_vector`](crate::insertion::run_vector) runs a whole
//! insertion pass with the same kernel. Kernels are written once against
//! [`fearless_simd`]'s [`Simd`] trait and [`dispatch!`] selects the widest
//! instruction set the CPU supports: once per insertion pass, and once per
//! order check or prefix sum.

use crate::insertion::Adjacency;
use crate::key::{Extract, RadixKey, Unsigned};
use core::marker::PhantomData;
use fearless_simd::prelude::*;
use fearless_simd::{Level, Simd, dispatch, u8x16, u16x8};

/// Pairs examined between mask checks.
const BLOCK: usize = 32;

/// Shortest slice worth a vector scan; below this the scalar loop wins.
pub(crate) const SIMD_MIN: usize = 64;

/// Pairs checked scalar before the vector kernel runs.
///
/// Most inversions in unsorted input sit within the first few pairs, and the
/// scalar check finds them without paying for a vector block.
const PEEL: usize = 4;

/// A key type whose slice can be scanned with [`fearless_simd`] vectors.
pub(crate) trait SimdKey: RadixKey {
    /// Offset of the first descending adjacent pair in `values`.
    fn scan_with<S: Simd>(simd: S, values: &[Self]) -> Option<usize>;
}

/// How a slice's adjacent keys descend, when they do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Descent {
    /// At least one adjacent pair ascends: the slice is not descending.
    NotDescending,
    /// Keys strictly decrease, so one reversal sorts the slice stably.
    Strict,
    /// Keys never increase and some pair ties, so equal runs reverse back.
    Tied,
}

/// What one pass over a slice learned about it.
pub(crate) struct Inspection {
    /// Key bits that differ between at least two records, within the candidate.
    pub(crate) varying: u128,
    /// Whether the records descend, and whether any adjacent pair ties.
    pub(crate) descent: Descent,
}

/// Classifies `records` and collects their varying bits in one pass.
///
/// Stops as soon as neither order survives and every candidate bit varies, so an
/// unordered random slice costs a few records while an ordered one costs a full
/// pass. `candidate` restricts the reported bits to those a later pass may
/// use, which also bounds the recursion for a changing key function.
///
/// Panics if `records` is empty or `candidate` is zero.
pub(crate) fn inspect<T, E: Extract<T>>(
    records: &[T],
    candidate: u128,
    extractor: &mut E,
) -> Inspection {
    debug_assert!(!records.is_empty() && candidate != 0);
    let full_mask = u128::MAX >> (128 - E::Value::BITS);
    let candidate = candidate & full_mask;
    let mut before = extractor.key(&records[0]);
    let (mut union, mut intersection) = (before, before);
    let (mut ascending, mut descending, mut strict) = (true, true, true);
    for record in &records[1..] {
        let after = extractor.key(record);
        union |= after;
        intersection = intersection & after;
        ascending &= before <= after;
        descending &= before >= after;
        strict &= before != after;
        let varying = union.to_u128() ^ intersection.to_u128();
        if !ascending && !descending && varying & candidate == candidate {
            break;
        }
        before = after;
    }
    let varying = (union.to_u128() ^ intersection.to_u128()) & candidate;
    let descent = if ascending {
        // Every pair ties at most: the caller's order check already excluded
        // an ascending slice, so this can only be a constant one.
        Descent::NotDescending
    } else if descending {
        if strict {
            Descent::Strict
        } else {
            Descent::Tied
        }
    } else {
        Descent::NotDescending
    };
    Inspection { varying, descent }
}

/// Offset of the first descending adjacent pair, with the vector kernel for
/// this call selected at run time.
///
/// One order check runs per slice, not per record, so the dispatch is amortized
/// even here; an insertion pass selects its kernel once for the whole pass.
pub(crate) fn inversion<K: SimdKey>(values: &[K]) -> Option<usize> {
    if values.len() < SIMD_MIN {
        return scalar(values);
    }
    dispatch!(Level::new(), simd => first_inversion_with(simd, values))
}

/// Offset of the first descending adjacent pair under an already selected backend.
#[inline(always)]
pub(crate) fn first_inversion_with<S: Simd, K: SimdKey>(simd: S, values: &[K]) -> Option<usize> {
    if values.len() < SIMD_MIN {
        return scalar(values);
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

/// Returns the offset of the first adjacent pair whose keys descend.
///
/// Scans one pair at a time and retains the previous key, so a user key
/// function runs once per record.
pub(crate) fn scalar<K: RadixKey>(keys: &[K]) -> Option<usize> {
    inverted(keys, |key| key.encode())
}

/// Returns the offset of the first adjacent pair whose keys descend, reading
/// keys through `extractor`.
pub(crate) fn scalar_by<T, E: Extract<T>>(records: &[T], extractor: &mut E) -> Option<usize> {
    inverted(records, |record| extractor.key(record))
}

/// Returns the offset of the first adjacent pair whose keys descend.
fn inverted<T, V: Ord>(records: &[T], mut key: impl FnMut(&T) -> V) -> Option<usize> {
    let mut records = records.iter();
    let mut previous = key(records.next()?);
    for (index, record) in records.enumerate() {
        let current = key(record);
        if previous > current {
            return Some(index);
        }
        previous = current;
    }
    None
}

/// Adjacency kernel that scans records-as-keys under one vector backend.
pub(crate) struct VectorScan<S, K> {
    simd: S,
    keys: PhantomData<fn() -> K>,
}

impl<S: Simd, K: SimdKey> VectorScan<S, K> {
    /// Wraps a backend token selected by [`dispatch!`].
    pub(crate) fn new(simd: S) -> Self {
        Self {
            simd,
            keys: PhantomData,
        }
    }
}

impl<S: Simd, K: SimdKey> Adjacency<K> for VectorScan<S, K> {
    type Value = K::Encoded;

    #[inline(always)]
    fn key(&mut self, record: &K) -> Self::Value {
        record.encode()
    }

    #[inline(always)]
    fn inversion(&mut self, values: &[K]) -> Option<usize> {
        first_inversion_with(self.simd, values)
    }
}

/// Converts per-bucket counts to per-bucket starts, in place.
///
/// The scalar loop is a dependent chain of one add per bucket; a vector prefix
/// sum shortens the chain to log2(lanes) adds per lane group. Counting passes
/// bound the total count by `u16::MAX`, so every start fits.
pub(crate) fn prefix_sum(counts: &mut [u16]) {
    dispatch!(Level::new(), simd => prefix_sum_with(simd, counts))
}

/// Prefix sum over `u16` counts under an already selected backend.
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
    T: SimdFloatElement + RadixKey,
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
    use crate::key::{ByKey, Direct};

    fn check_sorted_scans<K: SimdKey>(mut values: Vec<K>) {
        assert_eq!(inversion(&values), None);
        for level in [Level::baseline(), Level::new()] {
            assert_eq!(
                dispatch!(level, simd => first_inversion_with(simd, &values)),
                None
            );
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
                assert_eq!(inversion(&values), Some(index));
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
        check!(u8, u16, u32, u64, i8, i16, i32, i64);
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
    fn inspection_reports_order_and_variation() {
        let full = u128::MAX;
        let inspect_u64 = |values: &[u64]| super::inspect(values, full, &mut Direct);
        assert_eq!(inspect_u64(&[1, 1, 1]).varying, 0);
        assert_eq!(inspect_u64(&[1, 1, 1]).descent, Descent::NotDescending);
        assert_eq!(inspect_u64(&[3, 2, 1]).descent, Descent::Strict);
        assert_eq!(inspect_u64(&[3, 3, 1]).descent, Descent::Tied);
        assert_eq!(inspect_u64(&[1, 3, 2]).descent, Descent::NotDescending);
        // An unordered prefix covers the candidate and stops the pass.
        assert_eq!(inspect_u64(&[0, 1, 0, 0]).varying, 1);
        // Candidate bits outside the remaining difference are dropped.
        let values = [0u64, 1, 2, 3];
        assert_eq!(super::inspect(&values, 0b10, &mut Direct).varying, 0b10);
        let keyed = |values: &[(u64, u64)]| {
            super::inspect(values, full, &mut ByKey(|record: &(u64, u64)| record.0))
        };
        assert_eq!(keyed(&[(2, 0), (2, 1), (1, 2)]).descent, Descent::Tied);
    }

    #[test]
    fn prefix_sums() {
        for length in [0, 1, 7, 8, 9, 15, 16, 17, 31, 32, 257, 2048] {
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
}
