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

use super::key::RadixKey;
use crate::engine::key::Direct;
use fearless_simd::prelude::*;
use fearless_simd::{Level, Simd, dispatch, u8x16, u16x8};

/// Shortest slice worth a vector scan or SIMD MSD finish; below this scalar wins.
pub(crate) const SIMD_MIN: usize = 64;

/// Pairs checked scalar before the vector kernel is used.
///
/// Most inversions in unsorted input sit within the first few pairs, and the
/// scalar check finds them without paying for a vector block.
const PEEL: usize = 4;

/// Shortest native vector width for SIMD odd-even MSD finishing.
/// Narrower vectors (e.g. `u64x4` on AVX2 or `u32x4` on 128-bit backends)
/// cover too few bucket sizes (`<= w + 1`) to amortize deinterleave/interleave
/// overhead and lack native 64-bit unsigned min/max on AVX2.
const MSD_MIN_LANES: usize = 8;

/// Native adjacent-pair comparison and encoded-order vector load/store for
/// direct keys.
///
/// `key::sealed::Sealed` seals [`RadixKey`] over this trait rather than
/// exposing `SimdKey` directly as a supertrait. The kernels own traversal,
/// mask aggregation and sorting networks. Each key owns only the backend's
/// lane count and its total-order vector encoding. One lane denotes a scalar
/// fallback, used by wide and tuple keys.
pub trait SimdKey: Copy {
    /// Native integer vector holding encoded keys in total order; unused when
    /// [`Self::lanes`] is 1.
    type Vec<S: Simd>: SimdInt<S>;

    /// Number of lanes processed per vector under this backend, from one
    /// through 64, matching the backend's native vector width.
    #[inline(always)]
    fn lanes<S: Simd>() -> usize {
        1
    }

    /// Largest representable lane value in [`Self::Vec`], used as sentinel
    /// padding during odd-even compare-exchange steps.
    fn max_lane<S: Simd>() -> <Self::Vec<S> as SimdBase<S>>::Element {
        unreachable!("max_lane called on scalar SimdKey")
    }

    /// Loads [`Self::lanes`] keys into a vector whose native integer order
    /// matches `Self`'s key order. Only called when `lanes > 1`.
    fn load_encoded<S: Simd>(_simd: S, _slice: &[Self]) -> Self::Vec<S> {
        unreachable!("load_encoded called on scalar SimdKey")
    }

    /// Broadcasts one key into all lanes of [`Self::Vec`] in encoded order.
    /// Only called when `lanes > 1`.
    fn splat_encoded<S: Simd>(_simd: S, _key: Self) -> Self::Vec<S> {
        unreachable!("splat_encoded called on scalar SimdKey")
    }

    /// Decodes and stores [`Self::lanes`] keys loaded by [`Self::load_encoded`],
    /// preserving exact bit patterns. Only called when `lanes > 1`.
    fn store_decoded<S: Simd>(_vec: Self::Vec<S>, _slice: &mut [Self]) {
        unreachable!("store_decoded called on scalar SimdKey")
    }

    /// Lane `i` is true iff `left[i]`'s encoded key exceeds `right[i]`'s,
    /// for the first [`Self::lanes`] pairs. Both slices have exact length
    /// [`Self::lanes`]; only called when `lanes > 1`.
    #[inline(always)]
    fn pair_gt<S: Simd>(
        simd: S,
        left: &[Self],
        right: &[Self],
    ) -> <Self::Vec<S> as SimdBase<S>>::Mask {
        Self::load_encoded(simd, left).simd_gt(Self::load_encoded(simd, right))
    }
}

/// Provisional stream count for [`finish_msd_with`], kept small enough to stay
/// within AVX2's 16 vector registers without spilling.
const MSD_STREAMS: usize = 4;

