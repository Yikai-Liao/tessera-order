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
pub trait RadixKey: crate::portable_simd::SimdKey + Copy {
    #[doc(hidden)]
    type Encoded: Unsigned;
    #[doc(hidden)]
    fn encode(self) -> Self::Encoded;
    /// Inverts [`Self::encode`] bit for bit, including NaN payloads.
    #[doc(hidden)]
    fn decode(encoded: Self::Encoded) -> Self;
}

/// Unsigned integer operations the engine needs on encoded keys.
#[doc(hidden)]
pub trait Unsigned:
    RadixKey<Encoded = Self>
    + Copy
    + Send
    + Sync
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

macro_rules! unsigned {
    ($($ty:ty),* $(,)?) => {$(
        impl Unsigned for $ty {
            const BITS: usize = <$ty>::BITS as usize;
            #[inline] fn from_usize(value: usize) -> Self { value as Self }
            #[inline] fn to_usize(self) -> usize { self as usize }
            #[inline] fn to_u128(self) -> u128 { self as u128 }
        }
        impl RadixKey for $ty {
            type Encoded = Self;
            #[inline] fn encode(self) -> Self { self }
            #[inline] fn decode(encoded: Self) -> Self { encoded }
        }
    )*};
}
unsigned!(u8, u16, u32, u64, u128, usize);

macro_rules! signed {
    ($(($s:ty, $u:ty)),* $(,)?) => {$(
        impl RadixKey for $s {
            type Encoded = $u;
            #[inline] fn encode(self) -> $u {
                (self as $u) ^ (1 << (<$u>::BITS - 1))
            }
            #[inline] fn decode(encoded: $u) -> $s {
                (encoded ^ (1 << (<$u>::BITS - 1))) as $s
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
    ($(($f:ty, $u:ty)),* $(,)?) => {$(
        impl RadixKey for $f {
            type Encoded = $u;
            #[inline] fn encode(self) -> $u {
                let bits = self.to_bits();
                let sign = 1 << (<$u>::BITS - 1);
                if bits & sign != 0 { !bits } else { bits ^ sign }
            }
            #[inline] fn decode(encoded: $u) -> $f {
                // Encoded positive values keep the sign bit set; negative
                // values had every bit inverted.
                let sign = 1 << (<$u>::BITS - 1);
                <$f>::from_bits(if encoded & sign != 0 { encoded ^ sign } else { !encoded })
            }
        }
    )*};
}
float!((f32, u32), (f64, u64));

impl<K: RadixKey> RadixKey for Reverse<K> {
    type Encoded = K::Encoded;
    #[inline]
    fn encode(self) -> Self::Encoded {
        !self.0.encode()
    }
    #[inline]
    fn decode(encoded: Self::Encoded) -> Self {
        Reverse(K::decode(!encoded))
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
///
/// # Safety
///
/// If `CACHE_BUCKETS` is false, `key` must be deterministic and panic-free:
/// tile scatter may evaluate it again after ownership transfer has started.
pub(crate) unsafe trait Extract<T> {
    /// Whether tile passes must retain each classification before moving records.
    /// User callbacks keep the default; only repeatable library encoders opt out.
    const CACHE_BUCKETS: bool = true;

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

    /// Sorts `records` stably by insertion, without a swap limit.
    ///
    /// Direct extraction selects its scan once for the whole pass.
    fn finish_insertion(&mut self, records: &mut [T])
    where
        Self: Sized,
    {
        crate::engine::finish_insertion(records, self, crate::scan::by_key);
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
        crate::engine::insertion_loop(records, budget, self, crate::scan::by_key)
    }
}

/// Extracts keys with a user key function, which may panic or be inconsistent.
#[derive(Clone)]
pub(crate) struct ByKey<F>(pub F);
// SAFETY: user callbacks retain cached classifications, even if they panic or change keys.
unsafe impl<T, K: RadixKey, F: FnMut(&T) -> K> Extract<T> for ByKey<F> {
    type Value = K::Encoded;
    #[inline]
    fn key(&mut self, record: &T) -> Self::Value {
        (self.0)(record).encode()
    }
}

/// Uses the record itself as the key; deterministic and panic-free.
#[derive(Clone, Copy)]
pub(crate) struct Direct;

// SAFETY: sealed key encodings are deterministic and panic-free.
unsafe impl<K: RadixKey> Extract<K> for Direct {
    const CACHE_BUCKETS: bool = false;
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
        crate::portable_simd::confirm_ascending(records, previous)
    }

    #[inline]
    fn key(&mut self, record: &K) -> Self::Value {
        record.encode()
    }

    #[inline]
    fn finish_insertion(&mut self, records: &mut [K]) {
        crate::portable_simd::finish_insertion(records);
    }

    #[inline]
    fn insertion_sort(&mut self, records: &mut [K], budget: usize) -> Option<usize> {
        crate::portable_simd::insertion_sort(records, budget)
    }
}

/// A histogram of one encoded-key window, with the bits outside it.
///
/// Sealed encodings are bijective on bits, including NaN payloads and signed
/// zeros. When the window covers all variation, every record counted in a
/// bucket is the decoding of the shared outside bits with that bucket's digit.
/// Serial and parallel finishing share this ownership-free summary; only
/// collection and filling are scheduled differently.
struct KeyCounts<K: RadixKey> {
    low: usize,
    buckets: usize,
    counts: [usize; SMALL_WINDOW_BUCKETS],
    /// One encoded key with its window bits cleared.
    outside: K::Encoded,
}

impl<K: RadixKey> KeyCounts<K> {
    /// Counts a nonempty slice in a window of at most `SMALL_WINDOW_BITS` bits.
    fn count(records: &[K], low: usize, width: usize) -> Self {
        let buckets = 1 << width;
        let mask = K::Encoded::from_usize(buckets - 1);
        let shift = K::Encoded::from_usize(low);
        let mut counts = [0; SMALL_WINDOW_BUCKETS];
        for &record in records {
            counts[((record.encode() >> shift) & mask).to_usize()] += 1;
        }
        Self {
            low,
            buckets,
            counts,
            outside: records[0].encode() & !(mask << shift),
        }
    }

