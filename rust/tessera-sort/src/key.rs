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
        fn first_inversion(keys: &[Self]) -> Option<usize>
        where
            Self: RadixKey,
        {
            crate::scan::first_inversion(keys)
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
            #[inline]
            fn first_inversion(keys: &[Self]) -> Option<usize> {
                $default(keys)
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

/// Computes the encoded key of a record.
///
/// `ByKey` calls a user key function, `Direct` encodes the record itself, and
/// `KeyHalf` narrows another extractor to one 64-bit half of a 128-bit key.
pub(crate) trait Extract<T> {
    type Value: Unsigned;
    fn key(&mut self, record: &T) -> Self::Value;

    /// Returns the offset of the first adjacent pair whose keys descend.
    ///
    /// The default keeps the previous key, so it calls the key function once
    /// per record instead of once per compared pair.
    #[inline]
    fn first_inversion(&mut self, records: &[T]) -> Option<usize> {
        let mut records = records.iter();
        let first = records.next()?;
        let mut previous = self.key(first);
        for (index, record) in records.enumerate() {
            let current = self.key(record);
            if previous > current {
                return Some(index);
            }
            previous = current;
        }
        None
    }

    /// Stable insertion sort with the given swap budget.
    ///
    /// The default scans through the key function once per record. Direct
    /// extraction overrides it to resolve the vector backend once per call.
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
    #[inline]
    fn key(&mut self, record: &K) -> Self::Value {
        record.encode()
    }

    #[inline]
    fn first_inversion(&mut self, records: &[K]) -> Option<usize> {
        K::first_inversion(records)
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
}
