// SPDX-License-Identifier: BSD-2-Clause

//! Key encoding and key extraction.
//!
//! [`RadixKey`] maps each supported key to an unsigned integer whose order is
//! the key order. [`Extract`] computes that integer for a record and provides
//! the scans its key source supports, so one engine serves library-encoded keys
//! and user key functions alike.

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
///
/// Every encoding is a bijection on bit patterns, so equal encoded keys mean
/// identical keys, which lets the engine fill a run of equal keys from one of
/// them.
pub trait RadixKey: sealed::Sealed + Copy {
    #[doc(hidden)]
    type Encoded: Unsigned;
    #[doc(hidden)]
    fn encode(self) -> Self::Encoded;
}

mod sealed {
    use super::RadixKey;

    /// The scans a key type supports when the record is its own key.
    ///
    /// The defaults scan one pair at a time. Keys that a vector kernel covers
    /// override both, so the kernel is selected once per pass and not per scan.
    pub trait Sealed: Copy {
        /// Returns the offset of the first adjacent pair whose keys descend.
        #[inline]
        fn inversion(keys: &[Self]) -> Option<usize>
        where
            Self: RadixKey,
        {
            crate::scan::scalar(keys)
        }

        /// Sorts `keys` stably by insertion within `budget` swaps.
        #[inline]
        fn insertion_pass(keys: &mut [Self], budget: usize) -> Option<usize>
        where
            Self: RadixKey,
        {
            crate::insertion::run(keys, budget, &mut crate::insertion::Sealed)
        }
    }
}

/// Unsigned integer operations the engine needs on encoded keys.
#[doc(hidden)]
pub trait Unsigned:
    RadixKey<Encoded = Self>
    + Copy
    + Ord
    + Send
    + Sync
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

/// Routes the scans of a key type to the vector kernels it has.
macro_rules! vector_backend {
    () => {
        #[inline]
        fn inversion(keys: &[Self]) -> Option<usize>
        where
            Self: super::RadixKey,
        {
            crate::scan::inversion(keys)
        }

        #[inline]
        fn insertion_pass(keys: &mut [Self], budget: usize) -> Option<usize>
        where
            Self: super::RadixKey,
        {
            crate::insertion::run_vector(keys, budget)
        }
    };
}

/// Implements the encoded-key operations shared by every integer key.
macro_rules! unsigned_key {
    ($ty:ty) => {
        impl Unsigned for $ty {
            const BITS: usize = <$ty>::BITS as usize;
            #[inline]
            fn from_usize(value: usize) -> Self {
                value as Self
            }
            #[inline]
            fn to_usize(self) -> usize {
                self as usize
            }
            #[inline]
            fn to_u128(self) -> u128 {
                self as u128
            }
        }
        impl RadixKey for $ty {
            type Encoded = Self;
            #[inline]
            fn encode(self) -> Self {
                self
            }
        }
    };
}

macro_rules! unsigned {
    ($($ty:ty),* $(,)?) => {$(
        unsigned_key!($ty);
        impl sealed::Sealed for $ty {
            vector_backend!();
        }
    )*};
}

macro_rules! unsigned_wide {
    ($($ty:ty),* $(,)?) => {$(
        unsigned_key!($ty);
        impl sealed::Sealed for $ty {}
    )*};
}

unsigned!(u8, u16, u32, u64);
unsigned_wide!(u128, usize);

