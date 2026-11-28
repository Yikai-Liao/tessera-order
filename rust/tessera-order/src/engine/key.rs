// SPDX-License-Identifier: BSD-2-Clause

//! Key encoding and key extraction.
//!
//! [`RadixKey`] maps each supported key to an unsigned integer whose order is
//! the key order. [`Extract`] computes that integer for a record, either from a
//! user key function ([`ByKey`]) or from the record itself ([`Direct`]), and
//! selects the scan an insertion pass uses.

use crate::engine::block::Classify;
use core::cmp::Reverse;
use core::num::NonZero;
use core::ops::{BitAnd, BitOrAssign, Not, Shl, Shr};

/// A key type that Tessera can sort by.
///
/// Implemented for `bool`, `char`, every primitive integer from `u8`/`i8` to
/// `u128`/`i128` (including `usize` and `isize`), [`NonZero<T>`](NonZero) for
/// every primitive integer `T`, `f32` and `f64`, fixed-size byte arrays
/// `[u8; 1]` through `[u8; 16]`, [`Reverse<K>`](Reverse) of any supported key,
/// and tuples of 2 to 16 supported keys whose encoded widths sum to at most
/// 128 bits. The trait is sealed: other types, empty or over-wide byte arrays,
/// 1-tuples, and tuples wider than 128 bits cannot implement it, so an
/// unsupported key is a compile error.
///
/// Integers, `bool`, `char`, [`NonZero<T>`](NonZero) and byte arrays sort in
/// their standard [`Ord`] order, `f32` and `f64` follow [`f64::total_cmp`],
/// [`Reverse<K>`](Reverse) reverses the order of `K` while preserving stability
/// for equal keys, and tuples compare lexicographically by each field's Tessera
/// key order (so floats inside tuples follow `total_cmp`, e.g. `(false, -0.0)`
/// sorts before `(false, +0.0)`). See
/// [`conformance/semantics.md`](https://github.com/Yikai-Liao/tessera-order/blob/main/conformance/semantics.md#key-encoding)
/// for the normative bit-level encoding and padding rules.
///
/// Unsupported byte-array lengths, one-field tuples, and tuples whose fields
/// exceed 128 encoded bits fail to compile:
///
/// ```compile_fail
/// let mut keys = [[0u8; 0]];
/// tessera_order::sort(&mut keys);
/// ```
///
/// ```compile_fail
/// let mut keys = [[0u8; 17]];
/// tessera_order::sort(&mut keys);
/// ```
///
/// ```compile_fail
/// let mut keys = [([0u8; 13], 'a')];
/// tessera_order::sort(&mut keys);
/// ```
///
/// ```compile_fail
/// let mut keys = [(0u8,)];
/// tessera_order::sort(&mut keys);
/// ```
///
/// ```compile_fail
/// let mut keys = [(0u64, 0u64, 0u8)];
/// tessera_order::sort(&mut keys);
/// ```
///
/// ```compile_fail
/// let mut keys = [(0u128, 0u8)];
/// tessera_order::sort(&mut keys);
/// ```
pub trait RadixKey: sealed::Sealed + Copy + Send + Sync {
    #[doc(hidden)]
    type Encoded: Unsigned;
    #[doc(hidden)]
    type Width: BitWidth;
    #[doc(hidden)]
    fn encode(self) -> Self::Encoded;
    /// Mask of encoded bits that can take both 0 and 1 across valid values
    /// of this key, used by root variation scans to stop early.
    #[doc(hidden)]
    const VARYING_MASK: u128 = Self::Width::MASK;
    /// Inverts [`Self::encode`] bit for bit, including NaN payloads.
    #[doc(hidden)]
    fn decode(encoded: Self::Encoded) -> Self;
}

mod sealed {
    /// Seals [`super::RadixKey`] so external crates cannot implement it.
    pub trait Sealed: crate::engine::portable_simd::SimdKey {}
    impl<T: crate::engine::portable_simd::SimdKey> Sealed for T {}
}

/// Sealed type-level encoded bit width (multiples of 8 up to 128) for packing
/// tuple keys into the narrowest covering unsigned integer.
#[doc(hidden)]
pub trait BitWidth {
    const BITS: usize;
    const MASK: u128 = u128::MAX >> (128 - Self::BITS);
    type Encoded: Unsigned;
}

