// SPDX-License-Identifier: BSD-2-Clause

//! Key encoding and key extraction.
//!
//! [`RadixKey`] maps each supported key to an unsigned integer whose order is
//! the key order. [`Extract`] computes that integer for a record, either from a
//! user key function ([`ByKey`]) or from the record itself ([`Direct`]), and
//! selects the scan an insertion pass uses.

use core::cmp::Reverse;
use core::ops::{BitAnd, BitOrAssign, Not, Shl, Shr};

/// A key type that Tessera can sort by.
///
/// Implemented for every primitive integer from `u8`/`i8` to `u128`/`i128`
/// (including `usize` and `isize`), for `f32` and `f64`, and for
/// [`Reverse<K>`](Reverse) of any of these. The trait is sealed: other types
/// cannot implement it, so an unsupported key is a compile error.
///
/// Each key is encoded as an unsigned integer of the same width whose integer
/// order is the key order:
///
/// | Key | Encoding |
/// |---|---|
/// | Unsigned integers | Unchanged |
/// | Signed integers | Sign bit flipped |
/// | `f32`, `f64` | All bits inverted if the sign bit is set, otherwise the sign bit flipped; this matches `total_cmp` |
/// | `Reverse<K>` | All bits of the encoding of `K` inverted |
pub trait RadixKey: sealed::Sealed + Copy {
    #[doc(hidden)]
    type Encoded: Unsigned;
    #[doc(hidden)]
    fn encode(self) -> Self::Encoded;
}

mod sealed {
    use super::{Direct, RadixKey};

    /// Scans and insertion passes for direct keys of one type.
    ///
    /// The defaults are the default-build scans. Primitive keys override them
    /// with the vector backend under `portable-simd`; `Reverse<K>` keeps the
    /// defaults, so it needs no second set of vector kernels.
    pub trait Sealed: Copy {
        #[inline]
        fn confirm_ascending(keys: &[Self], previous: <Self as RadixKey>::Encoded) -> bool
        where
            Self: RadixKey,
        {
            // Confirmation and insertion probes cross the same ordered runs.
            keys.first().is_none_or(|key| previous <= key.encode())
                && Self::first_inversion(keys).is_none()
        }

        #[inline(always)]
        fn first_inversion(keys: &[Self]) -> Option<usize>
        where
            Self: RadixKey,
        {
            crate::scan::first_inversion(keys)
        }

        #[inline]
        fn finish_insertion(keys: &mut [Self])
        where
            Self: RadixKey,
        {
            // Tile finishing crosses many short ordered runs, so a blocked scan
            // would build a 32-pair mask for every inversion it finds. `ByKey`
            // scans one pair at a time; budgeted probes keep the blocked scan
            // for their long ordered stretches.
            crate::engine::finish_insertion(keys, &mut super::ByKey(|key: &Self| *key));
        }

        #[inline]
        fn insertion_sort(keys: &mut [Self], budget: usize) -> Option<usize>
        where
            Self: RadixKey,
        {
            crate::engine::insertion_loop(keys, budget, &mut Direct)
        }
    }
}

/// Unsigned integer operations the engine needs on encoded keys.
#[doc(hidden)]
pub trait Unsigned:
    RadixKey<Encoded = Self>
    + Copy
    + Ord
    + BitAnd<Output = Self>
    + BitOrAssign
    + Shl<Output = Self>
    + Shr<Output = Self>
    + Not<Output = Self>
{
    const BITS: usize;
    fn from_usize(value: usize) -> Self;
    fn to_usize(self) -> usize;
    fn to_u128(self) -> u128;
}

/// Routes the scans of a primitive key to the vector backend under
/// `portable-simd`. Expands to nothing in the default build.
macro_rules! vector_backend {
    () => {
        #[cfg(feature = "portable-simd")]
        #[inline]
        fn confirm_ascending(keys: &[Self], previous: <Self as RadixKey>::Encoded) -> bool {
            crate::portable_simd::confirm_ascending(keys, previous)
        }

        #[cfg(feature = "portable-simd")]
        #[inline]
        fn finish_insertion(keys: &mut [Self]) {
            crate::portable_simd::finish_insertion(keys)
        }

        #[cfg(feature = "portable-simd")]
        #[inline]
        fn insertion_sort(keys: &mut [Self], budget: usize) -> Option<usize> {
            crate::portable_simd::insertion_sort(keys, budget)
        }
    };
}