/// Shared eligibility rule for both planning and finishing: require a real SIMD
/// backend (never scalar `Fallback` emulation) with at least [`MSD_MIN_LANES`].
#[inline(always)]
fn msd_supported<S: Simd, K: SimdKey>(simd: S) -> bool {
    !simd.level().is_fallback() && K::lanes::<S>() >= MSD_MIN_LANES
}

/// Whether the active backend supports SIMD MSD finishing for `K`.
pub(crate) fn can_finish_msd<K: RadixKey>() -> bool {
    dispatch!(Level::new(), simd => msd_supported::<_, K>(simd))
}

/// Finishes MSD-partitioned keys in place via a branchless odd-even vector
/// network when every bucket has at most `largest` records and fits within one
/// vector of overlap. Returns false without changing keys when ineligible.
#[inline]
pub(crate) fn finish_msd<K: RadixKey>(keys: &mut [K], largest: usize) -> bool {
    keys.len() >= SIMD_MIN && finish_msd_simd(keys, largest)
}

#[inline(never)]
fn finish_msd_simd<K: RadixKey>(keys: &mut [K], largest: usize) -> bool {
    dispatch!(Level::new(), simd => finish_msd_with(simd, keys, largest))
}

/// Repairs `2 * w` overlapping windows in [`MSD_STREAMS`] independent streams
/// when each stream can hold two vectors, then repairs their seams and any
/// tail. Shorter slices use one overlapping sweep. Every bucket of length
/// `<= w + 1` fits completely inside at least one window.
#[inline(always)]
fn finish_msd_with<S: Simd, K: RadixKey>(simd: S, keys: &mut [K], largest: usize) -> bool {
    let (w, len) = (K::lanes::<S>(), keys.len());
    if !msd_supported::<S, K>(simd) || len < 2 * w || largest > w + 1 {
        return false;
    }
    if largest <= 1 {
        return true;
    }
    let max_v = K::Vec::<S>::splat(simd, K::max_lane::<S>());
    let lane0 = K::Vec::<S>::from_fn(simd, |i| <K::Vec<S> as SimdBase<S>>::Element::from(i == 0))
        .simd_ne(<K::Vec<S> as SimdBase<S>>::Element::from(false));
    macro_rules! sort_pair {
        ($a:expr, $b:expr) => {{
            let (mut even, mut odd) = ($a).deinterleave($b);
            for _ in 0..largest / 2 {
                let (e, o) = (even.min(odd), even.max(odd));
                let e_next = e.slide::<1>(max_v);
                odd = o.min(e_next);
                even = lane0.select(e, o.max(e_next).rotate_elements_right::<1>());
            }
            if largest & 1 != 0 {
                (even, odd) = (even.min(odd), even.max(odd));
            }
            even.interleave(odd)
        }};
    }
    macro_rules! repair {
        ($win:expr) => {{
            let win = $win;
            let (lo, hi) = sort_pair!(
                K::load_encoded(simd, &win[..w]),
                K::load_encoded(simd, &win[w..2 * w])
            );
            K::store_decoded::<S>(lo, &mut win[..w]);
            K::store_decoded::<S>(hi, &mut win[w..2 * w]);
        }};
    }
    let mut pos = 0;
    if len >= 2 * MSD_STREAMS * w {
        let seg = (len / (MSD_STREAMS * w)) * w;
        let mut carry = [max_v; MSD_STREAMS];
        for (i, slot) in carry.iter_mut().enumerate() {
            *slot = K::load_encoded(simd, &keys[i * seg..i * seg + w]);
        }
        let mut off = w;
        while off < seg {
            for (i, slot) in carry.iter_mut().enumerate() {
                let s = i * seg + off;
                // Read this stream's next vector before overwriting its consumed
                // predecessor. Other streams occupy disjoint `seg` ranges;
                // seam repair starts only after every carry has been stored.
                let (lo, hi) = sort_pair!(*slot, K::load_encoded(simd, &keys[s..s + w]));
                K::store_decoded::<S>(lo, &mut keys[s - w..s]);
                *slot = hi;
            }
            off += w;
        }
        for (i, &slot) in carry.iter().enumerate() {
            K::store_decoded::<S>(slot, &mut keys[(i + 1) * seg - w..(i + 1) * seg]);
        }
        for i in 1..MSD_STREAMS {
            repair!(&mut keys[i * seg - w..i * seg + w]);
        }
        pos = MSD_STREAMS * seg;
    }
    if pos < len {
        let mut p = pos.saturating_sub(w);
        while p + 2 * w <= len {
            repair!(&mut keys[p..p + 2 * w]);
            p += w;
        }
        if p + w != len {
            repair!(&mut keys[len - 2 * w..len]);
        }
    }
    true
}