    /// Combines disjoint shards counted with the same window.
    #[cfg(feature = "rayon")]
    fn merge(mut self, other: Self) -> Self {
        debug_assert_eq!(self.buckets, other.buckets);
        for bucket in 0..self.buckets {
            self.counts[bucket] += other.counts[bucket];
        }
        self
    }

    /// Pairs each nonempty destination run with its decoded key.
    /// The slice must contain exactly the records summarized by this histogram.
    fn runs(self, records: &mut [K]) -> impl Iterator<Item = (&mut [K], K)> {
        let (outside, shift) = (self.outside, K::Encoded::from_usize(self.low));
        self.counts
            .into_iter()
            .take(self.buckets)
            .enumerate()
            .scan(records, move |remaining, (bucket, count)| {
                let (run, rest) = core::mem::take(remaining).split_at_mut(count);
                *remaining = rest;
                Some((run, bucket))
            })
            .filter(|(run, _)| !run.is_empty())
            .map(move |(run, bucket)| {
                let mut key = outside;
                key |= K::Encoded::from_usize(bucket) << shift;
                (run, K::decode(key))
            })
    }
}

/// Extracts the 64 key bits starting at `SHIFT` when the other half is constant.
///
/// Scans retain the defaults for the narrowed keys, not the inner key width.
#[derive(Clone)]
pub(crate) struct KeyHalf<E, const SHIFT: usize>(pub E);

// SAFETY: the inner extractor owns repeatability; wrapping shifts cannot panic.
unsafe impl<T, E: Extract<T>, const SHIFT: usize> Extract<T> for KeyHalf<E, SHIFT> {
    const CACHE_BUCKETS: bool = E::CACHE_BUCKETS;
    type Value = u64;

    #[inline]
    fn key(&mut self, record: &T) -> u64 {
        self.0.key(record).to_u128().wrapping_shr(SHIFT as u32) as u64
    }

    fn try_count_window(&mut self, records: &mut [T], low: usize, width: usize) -> bool {
        // The other half is constant, so this translated window still
        // covers every difference in the underlying key.
        self.0.try_count_window(records, low + SHIFT, width)
    }

    #[cfg(feature = "rayon")]
    fn try_count_window_parallel(&mut self, records: &mut [T], low: usize, width: usize) -> bool
    where
        T: Send,
    {
        self.0
            .try_count_window_parallel(records, low + SHIFT, width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn half_keys_keep_encoded_bits() {
        let key = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210u128;
        assert_eq!(KeyHalf::<_, 0>(Direct).key(&key), 0xfedc_ba98_7654_3210);
        assert_eq!(KeyHalf::<_, 64>(Direct).key(&key), 0x0123_4567_89ab_cdef);
    }

    /// Checks shared window and decline rules against independently sorted keys.
    fn check_half_windows<const SHIFT: usize>(
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
            &mut KeyHalf::<_, 0>(Direct),
            &mut KeyHalf::<_, 0>(ByKey(|record: &(u128, usize)| record.0)),
        );
        check_half_windows::<64>(
            &mut KeyHalf::<_, 64>(Direct),
            &mut KeyHalf::<_, 64>(ByKey(|record: &(u128, usize)| record.0)),
        );
    }

    #[test]
    fn decoding_inverts_every_encoding() {
        fn round_trip<K: RadixKey>(key: K, same: impl Fn(K, K) -> bool) {
            assert!(same(K::decode(key.encode()), key));
        }
        let bits = [
            0u64,
            1,
            0x7ff8_0000_0000_0001,
            0x8000_0000_0000_0000,
            u64::MAX,
        ];
        for value in bits {
            round_trip(value, |a, b| a == b);
            round_trip(value as i64, |a, b| a == b);
            round_trip(value as u8, |a, b| a == b);
            round_trip(value as i8, |a, b| a == b);
            round_trip((value as u128) << 64 | 3, |a, b| a == b);
            round_trip(((value as u128) << 64) as i128, |a, b| a == b);
            round_trip(f64::from_bits(value), |a, b| a.to_bits() == b.to_bits());
            round_trip(f32::from_bits((value >> 32) as u32), |a, b| {
                a.to_bits() == b.to_bits()
            });
            round_trip(Reverse(f64::from_bits(value)), |a, b| {
                a.0.to_bits() == b.0.to_bits()
            });
            round_trip(Reverse(Reverse(value as i32)), |a, b| a == b);
        }
    }

    #[test]
    fn counted_floats_keep_exact_bits() {
        // Negative NaN payloads that differ only inside the low window.
        let mut negative: Vec<_> = (0..65)
            .map(|index| f32::from_bits(0xffc0_0000 | (index as u32 * 37 % 256)))
            .collect();
        let mut expected: Vec<_> = negative.iter().map(|key| key.to_bits()).collect();
        expected.sort_by_key(|&bits| f32::from_bits(bits).encode());
        assert!(Direct.try_count_window(&mut negative, 0, 8));
        assert_eq!(
            negative.iter().map(|key| key.to_bits()).collect::<Vec<_>>(),
            expected
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