macro_rules! unsigned {
    ($($ty:ty),* $(,)?) => {$(
        impl sealed::Sealed for $ty {
            vector_backend!();
        }
        impl Unsigned for $ty {
            const BITS: usize = <$ty>::BITS as usize;
            #[inline] fn from_usize(value: usize) -> Self { value as Self }
            #[inline] fn to_usize(self) -> usize { self as usize }
            #[inline] fn to_u128(self) -> u128 { self as u128 }
        }
        impl RadixKey for $ty {
            type Encoded = Self;
            #[inline] fn encode(self) -> Self { self }
        }
    )*};
}
unsigned!(u8, u16, u32, u64, u128, usize);

macro_rules! signed {
    ($(($s:ty, $u:ty)),* $(,)?) => {$(
        impl sealed::Sealed for $s {
            vector_backend!();
        }
        impl RadixKey for $s {
            type Encoded = $u;
            #[inline] fn encode(self) -> $u {
                (self as $u) ^ (1 << (<$u>::BITS - 1))
            }
        }
    )*};
}
signed!(
    (i8, u8),
    (i16, u16),
    (i32, u32),
    (i64, u64),
    (i128, u128),
    (isize, usize)
);

macro_rules! float {
    ($(($f:ty, $u:ty, $scan:path)),* $(,)?) => {$(
        impl sealed::Sealed for $f {
            vector_backend!();

            #[inline(always)]
            fn first_inversion(keys: &[Self]) -> Option<usize> {
                $scan(keys)
            }
        }
        impl RadixKey for $f {
            type Encoded = $u;
            #[inline] fn encode(self) -> $u {
                let bits = self.to_bits();
                let sign = 1 << (<$u>::BITS - 1);
                if bits & sign != 0 { !bits } else { bits ^ sign }
            }
        }
    )*};
}
// The default build's blocked scan compares 64-bit lanes poorly at the
// baseline vector width, so `f64` scans one pair at a time.
float!(
    (f32, u32, crate::scan::first_inversion),
    (f64, u64, crate::scan::scalar)
);

impl<K: RadixKey> sealed::Sealed for Reverse<K> {}
impl<K: RadixKey> RadixKey for Reverse<K> {
    type Encoded = K::Encoded;
    #[inline]
    fn encode(self) -> Self::Encoded {
        !self.0.encode()
    }
}

/// Widest key window that [`Extract::try_count_window`] counts and fills, in bits.
const SMALL_WINDOW_BITS: usize = 8;

/// Buckets such a window can hold.
const SMALL_WINDOW_BUCKETS: usize = 1 << SMALL_WINDOW_BITS;

/// Computes the encoded key of a record.
///
/// `ByKey` calls a user key function, `Direct` encodes the record itself, and
/// `KeyHalf` narrows another extractor to one 64-bit half of a 128-bit key.
pub(crate) trait Extract<T> {
    /// The encoded key type.
    type Value: Unsigned;

    /// Returns the encoded key of `record`.
    fn key(&mut self, record: &T) -> Self::Value;

    /// Finishes a slice whose keys all fall in one small window of the encoded
    /// key, by counting the window and filling each run from one of its records.
    ///
    /// The window starts at bit `low` and is `width` bits wide, at most
    /// [`SMALL_WINDOW_BITS`]; the caller guarantees that every key agrees
    /// outside it. Filling a run with one of its records preserves order and
    /// stability only where equal keys imply interchangeable records, which a
    /// user key function does not guarantee. The default therefore declines and
    /// leaves the slice unchanged for the block partition.
    ///
    /// Returns whether the slice is finished.
    fn try_count_window(&mut self, _records: &mut [T], _low: usize, _width: usize) -> bool {
        false
    }

