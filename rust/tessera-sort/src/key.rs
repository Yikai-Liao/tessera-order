// SPDX-License-Identifier: BSD-2-Clause

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

    // Backend selection belongs to the sealed key implementations, not the
    // public ordering contract. Reverse keys can use these defaults without
    // acquiring a second set of primitive SIMD implementations.
    pub trait Sealed: Copy {
        #[inline]
        fn confirm_ascending(keys: &[Self], previous: <Self as RadixKey>::Encoded) -> bool
        where
            Self: RadixKey,
        {
            crate::engine::confirm_ascending(keys, &mut Direct, previous)
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
            crate::engine::finish_insertion(keys, &mut Direct);
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

macro_rules! unsigned {
    ($($ty:ty),* $(,)?) => {$(
        impl sealed::Sealed for $ty {
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
    ($(($f:ty, $u:ty, $default:path)),* $(,)?) => {$(
        impl sealed::Sealed for $f {
            #[cfg(feature = "portable-simd")]
            #[inline]
            fn confirm_ascending(keys: &[Self], previous: <Self as RadixKey>::Encoded) -> bool {
                crate::portable_simd::confirm_ascending(keys, previous)
            }

            #[inline(always)]
            fn first_inversion(keys: &[Self]) -> Option<usize> {
                $default(keys)
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
    type Value: Unsigned;

    /// Whether records are their own keys and encoding cannot panic.
    ///
    /// A direct encoder lets a leaf scatter its records from block storage
    /// straight into their final position: no user key function runs while the
    /// leaf is finished, and equal keys imply identical records.
    const DIRECT: bool = false;
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

    /// Confirms ascending keys after an already encoded predecessor.
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

    /// Finishes an insertion sort without a swap limit.
    ///
    /// Direct extraction resolves the vector backend once for the whole pass.
    fn finish_insertion(&mut self, arr: &mut [T])
    where
        Self: Sized,
    {
        crate::engine::finish_insertion(arr, self);
    }

    /// Inserts within a swap budget and returns the completed pass's swap count.
    ///
    /// Direct extraction resolves the vector backend once for the whole pass.
    fn insertion_sort(&mut self, arr: &mut [T], budget: usize) -> Option<usize>
    where
        Self: Sized,
    {
        crate::engine::insertion_loop(arr, budget, self)
    }
}

/// Extracts keys with a user key function, which may panic or be inconsistent.
pub(crate) struct ByKey<F>(pub F);
impl<T, K: RadixKey, F: FnMut(&T) -> K> Extract<T> for ByKey<F> {
    type Value = K::Encoded;
    #[inline]
    fn key(&mut self, record: &T) -> Self::Value {
        (self.0)(record).encode()
    }
}

/// Uses the record itself as the key; deterministic and panic-free.
pub(crate) struct Direct;

impl<K: RadixKey> Extract<K> for Direct {
    type Value = K::Encoded;
    const DIRECT: bool = true;

    fn try_count_window(&mut self, records: &mut [K], low: usize, width: usize) -> bool {
        if width == 0 || width > SMALL_WINDOW_BITS {
            return false;
        }
        // An empty slice and a single record are already sorted.
        let Some(&first) = records.first() else {
            return true;
        };
        let buckets = 1usize << width;
        let mask = Self::Value::from_usize(buckets - 1);
        let shift = Self::Value::from_usize(low);
        let mut counts = [0usize; SMALL_WINDOW_BUCKETS];
        let mut representatives = [first; SMALL_WINDOW_BUCKETS];
        for record in records.iter() {
            let key = self.key(record);
            let bucket = ((key >> shift) & mask).to_usize();
            if counts[bucket] == 0 {
                representatives[bucket] = *record;
            }
            counts[bucket] += 1;
        }
        // Every sealed encoding is a bijection on bit patterns, so equal keys
        // mean equal records: a run of one bucket is interchangeable, and one of
        // its records stands for all of them, NaN payloads and signed zeros
        // included.
        let mut start = 0;
        for (bucket, &count) in counts[..buckets].iter().enumerate() {
            let end = start + count;
            records[start..end].fill(representatives[bucket]);
            start = end;
        }
        debug_assert_eq!(start, records.len());
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

    // Outlining this chain would add a call per scan and hide caller bounds.
    #[inline(always)]
    fn first_inversion(&mut self, records: &[K]) -> Option<usize> {
        K::first_inversion(records)
    }

    #[inline]
    fn finish_insertion(&mut self, arr: &mut [K]) {
        K::finish_insertion(arr);
    }

    #[inline]
    fn insertion_sort(&mut self, arr: &mut [K], budget: usize) -> Option<usize> {
        K::insertion_sort(arr, budget)
    }
}

/// The 64 key bits starting at `SHIFT`, used when only one half of a 128-bit
/// key varies.
pub(crate) struct KeyHalf<'a, E, const SHIFT: usize>(pub &'a mut E);
impl<T, E: Extract<T>, const SHIFT: usize> Extract<T> for KeyHalf<'_, E, SHIFT> {
    type Value = u64;
    #[inline]
    fn key(&mut self, record: &T) -> u64 {
        (self.0.key(record).to_u128() >> SHIFT) as u64
    }

    fn try_count_window(&mut self, records: &mut [T], low: usize, width: usize) -> bool {
        // The engine narrows a key to one half only when the other half is
        // constant, so the window moves up with the half and still covers every
        // difference in the underlying key.
        self.0.try_count_window(records, low + SHIFT, width)
    }
}