/// Sealed width addition defined only when `Self::BITS + Rhs::BITS <= 128`.
#[doc(hidden)]
pub trait SumWidth<Rhs: BitWidth>: BitWidth {
    type Sum: BitWidth;
}

/// Type-level key width in bits, instantiated for multiples of 8 up to 128.
#[doc(hidden)]
pub struct Width<const BITS: usize>;

macro_rules! bit_widths {
    ($($enc:ty: $($bits:literal),+);* $(;)?) => {$($(
        impl BitWidth for Width<$bits> {
            const BITS: usize = $bits;
            type Encoded = $enc;
        }
    )+)*};
}
bit_widths!(
    u8: 8;
    u16: 16;
    u32: 24, 32;
    u64: 40, 48, 56, 64;
    u128: 72, 80, 88, 96, 104, 112, 120, 128;
);

// Generates SumWidth<Width<B>> for Width<A> whenever A + B <= 128.
macro_rules! sum_widths {
    (@row $lhs:literal; [$sum:literal $(, $srest:literal)*]; [$rhs:literal $(, $rrest:literal)*]) => {
        impl SumWidth<Width<$rhs>> for Width<$lhs> { type Sum = Width<$sum>; }
        sum_widths!(@row $lhs; [$($srest),*]; [$($rrest),*]);
    };
    (@row $lhs:literal; []; [$($rrest:literal),*]) => {};
    ($lhs:literal; $first_sum:literal $(, $rest:literal)*) => {
        sum_widths!(@row $lhs; [$first_sum $(, $rest)*]; [8, 16, 24, 32, 40, 48, 56, 64, 72, 80, 88, 96, 104, 112, 120]);
        sum_widths!($first_sum; $($rest),*);
    };
    ($last:literal;) => {};
}
sum_widths!(8; 16, 24, 32, 40, 48, 56, 64, 72, 80, 88, 96, 104, 112, 120, 128);

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
    const MAX: Self;
    fn from_usize(value: usize) -> Self;
    fn from_u128(value: u128) -> Self;
    fn to_usize(self) -> usize;
    fn to_u128(self) -> u128;
}