    /// Finishes a small key window using parallel counting where supported.
    ///
    /// Has the same window and interchangeability contract as
    /// [`Self::try_count_window`]. Record extractors retain the serial fallback.
    #[cfg(feature = "rayon")]
    fn try_count_window_parallel(&mut self, records: &mut [T], low: usize, width: usize) -> bool
    where
        T: Send,
    {
        self.try_count_window(records, low, width)
    }

    /// Returns whether `previous` and the keys of `records` are in ascending
    /// order. `previous` is the already encoded key of the preceding record.
    fn confirm_ascending(&mut self, records: &[T], previous: Self::Value) -> bool
    where
        Self: Sized,
    {
        crate::engine::confirm_ascending(records, self, previous)
    }

    /// Returns the offset of the first adjacent pair whose keys descend.
    ///
    /// Retaining the previous key avoids evaluating it again for the next
    /// pair, so the key function is called once per scanned record.
    #[inline]
    fn first_inversion(&mut self, records: &[T]) -> Option<usize> {
        let first = records.first()?;
        let mut previous = self.key(first);
        records.windows(2).position(|pair| {
            let current = self.key(&pair[1]);
            let inverted = previous > current;
            previous = current;
            inverted
        })
    }

    /// Sorts `records` stably by insertion, without a swap limit.
    ///
    /// Direct extraction selects its scan once for the whole pass.
    fn finish_insertion(&mut self, records: &mut [T])
    where
        Self: Sized,
    {
        crate::engine::finish_insertion(records, self);
    }

    /// Sorts `records` stably by insertion within `budget` swaps.
    ///
    /// Returns the number of swaps, or `None` once the budget runs out; the
    /// records are then a stable permutation of their input. Direct extraction
    /// selects its scan once for the whole pass.
    fn insertion_sort(&mut self, records: &mut [T], budget: usize) -> Option<usize>
    where
        Self: Sized,
    {
        crate::engine::insertion_loop(records, budget, self)
    }
}

/// Extracts keys with a user key function, which may panic or be inconsistent.
#[derive(Clone)]
pub(crate) struct ByKey<F>(pub F);
impl<T, K: RadixKey, F: FnMut(&T) -> K> Extract<T> for ByKey<F> {
    type Value = K::Encoded;
    #[inline]
    fn key(&mut self, record: &T) -> Self::Value {
        (self.0)(record).encode()
    }
}

/// Uses the record itself as the key; deterministic and panic-free.
#[derive(Clone, Copy)]
pub(crate) struct Direct;

impl<K: RadixKey> Extract<K> for Direct {
    type Value = K::Encoded;

    #[cfg(feature = "rayon")]
    fn try_count_window_parallel(&mut self, records: &mut [K], low: usize, width: usize) -> bool
    where
        K: Send,
    {
        use rayon::prelude::*;

        if width == 0 || width > SMALL_WINDOW_BITS {
            return false;
        }
        if records.is_empty() {
            return true;
        }
        let shard_len = records.len().div_ceil(rayon::current_num_threads());
        // Mutable shards need only Send; no shared record borrow crosses tasks.
        let counts = records
            .par_chunks_mut(shard_len)
            .map(|records| KeyCounts::count(records, low, width))
            .reduce_with(KeyCounts::merge)
            .unwrap();
        counts
            .runs(records)
            .collect::<Vec<_>>()
            .into_par_iter()
            .for_each(|(run, key)| run.fill(key));
        true
    }

    fn try_count_window(&mut self, records: &mut [K], low: usize, width: usize) -> bool {
        if width == 0 || width > SMALL_WINDOW_BITS {
            return false;
        }
        if !records.is_empty() {
            for (run, key) in KeyCounts::count(records, low, width).runs(records) {
                run.fill(key);
            }
        }
        true
    }

    #[inline]
    fn confirm_ascending(&mut self, records: &[K], previous: Self::Value) -> bool {
        K::confirm_ascending(records, previous)
    }

    #[inline]
    fn key(&mut self, record: &K) -> Self::Value {
        record.encode()
    }

    // Inlined through `K::first_inversion` so each scan is not a call and the
    // scan sees the caller's slice bounds.
    #[inline(always)]
    fn first_inversion(&mut self, records: &[K]) -> Option<usize> {
        K::first_inversion(records)
    }