/// Sorts `keys` stably by insertion, without a swap limit.
pub(crate) fn finish_insertion<K: RadixKey>(keys: &mut [K]) {
    // Every slice the scan sees is a suffix of `keys`, so a short pass never
    // reaches the vector kernel and skips dispatch.
    if keys.len() < SIMD_MIN {
        crate::engine::order::finish_insertion(keys, &mut Direct, crate::engine::order::by_key);
        return;
    }
    finish_insertion_simd(keys);
}

// Keep `dispatch!` (and its non-target-feature fallback arm) out of short-input
// callers such as `refine_with_tile_bytes`.
#[inline(never)]
fn finish_insertion_simd<K: RadixKey>(keys: &mut [K]) {
    dispatch!(Level::new(), simd => crate::engine::order::finish_insertion(keys, &mut Direct, |values, _| first_inversion_with(simd, values)));
}

/// Sorts `keys` stably by insertion within `budget` swaps, dispatched once.
///
/// Returns the number of swaps, or `None` when the budget is exhausted; in
/// that case `keys` is a stable permutation of its input.
pub(crate) fn insertion_sort<K: RadixKey>(keys: &mut [K], budget: usize) -> Option<usize> {
    if keys.len() < SIMD_MIN {
        return crate::engine::order::insertion_loop(
            keys,
            budget,
            &mut Direct,
            crate::engine::order::by_key,
        );
    }
    insertion_sort_simd(keys, budget)
}

#[inline(never)]
fn insertion_sort_simd<K: RadixKey>(keys: &mut [K], budget: usize) -> Option<usize> {
    dispatch!(Level::new(), simd => crate::engine::order::insertion_loop(keys, budget, &mut Direct, |values, _| first_inversion_with(simd, values)))
}

/// Converts per-bucket counts to per-bucket starts in place, returning the
/// largest bucket count observed before prefixing.
///
/// The scalar loop is a dependent chain of one add per bucket; a vector
/// prefix sum shortens the chain to log2(lanes) adds per lane group while
/// tracking the maximum count with one SIMD max per chunk.
/// Counting passes bound the total count by `u16::MAX`, so every start fits.
pub(crate) fn prefix_sum(counts: &mut [u16]) -> u16 {
    dispatch!(Level::new(), simd => prefix_sum_with(simd, counts))
}