macro_rules! signed {
    ($($s:ty, $u:ty);* $(;)?) => {$(
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

macro_rules! signed_wide {
    ($($s:ty, $u:ty);* $(;)?) => {$(
        impl sealed::Sealed for $s {}
        impl RadixKey for $s {
            type Encoded = $u;
            #[inline] fn encode(self) -> $u {
                (self as $u) ^ (1 << (<$u>::BITS - 1))
            }
        }
    )*};
}

signed!(i8, u8; i16, u16; i32, u32; i64, u64);
signed_wide!(i128, u128; isize, usize);

macro_rules! float {
    ($($f:ty, $u:ty);* $(;)?) => {$(
        impl sealed::Sealed for $f {
            vector_backend!();
        }
        impl RadixKey for $f {
            type Encoded = $u;
            #[inline] fn encode(self) -> $u {
                let bits = <$f>::to_bits(self);
                let sign = 1 << (<$u>::BITS - 1);
                if bits & sign != 0 { !bits } else { bits ^ sign }
            }
        }
    )*};
}

float!(f32, u32; f64, u64);

impl<K: RadixKey> sealed::Sealed for Reverse<K> {}
impl<K: RadixKey> RadixKey for Reverse<K> {
    type Encoded = K::Encoded;
    #[inline]
    fn encode(self) -> Self::Encoded {
        !self.0.encode()
    }
}

/// One pass worth of encoded key bits: `width` bits starting at `shift`.
#[derive(Clone, Copy)]
pub(crate) struct Digit {
    /// Position of the lowest bit the window covers.
    pub(crate) shift: usize,
    /// Number of bits the window covers.
    pub(crate) width: usize,
}

impl Digit {
    /// Mask with this window's bits set, in key position.
    #[inline]
    pub(crate) fn mask<V: Unsigned>(self) -> V {
        V::from_usize((1usize << self.width) - 1) << V::from_usize(self.shift)
    }

    /// Returns the bucket of `record` within this window.
    #[inline]
    pub(crate) fn classify<T, E: Extract<T>>(self, extractor: &mut E, record: &T) -> usize {
        let shift = E::Value::from_usize(self.shift);
        let mask = E::Value::from_usize((1usize << self.width) - 1);
        ((extractor.key(record) >> shift) & mask).to_usize()
    }

    /// Returns the bucket the encoded key `key` falls in.
    #[inline]
    pub(crate) fn slot(self, key: u128) -> usize {
        ((key >> self.shift) & ((1u128 << self.width) - 1)) as usize
    }

    /// Highest window of at most `width` bits over the varying bits.
    ///
    /// The window always ends at the top varying bit, so the pass removes the
    /// highest difference the slice still has.
    pub(crate) fn top(varying: u128, width: usize) -> Self {
        let top = bit_len(varying);
        let window = u128::MAX << top.saturating_sub(width.max(1));
        let shift = (varying & window).trailing_zeros() as usize;
        Self {
            shift,
            width: (top - shift).max(1),
        }
    }
}

/// Number of bits needed to represent `value`; 0 for 0.
#[inline]
pub(crate) fn bit_len(value: u128) -> usize {
    (u128::BITS - value.leading_zeros()) as usize
}

/// Widest window [`Direct`] fills in place, in bits.
pub(crate) const FILL_BITS: usize = 8;

/// Buckets such a window can hold.
const FILL_BUCKETS: usize = 1 << FILL_BITS;

/// Computes the encoded key of a record and provides that key source's scans.
///
/// `ByKey` calls a user key function, `Direct` encodes the record itself,
/// and [`Narrowed`] restricts another extractor to one 64-bit half of a wide
/// key.
pub(crate) trait Extract<T> {
    /// The encoded key type.
    type Value: Unsigned;

    /// Returns the encoded key of `record`.
    fn key(&mut self, record: &T) -> Self::Value;

    /// Whether reading a key runs user code, which the cost model prices higher
    /// than a load.
    fn key_function() -> bool;

    /// Returns the offset of the first adjacent pair whose keys descend.
    ///
    /// Retaining the previous key avoids evaluating it twice, so the key
    /// function runs once per scanned record.
    fn inversion(&mut self, records: &[T]) -> Option<usize>
    where
        Self: Sized,
    {
        crate::scan::scalar_by(records, self)
    }

    /// Sorts `records` stably by insertion within `budget` swaps.
    ///
    /// Returns the number of swaps, or `None` once the budget runs out; the
    /// records are then a stable permutation of their input.
    fn insertion_sort(&mut self, records: &mut [T], budget: usize) -> Option<usize>
    where
        Self: Sized,
    {
        crate::insertion::run(records, budget, &mut crate::insertion::Keyed(self))
    }

    /// Fills every run of `records` when `digit` covers all remaining key
    /// differences, and reports whether it did.
    ///
    /// Filling a run from one of its records preserves order and stability only
    /// where equal keys imply identical records, which a user key function does
    /// not guarantee. The default declines and leaves the slice to a counting
    /// pass instead.
    fn fill_window(&mut self, _records: &mut [T], _digit: Digit) -> bool {
        false
    }

    /// Fills a window using parallel counting where the key source supports it.
    ///
    /// Has the same window and interchangeability contract as
    /// [`Self::fill_window`]. User key functions retain the serial fallback.
    #[cfg(feature = "rayon")]
    fn fill_window_parallel(&mut self, records: &mut [T], digit: Digit) -> bool
    where
        T: Send,
    {
        self.fill_window(records, digit)
    }
}

/// Forwards an extractor through a mutable borrow, keeping its scans.
impl<T, E: Extract<T>> Extract<T> for &mut E {
    type Value = E::Value;

    #[inline]
    fn key(&mut self, record: &T) -> Self::Value {
        (**self).key(record)
    }

    #[inline]
    fn key_function() -> bool {
        E::key_function()
    }

    #[inline]
    fn inversion(&mut self, records: &[T]) -> Option<usize> {
        (**self).inversion(records)
    }

    #[inline]
    fn insertion_sort(&mut self, records: &mut [T], budget: usize) -> Option<usize> {
        (**self).insertion_sort(records, budget)
    }

    #[inline]
    fn fill_window(&mut self, records: &mut [T], digit: Digit) -> bool {
        (**self).fill_window(records, digit)
    }

    #[cfg(feature = "rayon")]
    #[inline]
    fn fill_window_parallel(&mut self, records: &mut [T], digit: Digit) -> bool
    where
        T: Send,
    {
        (**self).fill_window_parallel(records, digit)
    }
}

/// Extracts keys with a user key function, which may panic or be inconsistent.
#[derive(Clone)]
pub(crate) struct ByKey<F>(pub(crate) F);

impl<T, K: RadixKey, F: FnMut(&T) -> K> Extract<T> for ByKey<F> {
    type Value = K::Encoded;

    #[inline]
    fn key(&mut self, record: &T) -> Self::Value {
        (self.0)(record).encode()
    }

    #[inline]
    fn key_function() -> bool {
        true
    }
}

/// Uses the record itself as the key; deterministic and panic-free.
#[derive(Clone, Copy)]
pub(crate) struct Direct;

impl<K: RadixKey> Extract<K> for Direct {
    type Value = K::Encoded;

    #[inline]
    fn key(&mut self, record: &K) -> K::Encoded {
        record.encode()
    }

    #[inline]
    fn key_function() -> bool {
        false
    }

    #[inline]
    fn inversion(&mut self, records: &[K]) -> Option<usize> {
        K::inversion(records)
    }

    #[inline]
    fn insertion_sort(&mut self, records: &mut [K], budget: usize) -> Option<usize> {
        K::insertion_pass(records, budget)
    }

    /// A sealed encoding is a bijection on bit patterns, so a run of equal keys
    /// holds interchangeable records: one of them stands for all of them, NaN
    /// payloads and signed zeros included.
    fn fill_window(&mut self, records: &mut [K], digit: Digit) -> bool {
        let Some(&first) = records.first() else {
            return true;
        };
        if digit.width == 0 || digit.width > FILL_BITS {
            return false;
        }
        let buckets = 1usize << digit.width;
        let mut counts = [0usize; FILL_BUCKETS];
        let mut representatives = [first; FILL_BUCKETS];
        for record in records.iter() {
            let bucket = digit.classify(self, record);
            if counts[bucket] == 0 {
                representatives[bucket] = *record;
            }
            counts[bucket] += 1;
        }
        let mut start = 0;
        for (bucket, &count) in counts[..buckets].iter().enumerate() {
            let end = start + count;
            records[start..end].fill(representatives[bucket]);
            start = end;
        }
        debug_assert_eq!(start, records.len());
        true
    }

    /// Counts in disjoint shards in parallel, merges the counts, then fills the
    /// disjoint runs of the whole slice in bucket order.
    #[cfg(feature = "rayon")]
    fn fill_window_parallel(&mut self, records: &mut [K], digit: Digit) -> bool
    where
        K: Send,
    {
        use rayon::prelude::*;

        if records.is_empty() {
            return true;
        }
        if digit.width == 0 || digit.width > FILL_BITS {
            return false;
        }
        let buckets = 1usize << digit.width;
        let shift = K::Encoded::from_usize(digit.shift);
        let mask = K::Encoded::from_usize((1usize << digit.width) - 1);
        let shard_len = records.len().div_ceil(rayon::current_num_threads()).max(1);
        // Classify from the encoded key directly, so a shard needs only Send
        // records: a representative stays optional and no captured key is
        // shared between the counting threads.
        let partials: Vec<_> = records
            .par_chunks_mut(shard_len)
            .map(move |shard| {
                let mut counts = [0usize; FILL_BUCKETS];
                let mut representatives = [None; FILL_BUCKETS];
                for record in shard.iter() {
                    let bucket = ((record.encode() >> shift) & mask).to_usize();
                    if counts[bucket] == 0 {
                        representatives[bucket] = Some(*record);
                    }
                    counts[bucket] += 1;
                }
                (counts, representatives)
            })
            .collect();
        let mut counts = [0usize; FILL_BUCKETS];
        let mut representatives = [None; FILL_BUCKETS];
        for (local_counts, local_representatives) in partials {
            for bucket in 0..buckets {
                if local_counts[bucket] != 0 {
                    representatives[bucket] = local_representatives[bucket];
                    counts[bucket] += local_counts[bucket];
                }
            }
        }
        let mut remaining = records;
        let mut runs = Vec::with_capacity(buckets);
        for bucket in 0..buckets {
            let (run, rest) = remaining.split_at_mut(counts[bucket]);
            remaining = rest;
            if let Some(record) = representatives[bucket].filter(|_| !run.is_empty()) {
                runs.push((run, record));
            }
        }
        runs.into_par_iter()
            .for_each(|(run, record)| run.fill(record));
        true
    }
}

/// Restricts an extractor to the 64 key bits starting at `SHIFT`.
///
/// The narrower value type keeps wide keys out of the hot loops. Windows need no
/// translation because every window is classified through this extractor: a
/// `Digit` over the narrowed key names the same records as that digit over the
/// inner key.
pub(crate) struct Narrowed<E, const SHIFT: usize>(pub(crate) E);

impl<E: Clone, const SHIFT: usize> Clone for Narrowed<E, SHIFT> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T, E: Extract<T>, const SHIFT: usize> Extract<T> for Narrowed<E, SHIFT> {
    type Value = u64;

    #[inline]
    fn key(&mut self, record: &T) -> u64 {
        (self.0.key(record).to_u128() >> SHIFT) as u64
    }

    #[inline]
    fn key_function() -> bool {
        E::key_function()
    }

    fn fill_window(&mut self, records: &mut [T], digit: Digit) -> bool {
        // The narrowed key is the inner key shifted right, so the window moves
        // with it. The inner key covers every remaining difference because the
        // other half is constant.
        self.0.fill_window(
            records,
            Digit {
                shift: digit.shift + SHIFT,
                width: digit.width,
            },
        )
    }

    #[cfg(feature = "rayon")]
    fn fill_window_parallel(&mut self, records: &mut [T], digit: Digit) -> bool
    where
        T: Send,
    {
        self.0.fill_window_parallel(
            records,
            Digit {
                shift: digit.shift + SHIFT,
                width: digit.width,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insertion;

    #[test]
    fn half_keys_keep_encoded_bits() {
        let key = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210u128;
        assert_eq!(
            Narrowed::<_, 0>(&mut Direct).key(&key),
            0xfedc_ba98_7654_3210
        );
        assert_eq!(
            Narrowed::<_, 64>(&mut Direct).key(&key),
            0x0123_4567_89ab_cdef
        );
    }

    /// Checks the window contract against independently sorted keys: the fill
    /// finishes direct records and declines user keys.
    #[test]
    fn windows_keep_offsets_and_payloads() {
        for (low, width) in [(0usize, 8usize), (9, 3), (57, 7)] {
            let mask = ((1u128 << width) - 1) << low;
            let base = 0x73a5_491b_c621_d8e0_2b93_a3f7_1d05_0000u128 & !mask;
            let input: Vec<_> = (0..65)
                .map(|index| base | (((index * 37) as u128) << low & mask))
                .collect();
            let mut expected = input.clone();
            expected.sort();

            let digit = Digit { shift: low, width };
            let mut keys = input.clone();
            assert!(Direct.fill_window(&mut keys, digit));
            assert_eq!(keys, expected);

            // A user key function keeps its payloads and declines the fill.
            let mut records: Vec<_> = input.iter().copied().zip(0..input.len()).collect();
            let original = records.clone();
            let mut key = ByKey(|record: &(u128, usize)| record.0);
            assert!(!key.fill_window(&mut records, digit));
            assert_eq!(records, original);
        }
    }

    #[test]
    fn insertion_passes_match_sorted_keys() {
        for length in [0u32, 1, 48, 64, 65, 500] {
            let mut keys: Vec<_> = (0..length).map(|index| (index * 37) % length).collect();
            let mut expected = keys.clone();
            expected.sort_unstable();
            let swaps = insertion::run(&mut keys, usize::MAX, &mut insertion::Sealed);
            assert!(swaps.is_some());
            assert_eq!(keys, expected);

            let mut keys: Vec<u32> = (0..length).rev().collect();
            let total = length as usize * length.saturating_sub(1) as usize / 2;
            assert_eq!(
                insertion::run(&mut keys, 10, &mut insertion::Sealed),
                (total <= 10).then_some(total),
            );
            keys.sort_unstable();
            assert_eq!(keys, (0..length).collect::<Vec<_>>());
        }
    }
}