    #[inline]
    fn finish_insertion(&mut self, records: &mut [K]) {
        K::finish_insertion(records);
    }

    #[inline]
    fn insertion_sort(&mut self, records: &mut [K], budget: usize) -> Option<usize> {
        K::insertion_sort(records, budget)
    }
}

/// A histogram retaining one interchangeable record per encoded-key bucket.
///
/// Sealed encodings are bijective on bits, including NaN payloads and signed
/// zeros. When the window covers all variation, a representative stands for
/// every record counted in its bucket. Serial and parallel finishing share this
/// ownership-free summary; only collection and filling are scheduled differently.
struct KeyCounts<K> {
    counts: [usize; SMALL_WINDOW_BUCKETS],
    representatives: [K; SMALL_WINDOW_BUCKETS],
}

impl<K: RadixKey> KeyCounts<K> {
    /// Counts a nonempty slice in a window of at most `SMALL_WINDOW_BITS` bits.
    fn count(records: &[K], low: usize, width: usize) -> Self {
        let mut result = Self {
            counts: [0; SMALL_WINDOW_BUCKETS],
            representatives: [records[0]; SMALL_WINDOW_BUCKETS],
        };
        let mask = K::Encoded::from_usize((1 << width) - 1);
        let shift = K::Encoded::from_usize(low);
        for &record in records {
            let bucket = ((record.encode() >> shift) & mask).to_usize();
            if result.counts[bucket] == 0 {
                result.representatives[bucket] = record;
            }
            result.counts[bucket] += 1;
        }
        result
    }

    /// Combines disjoint shards counted with the same window.
    #[cfg(feature = "rayon")]
    fn merge(mut self, other: Self) -> Self {
        for bucket in 0..SMALL_WINDOW_BUCKETS {
            if other.counts[bucket] != 0 {
                self.representatives[bucket] = other.representatives[bucket];
                self.counts[bucket] += other.counts[bucket];
            }
        }
        self
    }

    /// Pairs each nonempty destination run with its representative.
    /// The slice must contain exactly the records summarized by this histogram.
    fn runs(self, records: &mut [K]) -> impl Iterator<Item = (&mut [K], K)> {
        self.counts
            .into_iter()
            .zip(self.representatives)
            .scan(records, |remaining, (count, key)| {
                let (run, rest) = core::mem::take(remaining).split_at_mut(count);
                *remaining = rest;
                Some((run, key))
            })
            .filter(|(run, _)| !run.is_empty())
    }
}

/// Borrows the 64 key bits starting at `SHIFT` when the other half is constant.
pub(crate) struct KeyHalf<'a, E, const SHIFT: usize>(pub &'a mut E);

/// Implements half-key rules for a tuple wrapper around the original extractor.
/// Scans retain the defaults for the narrowed keys, not the inner key width.
macro_rules! impl_key_half {
    ($half:ty) => {
        impl<T, E: $crate::key::Extract<T>, const SHIFT: usize> $crate::key::Extract<T> for $half {
            type Value = u64;

            #[inline]
            fn key(&mut self, record: &T) -> u64 {
                ($crate::key::Unsigned::to_u128(self.0.key(record)) >> SHIFT) as u64
            }

            fn try_count_window(&mut self, records: &mut [T], low: usize, width: usize) -> bool {
                // The other half is constant, so this translated window still
                // covers every difference in the underlying key.
                self.0.try_count_window(records, low + SHIFT, width)
            }

            #[cfg(feature = "rayon")]
            fn try_count_window_parallel(
                &mut self,
                records: &mut [T],
                low: usize,
                width: usize,
            ) -> bool
            where
                T: Send,
            {
                self.0
                    .try_count_window_parallel(records, low + SHIFT, width)
            }
        }
    };
}