#[inline(always)]
fn prefix_sum_with<S: Simd>(simd: S, counts: &mut [u16]) -> u16 {
    const SHIFT1: [u8; 16] = [0xFF, 0xFF, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
    const SHIFT2: [u8; 16] = [0xFF, 0xFF, 0xFF, 0xFF, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    const SHIFT4: [u8; 16] = [
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0, 1, 2, 3, 4, 5, 6, 7,
    ];
    const LAST: [u8; 16] = [
        14, 15, 14, 15, 14, 15, 14, 15, 14, 15, 14, 15, 14, 15, 14, 15,
    ];
    let shift1 = u8x16::<S>::from_slice(simd, &SHIFT1);
    let shift2 = u8x16::<S>::from_slice(simd, &SHIFT2);
    let shift4 = u8x16::<S>::from_slice(simd, &SHIFT4);
    let last = u8x16::<S>::from_slice(simd, &LAST);
    let mut carry = u16x8::<S>::splat(simd, 0);
    let mut max_vec = u16x8::<S>::splat(simd, 0);
    let (chunks, remainder) = counts.as_chunks_mut::<8>();
    for chunk in chunks {
        let raw = u16x8::<S>::from_slice(simd, chunk);
        max_vec = max_vec.max(raw);
        let mut scan = raw + raw.swizzle_dyn_precise(shift1);
        scan = scan + scan.swizzle_dyn_precise(shift2);
        scan = scan + scan.swizzle_dyn_precise(shift4);
        let starts = scan.swizzle_dyn_precise(shift1) + carry;
        starts.store_slice(chunk);
        carry += scan.swizzle_dyn_precise(last);
    }
    let mut largest = max_vec.reduce_max();
    let mut offset = carry.to_array()[0];
    for slot in remainder {
        let count = *slot;
        largest = largest.max(count);
        *slot = offset;
        offset = offset.wrapping_add(count);
    }
    largest
}

/// Confirms ascending direct keys after an already encoded predecessor.
// Keep short scalar suffixes in the caller; see
// docs/design/evidence.md#shared-ordered-finishing-controls.
#[inline]
pub(crate) fn confirm_ascending<K: RadixKey>(values: &[K], previous: K::Encoded) -> bool {
    if values.len() < SIMD_MIN {
        return crate::engine::order::confirm_ascending(values, &mut Direct, previous);
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
        return crate::engine::order::by_key(values, &mut Direct);
    }
    let peel = PEEL.min(values.len() - 1);
    if let Some(index) = crate::engine::order::by_key(&values[..=peel], &mut Direct) {
        return Some(index);
    }
    scan_pairs(simd, &values[peel..]).map(|offset| offset + peel)
}

/// One scan for every direct encoding, using explicit fearless SIMD pairs.
#[inline(always)]
fn scan_pairs<S: Simd, K: RadixKey>(simd: S, values: &[K]) -> Option<usize> {
    let lanes = K::lanes::<S>();
    if lanes == 1 {
        return crate::engine::order::by_key(values, &mut Direct);
    }
    let pairs = values.len().saturating_sub(1);
    let block_len = 4 * lanes;
    let mut index = 0;
    while index + block_len <= pairs {
        // Combine four native comparison masks in SIMD registers so ordered
        // blocks pay one bitmask extraction without recomputing on a hit.
        // Write the four calls directly instead of `array::from_fn`, which
        // crosses a non-target-feature boundary and drops AVX2 inlining.
        let block = &values[index..=index + block_len];
        let m0 = K::pair_gt(simd, &block[..lanes], &block[1..1 + lanes]);
        let m1 = K::pair_gt(
            simd,
            &block[lanes..2 * lanes],
            &block[lanes + 1..2 * lanes + 1],
        );
        let m2 = K::pair_gt(
            simd,
            &block[2 * lanes..3 * lanes],
            &block[2 * lanes + 1..3 * lanes + 1],
        );
        let m3 = K::pair_gt(
            simd,
            &block[3 * lanes..4 * lanes],
            &block[3 * lanes + 1..4 * lanes + 1],
        );
        if ((m0 | m1) | (m2 | m3)).to_bitmask() != 0 {
            let masks = [m0, m1, m2, m3];
            for (vector, &mask) in masks.iter().enumerate() {
                let bits = mask.to_bitmask();
                if bits != 0 {
                    return Some(index + vector * lanes + bits.trailing_zeros() as usize);
                }
            }
        }
        index += block_len;
    }
    while index + lanes <= pairs {
        let bits = K::pair_gt(
            simd,
            &values[index..index + lanes],
            &values[index + 1..index + 1 + lanes],
        )
        .to_bitmask();
        if bits != 0 {
            return Some(index + bits.trailing_zeros() as usize);
        }
        index += lanes;
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
            type Vec<S: Simd> = <Self as SimdIntElement>::Native<S>;
            #[inline(always)] fn lanes<S: Simd>() -> usize { Self::Vec::<S>::LEN }
            #[inline(always)] fn max_lane<S: Simd>() -> Self { Self::MAX }
            #[inline(always)]
            fn load_encoded<S: Simd>(simd: S, slice: &[Self]) -> Self::Vec<S> {
                Self::Vec::<S>::from_slice(simd, slice)
            }
            #[inline(always)]
            fn splat_encoded<S: Simd>(simd: S, key: Self) -> Self::Vec<S> {
                Self::Vec::<S>::splat(simd, key)
            }
            #[inline(always)]
            fn store_decoded<S: Simd>(vec: Self::Vec<S>, slice: &mut [Self]) {
                vec.store_slice(slice);
            }
        }
    )*};
}
integer_key!(u8, u16, u32, u64, i8, i16, i32, i64);