macro_rules! unsigned {
    ($($ty:ty),* $(,)?) => {$(
        impl Unsigned for $ty {
            const BITS: usize = <$ty>::BITS as usize;
            const MAX: Self = <$ty>::MAX;
            #[inline] fn from_usize(value: usize) -> Self { value as Self }
            #[inline] fn from_u128(value: u128) -> Self { value as Self }
            #[inline] fn to_usize(self) -> usize { self as usize }
            #[inline] fn to_u128(self) -> u128 { self as u128 }
        }
        impl RadixKey for $ty {
            type Encoded = Self;
            type Width = Width<{ <$ty>::BITS as usize }>;
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
            type Width = Width<{ <$u>::BITS as usize }>;
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
            type Width = Width<{ <$u>::BITS as usize }>;
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
    type Width = K::Width;
    const VARYING_MASK: u128 = K::VARYING_MASK;
    #[inline]
    fn encode(self) -> Self::Encoded {
        !self.0.encode() & K::Encoded::from_u128(K::Width::MASK)
    }
    #[inline]
    fn decode(encoded: Self::Encoded) -> Self {
        Reverse(K::decode(!encoded & K::Encoded::from_u128(K::Width::MASK)))
    }
}

impl<A: RadixKey, B: RadixKey> RadixKey for (A, B)
where
    A::Width: SumWidth<B::Width>,
{
    type Width = <A::Width as SumWidth<B::Width>>::Sum;
    type Encoded = <Self::Width as BitWidth>::Encoded;
    const VARYING_MASK: u128 = (A::VARYING_MASK << B::Width::BITS) | B::VARYING_MASK;

    #[inline]
    fn encode(self) -> Self::Encoded {
        Self::Encoded::from_u128(
            (self.0.encode().to_u128() << B::Width::BITS) | self.1.encode().to_u128(),
        )
    }

    #[inline]
    fn decode(encoded: Self::Encoded) -> Self {
        let raw = encoded.to_u128();
        let lo = B::decode(B::Encoded::from_u128(raw & B::Width::MASK));
        let hi = A::decode(A::Encoded::from_u128(raw >> B::Width::BITS));
        (hi, lo)
    }
}

macro_rules! tuple_keys {
    (@step [$($head:ident),+]; $last:ident $(, $rest:ident)*) => {
        impl<$($head: RadixKey,)+ $last: RadixKey> RadixKey for ($($head,)+ $last)
        where
            ($($head,)+): RadixKey,
            <($($head,)+) as RadixKey>::Width: SumWidth<$last::Width>,
        {
            type Width = <<($($head,)+) as RadixKey>::Width as SumWidth<$last::Width>>::Sum;
            type Encoded = <Self::Width as BitWidth>::Encoded;
            const VARYING_MASK: u128 = <(($($head,)+), $last) as RadixKey>::VARYING_MASK;

            #[inline]
            fn encode(self) -> Self::Encoded {
                #[allow(non_snake_case)]
                let ($($head,)+ $last) = self;
                (($($head,)+), $last).encode()
            }

            #[inline]
            fn decode(encoded: Self::Encoded) -> Self {
                #[allow(non_snake_case)]
                let (($($head,)+), $last) = <(($($head,)+), $last) as RadixKey>::decode(encoded);
                ($($head,)+ $last)
            }
        }
        tuple_keys!(@step [$($head,)+ $last]; $($rest),*);
    };
    (@step [$($head:ident),+];) => {};
}
tuple_keys!(@step [A, B]; C, D, E, F, G, H, I, J, K, L, M, N, O, P);

impl RadixKey for bool {
    type Encoded = u8;
    type Width = Width<8>;
    const VARYING_MASK: u128 = 1;

    #[inline]
    fn encode(self) -> u8 {
        u8::from(self)
    }

    #[inline]
    fn decode(encoded: u8) -> bool {
        debug_assert!(encoded <= 1);
        encoded != 0
    }
}

impl RadixKey for char {
    type Encoded = u32;
    type Width = Width<32>;
    const VARYING_MASK: u128 = 0x001f_ffff;

    #[inline]
    fn encode(self) -> u32 {
        u32::from(self)
    }

    #[inline]
    fn decode(encoded: u32) -> char {
        char::from_u32(encoded).expect("valid encoded char")
    }
}

macro_rules! non_zero {
    ($($ty:ty),* $(,)?) => {$(
        impl RadixKey for NonZero<$ty> {
            type Encoded = <$ty as RadixKey>::Encoded;
            type Width = <$ty as RadixKey>::Width;

            #[inline]
            fn encode(self) -> Self::Encoded {
                self.get().encode()
            }

            #[inline]
            fn decode(encoded: Self::Encoded) -> Self {
                NonZero::new(<$ty>::decode(encoded)).expect("valid encoded non-zero integer")
            }
        }
    )*};
}
non_zero!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

macro_rules! byte_arrays {
    ($($n:literal),* $(,)?) => {$(
        impl RadixKey for [u8; $n] {
            type Width = Width<{ $n * 8 }>;
            type Encoded = <Self::Width as BitWidth>::Encoded;

            #[inline]
            fn encode(self) -> Self::Encoded {
                let mut buf = [0u8; 16];
                buf[16 - $n..].copy_from_slice(&self);
                Self::Encoded::from_u128(u128::from_be_bytes(buf))
            }

            #[inline]
            fn decode(encoded: Self::Encoded) -> Self {
                let bytes = encoded.to_u128().to_be_bytes();
                let mut out = [0u8; $n];
                out.copy_from_slice(&bytes[16 - $n..]);
                out
            }
        }
    )*};
}
byte_arrays!(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);

/// Widest key window that [`try_count_window`] counts and fills, in bits.
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
/// If `DIRECT` is present, `T` must be a sealed [`RadixKey`] type and `key`
/// must be its deterministic, panic-free encoded-key projection starting at
/// [`DirectKeys::shift`]. Tile scatter may recompute that projection after
/// ownership transfer has started. The counting functions must operate on the
/// complete record encoding and preserve its bits outside the requested window.
/// A projection need not identify the complete record: counting callers must
/// establish that all complete encodings agree outside the translated window.
pub(crate) unsafe trait Extract<T> {
    /// Present only for sealed native-key records, including narrowed keys.
    /// This one capability lets tiles recompute classification and lets a
    /// window covering all full-record variation finish by counting/refilling.
    /// User callbacks keep the default: their keys do not identify payloads.
    const DIRECT: Option<DirectKeys<T>> = None;

    /// The encoded key type.
    type Value: Unsigned;

    /// Mask of bits that may vary in keys produced by this extractor.
    /// Padded keys (tuples, byte arrays) and narrow-range keys (`bool`, `char`)
    /// exclude their guaranteed-constant bits so the root variation scan still
    /// stops early once every varying bit has taken both states.
    const KEY_MASK: u128 = u128::MAX >> (128 - Self::Value::BITS);

    /// Returns the encoded key of `record`.
    fn key(&mut self, record: &T) -> Self::Value;

    /// Returns whether `previous` and the keys of `records` are in ascending
    /// order. `previous` is the already encoded key of the preceding record.
    fn confirm_ascending(&mut self, records: &[T], previous: Self::Value) -> bool
    where
        Self: Sized,
    {
        crate::engine::order::confirm_ascending(records, self, previous)
    }

    /// Sorts `records` stably by insertion, without a swap limit.
    ///
    /// Direct extraction selects its scan once for the whole pass.
    fn finish_insertion(&mut self, records: &mut [T])
    where
        Self: Sized,
    {
        crate::engine::order::finish_insertion(records, self, crate::engine::order::by_key);
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
        crate::engine::order::insertion_loop(records, budget, self, crate::engine::order::by_key)
    }
}

/// Lends an extractor to an operation that takes its classification state by
/// value, such as a block pass, without changing its scans or caching policy.
// SAFETY: forwards the extraction and full-record counting capability unchanged.
unsafe impl<T, E: Extract<T>> Extract<T> for &mut E {
    const DIRECT: Option<DirectKeys<T>> = E::DIRECT;
    type Value = E::Value;
    const KEY_MASK: u128 = E::KEY_MASK;

    #[inline]
    fn key(&mut self, record: &T) -> Self::Value {
        (**self).key(record)
    }

    #[inline]
    fn confirm_ascending(&mut self, records: &[T], previous: Self::Value) -> bool {
        (**self).confirm_ascending(records, previous)
    }

    #[inline]
    fn finish_insertion(&mut self, records: &mut [T]) {
        (**self).finish_insertion(records);
    }

    #[inline]
    fn insertion_sort(&mut self, records: &mut [T], budget: usize) -> Option<usize> {
        (**self).insertion_sort(records, budget)
    }
}

/// Number of bits needed to represent `value`; 0 for 0.
#[inline]
pub(crate) fn bit_len(value: u128) -> usize {
    (u128::BITS - value.leading_zeros()) as usize
}

/// One pass worth of key bits: `width` bits starting at `shift`.
#[derive(Clone, Copy)]
pub(crate) struct Digit {
    pub(crate) shift: usize,
    pub(crate) width: usize,
}

impl Digit {
    #[inline]
    pub(crate) fn mask<V: Unsigned>(self) -> V {
        (V::from_usize((1usize << self.width) - 1)) << V::from_usize(self.shift)
    }
}

/// Classifies records by the key digit that starts at bit `shift`.
///
/// The partition keeps the digit's low bits; bits above the key width count
/// as zero.
#[derive(Clone)]
pub(crate) struct KeyDigit<E, V> {
    pub extractor: E,
    shift: V,
}

impl<E, V: Unsigned> KeyDigit<E, V> {
    /// Classifies by the digit at `shift`, which must be below the key width.
    pub(crate) fn new(extractor: E, shift: usize) -> Self {
        assert!(shift < V::BITS);
        Self {
            extractor,
            shift: V::from_usize(shift),
        }
    }
}

impl<T, E: Extract<T>> Classify<T> for KeyDigit<E, E::Value> {
    #[inline]
    fn bucket(&mut self, record: &T) -> usize {
        (self.extractor.key(record) >> self.shift).to_usize()
    }
}

/// Classifies records against a sampled key and the node's highest varying
/// bit, as buckets of a two-bit partition.
///
/// The bucket is the rank below (0), equal to (1) or above (2) the sampled
/// key, plus one at or above the smallest key that shares the sampled key's
/// bits above the top varying bit and sets that bit. For consistent keys every
/// bit above it is shared, so the four buckets stay in key order, the equal
/// key fills [`Self::equal_bucket`] alone, and every other bucket holds the
/// top bit constant: an equality split consumes that bit like any digit split.
#[derive(Clone)]
pub(crate) struct Rank<E, V> {
    pub extractor: E,
    equal: V,
    /// The first key above `equal`'s prefix that sets the top varying bit.
    split: V,
}

impl<E, V: Unsigned> Rank<E, V> {
    /// Ranks against `equal` and the highest bit of a nonzero `varying`.
    pub(crate) fn new(extractor: E, equal: V, varying: u128) -> Self {
        let bit = V::from_usize(varying.ilog2() as usize);
        let mut split = equal & (!V::from_usize(0) << bit << V::from_usize(1));
        split |= V::from_usize(1) << bit;
        Self {
            extractor,
            equal,
            split,
        }
    }

    /// The bucket that receives exactly the sampled key; it is complete.
    pub(crate) fn equal_bucket(&self) -> usize {
        1 + usize::from(self.equal >= self.split)
    }
}

impl<T, E: Extract<T>> Classify<T> for Rank<E, E::Value> {
    /// Full-width comparisons: byte arithmetic writes partial registers and
    /// made the scatter loop wait on the previous record's bucket pointer
    /// (1.5x slower on `u64/two_values/65536`).
    #[inline]
    fn bucket(&mut self, record: &T) -> usize {
        let key = self.extractor.key(record);
        usize::from(key >= self.equal)
            + usize::from(key > self.equal)
            + usize::from(key >= self.split)
    }
}

/// Extracts keys with a user key function, which may panic or be inconsistent.
#[derive(Clone)]
pub(crate) struct ByKey<F>(pub F);
// SAFETY: user callbacks retain cached classifications, even if they panic or change keys.
unsafe impl<T, K: RadixKey, F: FnMut(&T) -> K> Extract<T> for ByKey<F> {
    type Value = K::Encoded;
    const KEY_MASK: u128 = K::VARYING_MASK;
    #[inline]
    fn key(&mut self, record: &T) -> Self::Value {
        (self.0)(record).encode()
    }
}

/// Uses the record itself as the key; deterministic and panic-free.
#[derive(Clone, Copy)]
pub(crate) struct Direct;

// SAFETY: sealed key encodings are deterministic, panic-free and bitwise
// bijective. Counting uses that same complete encoding, with no projection.
unsafe impl<K: RadixKey> Extract<K> for Direct {
    const DIRECT: Option<DirectKeys<K>> = Some(DirectKeys {
        shift: 0,
        count: count_keys::<K>,
        can_finish_msd: crate::engine::portable_simd::can_finish_msd::<K>,
        finish_msd: crate::engine::portable_simd::finish_msd::<K>,
    });
    type Value = K::Encoded;
    const KEY_MASK: u128 = K::VARYING_MASK;

    #[inline]
    fn confirm_ascending(&mut self, records: &[K], previous: Self::Value) -> bool {
        crate::engine::portable_simd::confirm_ascending(records, previous)
    }

    #[inline]
    fn key(&mut self, record: &K) -> Self::Value {
        record.encode()
    }

    #[inline]
    fn finish_insertion(&mut self, records: &mut [K]) {
        crate::engine::portable_simd::finish_insertion(records);
    }

    #[inline]
    fn insertion_sort(&mut self, records: &mut [K], budget: usize) -> Option<usize> {
        crate::engine::portable_simd::insertion_sort(records, budget)
    }
}

/// Counting and SIMD finishing access to the complete encoding of sealed native-key records.
///
/// Only [`Extract::DIRECT`] supplies it, so classification caching and
/// direct-key finishing eligibility cannot disagree. Counting retains the shared
/// bits outside its full-record window; an extractor's projection alone need not
/// identify a record.
pub(crate) struct DirectKeys<T> {
    /// Position of the extractor's key bit 0 in the record's encoded key.
    shift: usize,
    /// Counts/fills a window in the full encoding, whose outside bits agree.
    /// The final flag selects parallel execution when the `rayon` feature is on.
    count: fn(&mut [T], usize, usize, bool),
    /// Whether SIMD MSD finishing supports this key type on the active CPU.
    pub(crate) can_finish_msd: fn() -> bool,
    /// Finishes MSD-partitioned records in place when bounded by `largest`.
    pub(crate) finish_msd: fn(&mut [T], usize) -> bool,
}

/// Finishes a slice by counting a small encoded-key window and decoding its runs.
///
/// The window starts at bit `low` of `E::Value` and is `width` bits wide, at most
/// [`SMALL_WINDOW_BITS`]. For a direct extractor, the caller guarantees that it
/// fits the projection and that all complete record encodings agree outside
/// the window starting at `low + DirectKeys::shift`. This includes any bits
/// omitted by `KeyHalf`: a constant omitted half may be nonzero. Counting
/// retains those outside bits when decoding the runs. Extractors without
/// [`Extract::DIRECT`] decline and leave the payload records unchanged.
///
/// Returns whether the slice is finished.
pub(crate) fn try_count_window<T, E: Extract<T>>(
    records: &mut [T],
    low: usize,
    width: usize,
    parallel: bool,
) -> bool {
    match E::DIRECT {
        Some(direct) if (1..=SMALL_WINDOW_BITS).contains(&width) => {
            if !records.is_empty() {
                (direct.count)(records, low + direct.shift, width, parallel);
            }
            true
        }
        _ => false,
    }
}

fn count_keys<K: RadixKey>(records: &mut [K], low: usize, width: usize, parallel: bool) {
    #[cfg(feature = "rayon")]
    if parallel {
        use rayon::prelude::*;

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
        return;
    }
    #[cfg(not(feature = "rayon"))]
    let _ = parallel;
    for (run, key) in KeyCounts::count(records, low, width).runs(records) {
        run.fill(key);
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
    /// Full encodings must agree outside that window, including omitted halves.
    fn count(records: &[K], low: usize, width: usize) -> Self {
        // Establish the table bound once, not at every histogram update.
        assert!(width <= SMALL_WINDOW_BITS);
        let buckets = 1 << width;
        let mask = K::Encoded::from_usize(buckets - 1);
        let shift = K::Encoded::from_usize(low);
        let mut summary = Self {
            low,
            buckets,
            counts: [0; SMALL_WINDOW_BUCKETS],
            outside: records[0].encode() & !(mask << shift),
        };
        for &record in records {
            summary.counts[((record.encode() >> shift) & mask).to_usize()] += 1;
        }
        summary
    }

    /// Combines disjoint shards with the same window and shared outside bits.
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
    fn runs<'a>(&self, mut records: &'a mut [K]) -> impl Iterator<Item = (&'a mut [K], K)> {
        let (outside, shift) = (self.outside, K::Encoded::from_usize(self.low));
        self.counts[..self.buckets]
            .iter()
            .copied()
            .enumerate()
            .filter_map(move |(bucket, count)| {
                let (run, rest) = core::mem::take(&mut records).split_at_mut(count);
                records = rest;
                (count != 0).then(|| {
                    let mut key = outside;
                    key |= K::Encoded::from_usize(bucket) << shift;
                    (run, K::decode(key))
                })
            })
    }
}

/// Extracts the 64 key bits starting at `SHIFT` when the other half is constant.
///
/// Scans retain the defaults for the narrowed keys, not the inner key width.
/// Counting still uses the complete record encoding, including that constant
/// half; narrowing does not make the projection identify the complete record.
#[derive(Clone)]
pub(crate) struct KeyHalf<E, const SHIFT: usize>(pub E);

// SAFETY: the inner extractor owns deterministic, panic-free direct extraction;
// the wrapping projection cannot panic and translates only the window offset.
// Counting retains the inner functions over complete record encodings. Its
// callers must establish agreement outside the translated window.
unsafe impl<T, E: Extract<T>, const SHIFT: usize> Extract<T> for KeyHalf<E, SHIFT> {
    // Root narrowing establishes that the omitted half is constant; count/fill
    // preserves it through the inner functions rather than reconstructing it
    // from this projection.
    const DIRECT: Option<DirectKeys<T>> = match E::DIRECT {
        Some(direct) => Some(DirectKeys {
            shift: direct.shift + SHIFT,
            ..direct
        }),
        None => None,
    };
    type Value = u64;

    #[inline]
    fn key(&mut self, record: &T) -> u64 {
        self.0.key(record).to_u128().wrapping_shr(SHIFT as u32) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    // This enumerates the arithmetic classification rule, not record movement.
    // Miri exercises Rank through the engine's ownership and recovery fixtures.
    #[test]
    #[cfg_attr(miri, ignore = "exhaustive arithmetic is checked by native tests")]
    fn equality_digits_order_keys_and_hold_the_top_bit() {
        // Every pivot and every highest varying bit of a node whose keys share
        // the bits above it: digits ascend with keys, the equal bucket holds
        // exactly the pivot and every bucket holds the top bit constant.
        for top in 0..8 {
            let high = !0u8 << top << 1;
            for prefix in (0..=255u8).filter(|key| key & !high == 0) {
                let keys: Vec<u8> = (0..=255u8).filter(|key| key & high == prefix).collect();
                for &pivot in &keys {
                    let mut rank = Rank::new(Direct, pivot, 1u128 << top);
                    let equal = rank.equal_bucket();
                    let mut top_bits = [None; 4];
                    let mut previous = 0;
                    for &key in &keys {
                        let digit = rank.bucket(&key);
                        assert!(previous <= digit && digit < 4);
                        assert_eq!(digit == equal, key == pivot);
                        let bit = key >> top & 1;
                        assert_eq!(*top_bits[digit].get_or_insert(bit), bit);
                        previous = digit;
                    }
                }
            }
        }
    }

    #[test]
    fn half_keys_keep_encoded_bits() {
        let key = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210u128;
        assert_eq!(KeyHalf::<_, 0>(Direct).key(&key), 0xfedc_ba98_7654_3210);
        assert_eq!(KeyHalf::<_, 64>(Direct).key(&key), 0x0123_4567_89ab_cdef);
    }

    fn count<T, E: Extract<T>>(_: &E, records: &mut [T], low: usize, width: usize) -> bool {
        try_count_window::<T, E>(records, low, width, false)
    }

    #[cfg(all(feature = "rayon", not(miri)))]
    fn count_parallel<T, E: Extract<T>>(
        _: &E,
        records: &mut [T],
        low: usize,
        width: usize,
    ) -> bool {
        try_count_window::<T, E>(records, low, width, true)
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
            assert!(count(direct_half, &mut keys, low, width));
            assert!(!count(keyed_half, &mut records, low, width));
            assert_eq!(keys, expected);
            assert_eq!(records, original);

            // Miri interprets both storage forms without invoking Rayon's pool.
            #[cfg(all(feature = "rayon", not(miri)))]
            {
                let mut keys = input;
                assert!(count_parallel(direct_half, &mut keys, low, width));
                assert!(!count_parallel(keyed_half, &mut records, low, width));
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

        fn check_ordered<K: RadixKey + Eq>(sorted: &[K]) {
            let (mut prev, mut prev_rev) = (None, None);
            for &key in sorted {
                let enc = key.encode();
                assert_eq!(enc.to_u128() & !K::Width::MASK, 0);
                assert!(K::decode(enc) == key);
                assert!(prev.replace(enc).is_none_or(|p| p < enc));

                let rev = Reverse(key);
                let rev_enc = rev.encode();
                assert_eq!(rev_enc.to_u128() & !K::Width::MASK, 0);
                assert!(Reverse::<K>::decode(rev_enc) == rev);
                assert!(prev_rev.replace(rev_enc).is_none_or(|p| p > rev_enc));
            }
        }

        assert_eq!(<bool as RadixKey>::Width::BITS, 8);
        assert_eq!(<char as RadixKey>::Width::BITS, 32);
        assert_eq!(<([u8; 12], char) as RadixKey>::Width::BITS, 128);
        assert_eq!(false.encode(), 0u8);
        assert_eq!(true.encode(), 1u8);
        assert_eq!('A'.encode(), 0x41u32);
        assert_eq!(char::MAX.encode(), 0x10_ffffu32);
        check_ordered(&[false, true]);
        check_ordered(&['\0', 'A', '\u{d7ff}', '\u{e000}', '🦀', char::MAX]);

        macro_rules! check_nz {
            ($($ty:ty: [$($mid:expr),*]),* $(,)?) => {$(
                assert_eq!(<NonZero<$ty> as RadixKey>::Width::BITS, <$ty as RadixKey>::Width::BITS);
                check_ordered(&[NonZero::<$ty>::MIN, $(NonZero::new($mid as $ty).unwrap(),)* NonZero::<$ty>::MAX]);
            )*};
        }
        check_nz!(
            u8: [2], u16: [2], u32: [2], u64: [2], u128: [2], usize: [2],
            i8: [-1, 1], i16: [-1, 1], i32: [-1, 1], i64: [-1, 1], i128: [-1, 1], isize: [-1, 1],
        );

        macro_rules! check_bytes {
            ($($n:literal),*) => {$(
                assert_eq!(<[u8; $n] as RadixKey>::Width::BITS, $n * 8);
                assert_eq!(
                    size_of::<<[u8; $n] as RadixKey>::Encoded>(),
                    usize::next_power_of_two($n)
                );
                let (mut lo, mut hi) = ([0u8; $n], [0u8; $n]);
                (lo[$n - 1], hi[0]) = (1, 0x11);
                let mid = core::array::from_fn(|i| (0x10 + i) as u8);
                check_ordered(&[[0u8; $n], lo, mid, hi, [0xffu8; $n]]);
            )*};
        }
        check_bytes!(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);

        let z = [0u8; 12];
        check_ordered(&[(z, '\0'), (z, char::MAX), ([0xff; 12], 'a')]);
        let nz = |v| NonZero::new(v).unwrap();
        check_ordered(&[
            (false, NonZero::<i16>::MIN, [0; 3], Reverse(char::MAX)),
            (false, nz(-1i16), [0x12, 0x34, 0x56], Reverse('\u{e000}')),
            (false, nz(1i16), [0x12, 0x34, 0x57], Reverse('\u{d7ff}')),
            (true, NonZero::<i16>::MAX, [0xff; 3], Reverse('\0')),
        ]);
    }

    #[test]
    fn counted_primitive_windows_skip_empty_and_keep_outside_bits() {
        fn check<K: RadixKey + Ord + core::fmt::Debug>(input: &[K], low: usize, width: usize) {
            let (mut actual, mut expected) = (input.to_vec(), input.to_vec());
            expected.sort();
            assert!(count(&Direct, &mut actual, low, width));
            assert_eq!(actual, expected);

            let mut rev: Vec<_> = input.iter().copied().map(Reverse).collect();
            let mut expected_rev = rev.clone();
            expected_rev.sort();
            assert!(count(&Direct, &mut rev, low, width));
            assert_eq!(rev, expected_rev);
        }
        check(&[true, false, true, false], 0, 1);
        check(&['\u{e1ff}', '\u{d7ff}', '\u{e0ff}', '\u{d6ff}'], 8, 6);
        check(&[3u8, 1, 2].map(|v| NonZero::new(v).unwrap()), 0, 2);
        check(&[-2i8, 2, -1, 1].map(|v| NonZero::new(v).unwrap()), 0, 8);
        check(&[0x35, 0x02, 0x3f, 0x00].map(|b| [0xab, b, 0xef]), 8, 6);
    }

    #[test]
    fn counted_floats_keep_exact_bits() {
        // Negative NaN payloads that differ only inside the low window.
        let mut negative: Vec<_> = (0..65)
            .map(|index| f32::from_bits(0xffc0_0000 | (index as u32 * 37 % 256)))
            .collect();
        let mut expected: Vec<_> = negative.iter().map(|key| key.to_bits()).collect();
        expected.sort_by_key(|&bits| f32::from_bits(bits).encode());
        assert!(try_count_window::<_, Direct>(&mut negative, 0, 8, false));
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
                    crate::engine::order::confirm_ascending(
                        records,
                        &mut ByKey(|key: &K| *key),
                        previous,
                    ),
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