impl_key_half!(KeyHalf<'_, E, SHIFT>);
#[cfg(feature = "rayon")]
pub(crate) use impl_key_half;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn half_keys_keep_encoded_bits() {
        let key = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210u128;
        assert_eq!(
            KeyHalf::<_, 0>(&mut Direct).key(&key),
            0xfedc_ba98_7654_3210
        );
        assert_eq!(
            KeyHalf::<_, 64>(&mut Direct).key(&key),
            0x0123_4567_89ab_cdef
        );
    }

    /// Checks shared window and decline rules against independently sorted keys.
    pub(crate) fn check_half_windows<const SHIFT: usize>(
        direct_half: &mut impl Extract<u128, Value = u64>,
        keyed_half: &mut impl Extract<(u128, usize), Value = u64>,
    ) {
        for (low, width) in [(0, 8), (9, 3), (57, 7)] {
            let mask = ((1u128 << width) - 1) << (low + SHIFT);
            let base = 0x73a5_491b_c621_d8e0_2b93_a3f7_1d05_0000u128 & !mask;
            let input: Vec<_> = (0..65)
                .map(|index| base | (((index * 37) as u128) << (low + SHIFT) & mask))
                .collect();
            let mut expected = input.clone();
            expected.sort();

            let mut keys = input.clone();
            let mut records: Vec<_> = input.iter().copied().zip(0..input.len()).collect();
            let original = records.clone();
            assert!(direct_half.try_count_window(&mut keys, low, width));
            assert!(!keyed_half.try_count_window(&mut records, low, width));
            assert_eq!(keys, expected);
            assert_eq!(records, original);

            // Miri interprets both storage forms without invoking Rayon's pool.
            #[cfg(all(feature = "rayon", not(miri)))]
            {
                let mut keys = input;
                assert!(direct_half.try_count_window_parallel(&mut keys, low, width));
                assert!(!keyed_half.try_count_window_parallel(&mut records, low, width));
                assert_eq!(keys, expected);
                assert_eq!(records, original);
            }
        }
    }

    #[test]
    fn half_windows_keep_offsets_and_payloads() {
        check_half_windows::<0>(
            &mut KeyHalf::<_, 0>(&mut Direct),
            &mut KeyHalf::<_, 0>(&mut ByKey(|record: &(u128, usize)| record.0)),
        );
        check_half_windows::<64>(
            &mut KeyHalf::<_, 64>(&mut Direct),
            &mut KeyHalf::<_, 64>(&mut ByKey(|record: &(u128, usize)| record.0)),
        );
    }

    /// Checks confirmation against scalar extraction at every adjacent pair.
    fn check_confirmation<K: RadixKey>(mut keys: Vec<K>) {
        keys.sort_by_key(|key| key.encode());
        let zero = K::Encoded::from_usize(0);
        for previous in [zero, !zero] {
            let agrees = |records: &[K]| {
                assert_eq!(
                    Direct.confirm_ascending(records, previous),
                    crate::engine::confirm_ascending(records, &mut ByKey(|key: &K| *key), previous,),
                );
            };
            agrees(&keys);
            for index in 1..keys.len() {
                keys.swap(index - 1, index);
                agrees(&keys);
                keys.swap(index - 1, index);
            }
        }
    }

    #[test]
    fn confirmation_matches_scalar() {
        // Include peel, mask-block and SIMD boundaries, with an inversion on
        // either side of each one. Duplicate keys exercise non-strict order.
        let lengths: &[usize] = if cfg!(miri) {
            &[0, 1, 5, 34]
        } else {
            &[0, 1, 2, 4, 5, 31, 32, 33, 34, 63, 64, 65, 66, 127, 128, 129]
        };
        for &len in lengths {
            check_confirmation((0..len).map(|index| index as u32 / 2).collect());
            check_confirmation((0..len).map(|index| index as i64 / 2 - 32).collect());
            check_confirmation((0..len).map(|index| (index as u128) << 96).collect());
            check_confirmation((0..len).map(|index| Reverse(index as u64 / 2)).collect());
            check_confirmation(
                (0..len)
                    .map(|index| f32::from_bits((index as u32).wrapping_mul(0x9e37_79b9)))
                    .collect(),
            );
            check_confirmation(
                (0..len)
                    .map(|index| f64::from_bits((index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)))
                    .collect(),
            );
        }
    }
}