macro_rules! pointer_key {
    ($(($ptr:ty, $fixed:ty)),* $(,)?) => {$(
        impl SimdKey for $ptr {
            type Vec<S: Simd> = <$fixed as SimdKey>::Vec<S>;
            #[inline(always)] fn lanes<S: Simd>() -> usize { <$fixed as SimdKey>::lanes::<S>() }
            #[inline(always)] fn max_lane<S: Simd>() -> $fixed { <$fixed as SimdKey>::max_lane::<S>() }
            #[inline(always)]
            fn load_encoded<S: Simd>(simd: S, slice: &[Self]) -> Self::Vec<S> {
                const { assert!(size_of::<$ptr>() == size_of::<$fixed>() && align_of::<$ptr>() == align_of::<$fixed>()) }
                // SAFETY: pointer-sized integers have the same size, alignment,
                // signed/two's-complement bit validity and shared-borrow
                // lifetimes as the selected fixed-width integer.
                let slice = unsafe {
                    core::slice::from_raw_parts(slice.as_ptr().cast::<$fixed>(), slice.len())
                };
                <$fixed as SimdKey>::load_encoded(simd, slice)
            }
            #[inline(always)]
            fn splat_encoded<S: Simd>(simd: S, key: Self) -> Self::Vec<S> {
                <$fixed as SimdKey>::splat_encoded(simd, key as $fixed)
            }
            #[inline(always)]
            fn store_decoded<S: Simd>(vec: Self::Vec<S>, slice: &mut [Self]) {
                const { assert!(size_of::<$ptr>() == size_of::<$fixed>() && align_of::<$ptr>() == align_of::<$fixed>()) }
                // SAFETY: same layout, alignment, bit validity and exclusive
                // borrow lifetime as `$fixed`.
                let slice = unsafe {
                    core::slice::from_raw_parts_mut(slice.as_mut_ptr().cast::<$fixed>(), slice.len())
                };
                <$fixed as SimdKey>::store_decoded::<S>(vec, slice);
            }
        }
    )*};
}
#[cfg(target_pointer_width = "64")]
pointer_key!((usize, u64), (isize, i64));
#[cfg(target_pointer_width = "32")]
pointer_key!((usize, u32), (isize, i32));
#[cfg(target_pointer_width = "16")]
pointer_key!((usize, u16), (isize, i16));

macro_rules! float_key {
    ($(($f:ty, $u:ty)),* $(,)?) => {$(
        impl SimdKey for $f {
            type Vec<S: Simd> = <<Self as SimdFloatElement>::Native<S> as SimdFloat<S>>::Bits;
            #[inline(always)] fn lanes<S: Simd>() -> usize { <Self as SimdFloatElement>::Native::<S>::LEN }
            #[inline(always)] fn max_lane<S: Simd>() -> $u { <$u>::MAX }
            #[inline(always)]
            fn load_encoded<S: Simd>(simd: S, slice: &[Self]) -> Self::Vec<S> {
                let sign: Self::Vec<S> = ((1 as $u) << (<$u>::BITS - 1)).simd_into(simd);
                let zero: Self::Vec<S> = (0 as $u).simd_into(simd);
                let bits = <Self as SimdFloatElement>::Native::<S>::from_slice(simd, slice).to_bits();
                // The sign is zero or one; subtraction expands it to
                // zero or all ones, preserving total_cmp including NaN bits.
                bits ^ ((zero - (bits >> (<$u>::BITS - 1))) | sign)
            }
            #[inline(always)]
            fn store_decoded<S: Simd>(vec: Self::Vec<S>, slice: &mut [Self]) {
                let simd = vec.token();
                let sign: Self::Vec<S> = ((1 as $u) << (<$u>::BITS - 1)).simd_into(simd);
                let one: Self::Vec<S> = (1 as $u).simd_into(simd);
                // Encoded non-negatives have sign bit 1 (invert sign only);
                // encoded negatives have sign bit 0 (invert all bits).
                let bits = vec ^ (((vec >> (<$u>::BITS - 1)) - one) | sign);
                <Self as SimdFloatElement>::Native::<S>::from_bits(bits).store_slice(slice);
            }
        }
    )*};
}
float_key!((f32, u32), (f64, u64));

macro_rules! scalar_key {
    (NonZero: $($ty:ty),* $(,)?) => { $(scalar_key!(core::num::NonZero<$ty>);)* };
    (bytes: $($n:literal),* $(,)?) => { $(scalar_key!([u8; $n]);)* };
    ($($ty:ty),* $(,)?) => {$(
        impl SimdKey for $ty {
            type Vec<S: Simd> = S::u64s;
        }
    )*};
}
scalar_key!(bool, char, u128, i128);
scalar_key!(NonZero: u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize);
scalar_key!(bytes: 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);
#[cfg(not(any(
    target_pointer_width = "16",
    target_pointer_width = "32",
    target_pointer_width = "64"
)))]
scalar_key!(usize, isize);

macro_rules! tuple_simd_key {
    (@step [$($head:ident),+]; $last:ident $(, $rest:ident)*) => {
        impl<$($head: SimdKey,)+ $last: SimdKey> SimdKey for ($($head,)+ $last) {
            type Vec<S: Simd> = S::u64s;
        }
        tuple_simd_key!(@step [$($head,)+ $last]; $($rest),*);
    };
    (@step [$($head:ident),+];) => {};
}
tuple_simd_key!(@step [A]; B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

impl<K: RadixKey> SimdKey for core::cmp::Reverse<K> {
    type Vec<S: Simd> = K::Vec<S>;
    #[inline(always)]
    fn lanes<S: Simd>() -> usize {
        K::lanes::<S>()
    }
    #[inline(always)]
    fn max_lane<S: Simd>() -> <Self::Vec<S> as SimdBase<S>>::Element {
        K::max_lane::<S>()
    }

    #[inline(always)]
    fn load_encoded<S: Simd>(simd: S, slice: &[Self]) -> Self::Vec<S> {
        // SAFETY: Reverse<K> is repr(transparent) over K.
        let slice = unsafe { core::slice::from_raw_parts(slice.as_ptr().cast::<K>(), slice.len()) };
        // Bitwise complement strictly reverses unsigned, two's-complement
        // signed, and encoded-float order, and is its own inverse.
        !K::load_encoded(simd, slice)
    }

    #[inline(always)]
    fn store_decoded<S: Simd>(vec: Self::Vec<S>, slice: &mut [Self]) {
        // SAFETY: Reverse<K> is repr(transparent) over K.
        let slice =
            unsafe { core::slice::from_raw_parts_mut(slice.as_mut_ptr().cast::<K>(), slice.len()) };
        K::store_decoded::<S>(!vec, slice);
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
        for level in msd_levels() {
            // Reuse the scan fixtures for SIMD finishing's encoded load/store,
            // including exact float bits, scalar keys and nested Reverse.
            let (w, supported) = msd_info::<K>(level);
            let mut partitioned = values.clone();
            for bucket in partitioned.chunks_mut(w + 1) {
                bucket.reverse();
            }
            check_msd(
                level,
                partitioned,
                w + 1,
                supported && values.len() >= 2 * w,
            );
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
            &[0, 1, 2, 31, 63, 64, 65, 97, 129, 265]
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
    fn pointer_sized_lanes_match_fixed_width() {
        #[cfg(target_pointer_width = "64")]
        type FixedU = u64;
        #[cfg(target_pointer_width = "64")]
        type FixedI = i64;
        #[cfg(target_pointer_width = "32")]
        type FixedU = u32;
        #[cfg(target_pointer_width = "32")]
        type FixedI = i32;
        #[cfg(target_pointer_width = "16")]
        type FixedU = u16;
        #[cfg(target_pointer_width = "16")]
        type FixedI = i16;

        #[cfg(any(
            target_pointer_width = "16",
            target_pointer_width = "32",
            target_pointer_width = "64"
        ))]
        {
            fn lanes<S: Simd, K: SimdKey>(_: S) -> usize {
                K::lanes::<S>()
            }
            for level in [Level::baseline(), Level::new()] {
                dispatch!(level, simd => {
                    let (u, i) = (lanes::<_, FixedU>(simd), lanes::<_, FixedI>(simd));
                    assert_eq!(lanes::<_, usize>(simd), u);
                    assert_eq!(lanes::<_, isize>(simd), i);
                    assert_eq!(lanes::<_, core::cmp::Reverse<usize>>(simd), u);
                    assert_eq!(lanes::<_, core::cmp::Reverse<isize>>(simd), i);
                });
            }
        }
    }

    #[test]
    fn float_scans() {
        let lengths: &[usize] = if cfg!(miri) {
            &[0, 1, 97]
        } else {
            &[0, 1, 2, 31, 63, 64, 65, 97, 129, 265]
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
            for (first, later) in [
                (4, 12),
                (5, 8),
                (7, 12),
                (7, 33),
                (31, 65),
                (35, 67),
                (67, 95),
            ] {
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
            for shape in 0..4 {
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
                        2 => {
                            if index + 1 == length {
                                u16::MAX
                            } else {
                                0
                            }
                        }
                        _ => ((index * 37 + 11) % 4) as u16,
                    })
                    .collect();
                let expected_max = counts.iter().copied().max().unwrap_or(0);
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
                    let largest = dispatch!(level, simd => prefix_sum_with(simd, &mut actual));
                    assert_eq!(
                        actual, expected,
                        "level={level:?} len={length} shape={shape}"
                    );
                    assert_eq!(
                        largest, expected_max,
                        "largest mismatch: level={level:?} len={length} shape={shape}"
                    );
                }
            }
        }
    }

    #[test]
    fn insertion_encodings() {
        use crate::engine::key::{ByKey, Extract};

        fn check<K: RadixKey>(keys: Vec<K>)
        where
            K::Encoded: core::fmt::Debug,
        {
            let encoded = |keys: &[K]| keys.iter().copied().map(K::encode).collect::<Vec<_>>();
            for budget in [0, 10, usize::MAX] {
                let mut direct = keys.clone();
                let mut scalar = keys.clone();
                assert_eq!(
                    insertion_sort(&mut direct, budget),
                    ByKey(|key: &K| *key).insertion_sort(&mut scalar, budget),
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

    fn msd_levels() -> Vec<Level> {
        let levels = vec![Level::baseline(), Level::new()];
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        let levels = levels
            .into_iter()
            .chain(Level::new().as_avx2().map(Level::Avx2))
            .collect();
        levels
    }

    fn msd_info<K: SimdKey>(level: Level) -> (usize, bool) {
        dispatch!(level, simd => {
            fn info<S: Simd, K: SimdKey>(simd: S) -> (usize, bool) {
                (K::lanes::<S>(), msd_supported::<S, K>(simd))
            }
            info::<_, K>(simd)
        })
    }

    fn check_msd<K: RadixKey>(level: Level, mut keys: Vec<K>, largest: usize, eligible: bool) {
        use crate::engine::key::Unsigned;
        let encoded = |keys: &[K]| {
            keys.iter()
                .map(|k| k.encode().to_u128())
                .collect::<Vec<_>>()
        };
        let mut expected = encoded(&keys);
        if eligible {
            expected.sort();
        }
        assert_eq!(
            dispatch!(level, simd => finish_msd_with(simd, &mut keys, largest)),
            eligible,
            "level={level:?} len={} largest={largest}",
            keys.len()
        );
        // Scalar encoding is bijective, including NaN payloads and signed zeros.
        assert_eq!(encoded(&keys), expected);
    }

    #[test]
    fn finish_msd_boundaries() {
        // Window geometry is independent of encoding; the scan fixtures above
        // exercise the individual encodings. Keep ordinary and wide lane counts.
        fn check<K: RadixKey>(make: impl Fn(usize) -> K) {
            for level in msd_levels() {
                let (w, supported) = msd_info::<K>(level);
                let lengths = [
                    0,
                    2 * w - 1,
                    2 * w,
                    2 * w + 1,
                    8 * w - 1,
                    8 * w,
                    8 * w + 1,
                    9 * w,
                    9 * w + 3,
                    10 * w + 3,
                ];
                for len in lengths {
                    for largest in [1, 2, w, w + 1, w + 2] {
                        let mut keys: Vec<K> = (0..len).map(&make).collect();
                        for bucket in keys.chunks_mut(largest) {
                            bucket.reverse();
                        }
                        check_msd(
                            level,
                            keys,
                            largest,
                            supported && len >= 2 * w && largest <= w + 1,
                        );
                    }
                }
                if supported {
                    let len = 10 * w + 3;
                    let seg = (len / (4 * w)) * w;
                    // A worst-case (w + 1)-record bucket crosses every stream
                    // seam and tail boundary, at both starting parities.
                    for seam in [seg, 2 * seg, 3 * seg, 4 * seg, 4 * seg + w, len - w] {
                        for parity in [0, 1] {
                            let start = seam.saturating_sub(w / 2 + parity);
                            let mut keys: Vec<K> = (0..len).map(&make).collect();
                            keys[start..(start + w + 1).min(len)].reverse();
                            check_msd(level, keys, w + 1, true);
                        }
                    }
                    // Mixed-size tail: `[4*seg + 2 .. 4*seg + w + 1)` lies
                    // outside both the seam repair `[4*seg - w .. 4*seg + w)`
                    // and the shifted end `[len - 2*w .. len)`, so only the
                    // interior aligned tail window `[4*seg .. 4*seg + 2*w)`
                    // covers the whole reversed bucket.
                    let mut mixed: Vec<K> = (0..len).map(&make).collect();
                    mixed[..w + 1].reverse();
                    mixed[4 * seg + 2..4 * seg + w + 1].reverse();
                    check_msd(level, mixed, w + 1, true);
                }
            }
        }
        check(|i| i as u32);
        check(|i| (i / 8) as u8);
        let mut short = [2u32; SIMD_MIN - 1];
        assert!(!finish_msd(&mut short, 2));
        assert_eq!(short, [2u32; SIMD_MIN - 1]);
        let mut wide = [3u128; SIMD_MIN];
        assert!(!finish_msd(&mut wide, 2));
        assert_eq!(wide, [3u128; SIMD_MIN]);
    }
}
