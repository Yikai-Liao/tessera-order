// SPDX-License-Identifier: BSD-2-Clause

//! Stable numeric ordering, float normalization, and lossless key collection.

use std::cmp::Reverse;
use tessera_order::RadixKey;

macro_rules! float_encoder {
    ($name:ident, $float:ty, $uint:ty) => {
        /// Encodes numeric order with equal zeros and NaNs last in either direction.
        pub(crate) fn $name(x: $float, descending: bool) -> $uint {
            if x.is_nan() {
                return <$uint>::MAX;
            }
            let bits = if x == 0.0 { 0 } else { x.to_bits() };
            let sign = 1 << (<$uint>::BITS - 1);
            let ascending = if bits & sign != 0 { !bits } else { bits ^ sign };
            if descending { !ascending } else { ascending }
        }
    };
}

float_encoder!(encode_f32, f32, u32);
float_encoder!(encode_f64, f64, u64);

mod sealed {
    pub(crate) trait Sealed {}
}

/// Numeric buffer elements with adapter keys that never modify original bits.
pub(crate) trait NumericElem: sealed::Sealed + RadixKey + Send + Sync {
    type Ascending: RadixKey;
    type Descending: RadixKey;
    fn direct_order(values: &[Self]) -> bool;
    fn ascending(&self) -> Self::Ascending;
    fn descending(&self) -> Self::Descending;
}

macro_rules! integers {
    ($($ty:ty),*) => {$ (
        impl sealed::Sealed for $ty {}
        impl NumericElem for $ty {
            type Ascending = Self;
            type Descending = Reverse<Self>;
            fn direct_order(_: &[Self]) -> bool { true }
            fn ascending(&self) -> Self { *self }
            fn descending(&self) -> Reverse<Self> { Reverse(*self) }
        }
    )*};
}
integers!(i8, i16, i32, i64, u8, u16, u32, u64, i128, u128);

macro_rules! floats {
    ($ty:ty, $uint:ty, $encode:ident) => {
        impl DirectFloat for $ty {
            fn classify(self) -> (bool, Option<bool>) {
                (
                    self.is_nan(),
                    (self == 0.0).then(|| self.is_sign_negative()),
                )
            }
        }
        impl sealed::Sealed for $ty {}
        impl NumericElem for $ty {
            type Ascending = $uint;
            type Descending = $uint;
            fn direct_order(values: &[Self]) -> bool {
                is_direct_float_order(values)
            }
            fn ascending(&self) -> $uint {
                $encode(*self, false)
            }
            fn descending(&self) -> $uint {
                $encode(*self, true)
            }
        }
    };
}
floats!(f32, u32, encode_f32);
floats!(f64, u64, encode_f64);

// Reverse is a representation-only view; it changes keys, never stored bits.
fn reversed<T>(values: &[T]) -> &[Reverse<T>] {
    // SAFETY: Reverse<T> is #[repr(transparent)] over T, with identical validity,
    // size and alignment. The shared borrow and slice length are unchanged.
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), values.len()) }
}

fn reversed_mut<T>(values: &mut [T]) -> &mut [Reverse<T>] {
    // SAFETY: Reverse<T> is #[repr(transparent)] over T, with identical validity,
    // size and alignment. The exclusive borrow and slice length are unchanged.
    unsafe { std::slice::from_raw_parts_mut(values.as_mut_ptr().cast(), values.len()) }
}

trait DirectFloat: Copy {
    fn classify(self) -> (bool, Option<bool>);
}

// Stop as soon as total_cmp would disagree with the adapter's ordering.
fn is_direct_float_order<T: DirectFloat>(values: &[T]) -> bool {
    let (mut negative_zero, mut positive_zero) = (false, false);
    for &value in values {
        let (nan, zero_sign) = value.classify();
        if nan {
            return false;
        }
        match zero_sign {
            Some(true) => negative_zero = true,
            Some(false) => positive_zero = true,
            None => {}
        }
        if negative_zero && positive_zero {
            return false;
        }
    }
    true
}

/// Stably sorts numeric values, retaining their original bits.
pub(crate) fn sort_slice<T: NumericElem>(values: &mut [T], descending: bool, parallel: bool) {
    if T::direct_order(values) {
        if descending {
            let values = reversed_mut(values);
            if parallel {
                tessera_order::par_sort(values)
            } else {
                tessera_order::sort(values)
            }
        } else if parallel {
            tessera_order::par_sort(values)
        } else {
            tessera_order::sort(values)
        }
    } else if descending {
        if parallel {
            tessera_order::par_sort_by_key(values, T::descending)
        } else {
            tessera_order::sort_by_key(values, T::descending)
        }
    } else if parallel {
        tessera_order::par_sort_by_key(values, T::ascending);
    } else {
        tessera_order::sort_by_key(values, T::ascending);
    }
}

/// Returns stable source indices in numeric order.
pub(crate) fn argsort_slice<T: NumericElem>(
    values: &[T],
    descending: bool,
    parallel: bool,
) -> Vec<usize> {
    if T::direct_order(values) {
        if descending {
            let values = reversed(values);
            if parallel {
                tessera_order::par_argsort(values)
            } else {
                tessera_order::argsort(values)
            }
        } else if parallel {
            tessera_order::par_argsort(values)
        } else {
            tessera_order::argsort(values)
        }
    } else if descending {
        if parallel {
            tessera_order::par_argsort_by_key(values, T::descending)
        } else {
            tessera_order::argsort_by_key(values, T::descending)
        }
    } else if parallel {
        tessera_order::par_argsort_by_key(values, T::ascending)
    } else {
        tessera_order::argsort_by_key(values, T::ascending)
    }
}

// Ranks stay relative to the original slice; offsets avoid allocating local ranks.
fn partition_ranks<T: Send, F: Fn(&mut [T], usize) + Sync>(
    values: &mut [T],
    ranks: &[usize],
    offset: usize,
    parallel: bool,
    select: &F,
) {
    if ranks.is_empty() {
        return;
    }
    let middle = ranks.len() / 2;
    let rank = ranks[middle];
    select(values, rank - offset);
    let (left, rest) = values.split_at_mut(rank - offset);
    let right = &mut rest[1..];
    let mut before = || partition_ranks(left, &ranks[..middle], offset, parallel, select);
    let mut after = || partition_ranks(right, &ranks[middle + 1..], rank + 1, parallel, select);
    if parallel && ranks.len() > 1 {
        rayon::join(before, after);
    } else {
        before();
        after();
    }
}

fn partition_ranks_by_key<T: Send, K: RadixKey, F: Fn(&T) -> K + Sync>(
    values: &mut [T],
    ranks: &[usize],
    parallel: bool,
    key: F,
) {
    partition_ranks(values, ranks, 0, parallel, &|lane, rank| {
        tessera_order::select_nth_by_key(lane, rank, &key);
    });
}

/// Partitions at strictly increasing ranks in `0..values.len()`, preserving ties.
pub(crate) fn partition_slice<T: NumericElem>(
    values: &mut [T],
    ranks: &[usize],
    descending: bool,
    parallel: bool,
) {
    if T::direct_order(values) {
        if descending {
            partition_ranks(reversed_mut(values), ranks, 0, parallel, &|lane, rank| {
                tessera_order::select_nth(lane, rank);
            });
        } else {
            partition_ranks(values, ranks, 0, parallel, &|lane, rank| {
                tessera_order::select_nth(lane, rank);
            });
        }
    } else if descending {
        partition_ranks_by_key(values, ranks, parallel, T::descending);
    } else {
        partition_ranks_by_key(values, ranks, parallel, T::ascending);
    }
}

/// Partitions source indices at strictly increasing valid ranks without a full sort.
pub(crate) fn argpartition_slice<T: NumericElem>(
    values: &[T],
    ranks: &[usize],
    descending: bool,
    parallel: bool,
) -> Vec<usize> {
    let mut indices = (0..values.len()).collect::<Vec<_>>();
    if T::direct_order(values) {
        if descending {
            let values = reversed(values);
            partition_ranks_by_key(&mut indices, ranks, parallel, |&i| values[i]);
        } else {
            partition_ranks_by_key(&mut indices, ranks, parallel, |&i| values[i]);
        }
    } else if descending {
        partition_ranks_by_key(&mut indices, ranks, parallel, |&i| values[i].descending());
    } else {
        partition_ranks_by_key(&mut indices, ranks, parallel, |&i| values[i].ascending());
    }
    indices
}

/// Keys extracted once from Python numbers, widened without loss.
#[derive(Debug, Default, PartialEq)]
pub(crate) enum NumericKeys {
    #[default]
    Empty,
    I64(Vec<i64>),
    U64(Vec<u64>),
    I128(Vec<i128>),
    U128(Vec<u128>),
    F64(Vec<f64>),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NumericError {
    MixedNumeric,
    Overflow,
}

impl NumericKeys {
    /// Appends an integer, leaving the collector unchanged on error.
    pub(crate) fn push_i128(&mut self, value: i128) -> Result<(), NumericError> {
        match self {
            Self::Empty => {
                *self = if let Ok(x) = i64::try_from(value) {
                    Self::I64(vec![x])
                } else if let Ok(x) = u64::try_from(value) {
                    Self::U64(vec![x])
                } else {
                    Self::I128(vec![value])
                }
            }
            Self::I64(values) => {
                if let Ok(x) = i64::try_from(value) {
                    values.push(x);
                } else if value >= 0 && value <= u64::MAX as i128 && values.iter().all(|&x| x >= 0)
                {
                    let mut widened = values.iter().map(|&x| x as u64).collect::<Vec<_>>();
                    widened.push(value as u64);
                    *self = Self::U64(widened);
                } else {
                    let mut widened = values.iter().map(|&x| i128::from(x)).collect::<Vec<_>>();
                    widened.push(value);
                    *self = Self::I128(widened);
                }
            }
            Self::U64(values) => {
                if let Ok(x) = u64::try_from(value) {
                    values.push(x);
                } else {
                    let mut widened = values.iter().map(|&x| i128::from(x)).collect::<Vec<_>>();
                    widened.push(value);
                    *self = Self::I128(widened);
                }
            }
            Self::I128(values) => values.push(value),
            Self::U128(values) => {
                if let Ok(x) = u128::try_from(value) {
                    values.push(x);
                } else {
                    if values.iter().any(|&x| x > i128::MAX as u128) {
                        return Err(NumericError::Overflow);
                    }
                    let mut widened = values.iter().map(|&x| x as i128).collect::<Vec<_>>();
                    widened.push(value);
                    *self = Self::I128(widened);
                }
            }
            Self::F64(_) => return Err(NumericError::MixedNumeric),
        }
        Ok(())
    }

    /// Appends an unsigned integer, rejecting ranges no integer type can hold.
    pub(crate) fn push_u128(&mut self, value: u128) -> Result<(), NumericError> {
        if let Ok(x) = i128::try_from(value) {
            return self.push_i128(x);
        }
        let mut widened: Vec<u128> = match self {
            Self::Empty => Vec::new(),
            Self::I64(values) => values
                .iter()
                .map(|&x| u128::try_from(x))
                .collect::<Result<_, _>>()
                .map_err(|_| NumericError::Overflow)?,
            Self::U64(values) => values.iter().map(|&x| u128::from(x)).collect(),
            Self::I128(values) => values
                .iter()
                .map(|&x| u128::try_from(x))
                .collect::<Result<_, _>>()
                .map_err(|_| NumericError::Overflow)?,
            Self::U128(values) => {
                values.push(value);
                return Ok(());
            }
            Self::F64(_) => return Err(NumericError::MixedNumeric),
        };
        widened.push(value);
        *self = Self::U128(widened);
        Ok(())
    }

    /// Appends a float; mixing floats with integers is an explicit error.
    pub(crate) fn push_f64(&mut self, value: f64) -> Result<(), NumericError> {
        match self {
            Self::Empty => *self = Self::F64(vec![value]),
            Self::F64(values) => values.push(value),
            _ => return Err(NumericError::MixedNumeric),
        }
        Ok(())
    }

    /// Orders source indices by a full sort or strictly increasing valid ranks.
    pub(crate) fn order_indices(
        &self,
        ranks: Option<&[usize]>,
        descending: bool,
        parallel: bool,
    ) -> Vec<usize> {
        macro_rules! order {
            ($values:expr) => {
                match ranks {
                    Some(ranks) => argpartition_slice($values, ranks, descending, parallel),
                    None => argsort_slice($values, descending, parallel),
                }
            };
        }
        match self {
            Self::Empty => Vec::new(),
            Self::I64(values) => order!(values),
            Self::U64(values) => order!(values),
            Self::I128(values) => order!(values),
            Self::U128(values) => order!(values),
            Self::F64(values) => order!(values),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, fmt::Debug};

    fn check_numeric<T, K, B>(values: &[T], key: fn(T, bool) -> K, bits: fn(T) -> B)
    where
        T: NumericElem + Debug,
        K: Ord + Copy + Debug,
        B: Ord + Copy + Debug,
    {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(3)
            .build()
            .unwrap();
        pool.install(|| {
            for descending in [false, true] {
                let mut sorted: Vec<_> = (0..values.len()).collect();
                sorted.sort_by_key(|&i| key(values[i], descending));
                let expected: Vec<_> = sorted.iter().map(|&i| bits(values[i])).collect();
                let mut groups = BTreeMap::<K, Vec<B>>::new();
                for &value in values {
                    groups
                        .entry(key(value, descending))
                        .or_default()
                        .push(bits(value));
                }
                let mut rank_sets = vec![vec![]];
                if !values.is_empty() {
                    rank_sets.extend([vec![0], vec![values.len() / 2], vec![values.len() - 1]]);
                    let mut ranks = vec![
                        0,
                        values.len() / 4,
                        values.len() / 2,
                        values.len() * 3 / 4,
                        values.len() - 1,
                    ];
                    ranks.sort_unstable();
                    ranks.dedup();
                    rank_sets.push(ranks);
                    if values.len() < 100 {
                        rank_sets.push((0..values.len()).collect());
                    }
                }
                for parallel in [false, true] {
                    assert_eq!(argsort_slice(values, descending, parallel), sorted);
                    let mut ordered = values.to_vec();
                    sort_slice(&mut ordered, descending, parallel);
                    assert_eq!(ordered.into_iter().map(bits).collect::<Vec<_>>(), expected);
                    for ranks in &rank_sets {
                        let indices = argpartition_slice(values, ranks, descending, parallel);
                        let mut permutation = indices.clone();
                        permutation.sort_unstable();
                        assert_eq!(permutation, (0..values.len()).collect::<Vec<_>>());
                        let mut positions = vec![0; values.len()];
                        for (rank, &i) in sorted.iter().enumerate() {
                            positions[i] = rank;
                        }
                        let mut last = BTreeMap::new();
                        for &i in &indices {
                            if let Some(previous) = last.insert(key(values[i], descending), i) {
                                assert!(previous < i, "equal keys must retain source order");
                            }
                        }
                        for &rank in ranks {
                            assert_eq!(indices[rank], sorted[rank]);
                            assert!(indices[..rank].iter().all(|&i| positions[i] < rank));
                            assert!(indices[rank + 1..].iter().all(|&i| positions[i] > rank));
                        }
                        let mut partitioned = values.to_vec();
                        partition_slice(&mut partitioned, ranks, descending, parallel);
                        let mut actual_groups = BTreeMap::<K, Vec<B>>::new();
                        for &value in &partitioned {
                            actual_groups
                                .entry(key(value, descending))
                                .or_default()
                                .push(bits(value));
                        }
                        // Grouped original bits expose zero/NaN identity as well as loss or duplication.
                        assert_eq!(actual_groups, groups);
                        for &rank in ranks {
                            assert_eq!(bits(partitioned[rank]), expected[rank]);
                            let pivot = key(partitioned[rank], descending);
                            assert!(
                                partitioned[..rank]
                                    .iter()
                                    .all(|&x| key(x, descending) <= pivot)
                            );
                            assert!(
                                partitioned[rank + 1..]
                                    .iter()
                                    .all(|&x| key(x, descending) >= pivot)
                            );
                        }
                        if ranks.is_empty() {
                            assert_eq!(indices, (0..values.len()).collect::<Vec<_>>());
                            assert_eq!(
                                partitioned.into_iter().map(bits).collect::<Vec<_>>(),
                                values.iter().copied().map(bits).collect::<Vec<_>>()
                            );
                        }
                    }
                }
            }
        });
    }

    macro_rules! float_tests {
        ($name:ident, $float:ty, $uint:ty, $encode:ident, $quiet:expr) => {
            #[test]
            fn $name() {
                let infinity = <$float>::INFINITY.to_bits();
                let sign = 1 << (<$uint>::BITS - 1);
                let nan_bits = [
                    infinity | 1,
                    infinity | $quiet | 17,
                    sign | infinity | 3,
                    sign | infinity | $quiet | 29,
                ];
                let ordered = [
                    <$float>::NEG_INFINITY,
                    -123.5,
                    -1.0,
                    -<$float>::MIN_POSITIVE,
                    -0.0,
                    0.0,
                    <$float>::MIN_POSITIVE,
                    1.0,
                    123.5,
                    <$float>::INFINITY,
                ];
                for descending in [false, true] {
                    let keys = ordered.map(|x| $encode(x, descending));
                    for (i, pair) in keys.windows(2).enumerate() {
                        if i == 4 {
                            assert_eq!(pair[0], pair[1]);
                        } else if descending {
                            assert!(pair[0] > pair[1]);
                        } else {
                            assert!(pair[0] < pair[1]);
                        }
                    }
                    assert!(keys.iter().all(|&key| key < <$uint>::MAX));
                    for bits in nan_bits {
                        assert_eq!($encode(<$float>::from_bits(bits), descending), <$uint>::MAX);
                    }
                }
                let values = [
                    <$float>::from_bits(nan_bits[0]),
                    0.0,
                    -1.0,
                    <$float>::INFINITY,
                    <$float>::from_bits(nan_bits[1]),
                    -0.0,
                    -1.0,
                    <$float>::NEG_INFINITY,
                    <$float>::from_bits(nan_bits[2]),
                    0.0,
                    2.0,
                    <$float>::from_bits(nan_bits[3]),
                    -0.0,
                ];
                assert!(!is_direct_float_order(&values));
                assert!(!is_direct_float_order(&[<$float>::NAN]));
                assert!(!is_direct_float_order(&[-0.0 as $float, 0.0]));
                check_numeric(&values, $encode, <$float>::to_bits);
                for clean in [
                    vec![3.0 as $float, -2.0, 1.0],
                    vec![0.0, 2.0, 0.0, -1.0],
                    vec![-0.0, 2.0, -0.0, -1.0],
                    vec![0.0; 4],
                    vec![-0.0; 4],
                    vec![],
                ] {
                    assert!(is_direct_float_order(&clean));
                    check_numeric(&clean, $encode, <$float>::to_bits);
                }
                for fallback in [vec![<$float>::NAN, 1.0, -1.0], vec![0.0, -0.0, 1.0, -1.0]] {
                    assert!(!is_direct_float_order(&fallback));
                    check_numeric(&fallback, $encode, <$float>::to_bits);
                }
                // Cross the core's tile boundary, with ties on both sides of rank cuts.
                let large: Vec<_> = (0..20_000)
                    .map(|i| values[(i * 7) % values.len()])
                    .collect();
                check_numeric(&large, $encode, <$float>::to_bits);
            }
        };
    }
    float_tests!(
        f32_encoding_and_stable_operations,
        f32,
        u32,
        encode_f32,
        0x0040_0000
    );
    float_tests!(
        f64_encoding_and_stable_operations,
        f64,
        u64,
        encode_f64,
        0x0008_0000_0000_0000
    );

    #[test]
    fn integer_types_and_empty_inputs() {
        macro_rules! check {
            ($($ty:ty),*) => {$ (
                check_numeric(&[<$ty>::MAX, 0, 1, <$ty>::MIN, 0, <$ty>::MAX],
                    |x, descending| if descending { Reverse(x) } else { Reverse(!x) }, |x| x);
            )*};
        }
        check!(i8, i16, i32, i64, i128, u8, u16, u32, u64, u128);
        check_numeric::<i64, _, _>(&[], |x, d| if d { Reverse(x) } else { Reverse(!x) }, |x| x);
        check_numeric::<i64, _, _>(
            &[42],
            |x, d| if d { Reverse(x) } else { Reverse(!x) },
            |x| x,
        );
        let large: Vec<i64> = (0..20_000).map(|i| (i * 7919 % 101) - 50).collect();
        check_numeric(
            &large,
            |x, d| if d { Reverse(x) } else { Reverse(!x) },
            |x| x,
        );
    }

    #[test]
    fn lossless_widening_and_integer_ranges() {
        let mut keys = NumericKeys::Empty;
        keys.push_i128(7).unwrap();
        assert_eq!(keys, NumericKeys::I64(vec![7]));
        keys.push_u128(u64::MAX as u128).unwrap();
        assert_eq!(keys, NumericKeys::U64(vec![7, u64::MAX]));
        keys.push_i128(i128::MAX).unwrap();
        assert_eq!(
            keys,
            NumericKeys::I128(vec![7, u64::MAX as i128, i128::MAX])
        );
        keys.push_u128(u128::MAX).unwrap();
        assert_eq!(
            keys,
            NumericKeys::U128(vec![7, u64::MAX as u128, i128::MAX as u128, u128::MAX])
        );
        keys.push_i128(0).unwrap();
        keys.push_u128(1).unwrap();
        assert_eq!(keys.order_indices(None, false, false), [4, 5, 0, 1, 2, 3]);
        assert_eq!(keys.push_i128(-1), Err(NumericError::Overflow));
        let mut signed = NumericKeys::I64(vec![-1]);
        signed.push_u128(u64::MAX as u128).unwrap();
        signed.push_i128(i128::MIN).unwrap();
        assert_eq!(
            signed,
            NumericKeys::I128(vec![-1, u64::MAX as i128, i128::MIN])
        );
        assert_eq!(signed.push_u128(u128::MAX), Err(NumericError::Overflow));
        let mut unsigned = NumericKeys::U64(vec![u64::MAX]);
        unsigned.push_i128(-1).unwrap();
        assert_eq!(unsigned, NumericKeys::I128(vec![u64::MAX as i128, -1]));
        let mut narrow_unsigned = NumericKeys::U128(vec![17]);
        narrow_unsigned.push_i128(-1).unwrap();
        assert_eq!(narrow_unsigned, NumericKeys::I128(vec![17, -1]));
        assert!(
            NumericKeys::Empty
                .order_indices(Some(&[]), false, false)
                .is_empty()
        );
    }

    #[test]
    fn overflow_and_mixed_keys_leave_collector_unchanged() {
        for mut keys in [NumericKeys::I64(vec![-1]), NumericKeys::I128(vec![-1])] {
            assert_eq!(keys.push_u128(u128::MAX), Err(NumericError::Overflow));
            assert_eq!(keys.order_indices(None, false, false), [0]);
        }
        let mut keys = NumericKeys::U128(vec![u128::MAX]);
        assert_eq!(keys.push_i128(-1), Err(NumericError::Overflow));
        assert_eq!(keys, NumericKeys::U128(vec![u128::MAX]));
        for mut keys in [
            NumericKeys::I64(vec![1]),
            NumericKeys::U64(vec![1]),
            NumericKeys::I128(vec![1]),
            NumericKeys::U128(vec![1]),
        ] {
            assert_eq!(keys.push_f64(1.0), Err(NumericError::MixedNumeric));
            assert_eq!(keys.order_indices(None, false, false), [0]);
        }
        let mut keys = NumericKeys::Empty;
        keys.push_f64(1.0).unwrap();
        keys.push_f64(-2.0).unwrap();
        assert_eq!(keys.push_i128(1), Err(NumericError::MixedNumeric));
        assert_eq!(keys.push_u128(u128::MAX), Err(NumericError::MixedNumeric));
        assert_eq!(keys, NumericKeys::F64(vec![1.0, -2.0]));
    }

    #[test]
    fn collector_orders_every_storage_variant() {
        let variants = [
            NumericKeys::I64(vec![3, 1, 3, 2]),
            NumericKeys::U64(vec![3, 1, 3, 2]),
            NumericKeys::I128(vec![3, 1, 3, 2]),
            NumericKeys::U128(vec![3, 1, 3, 2]),
            NumericKeys::F64(vec![3.0, 1.0, 3.0, 2.0]),
        ];
        for keys in variants {
            for (descending, expected) in [(false, [1, 3, 0, 2]), (true, [0, 2, 3, 1])] {
                for parallel in [false, true] {
                    assert_eq!(keys.order_indices(None, descending, parallel), expected);
                    for ranks in [&[1][..], &[0, 2, 3][..]] {
                        let indices = keys.order_indices(Some(ranks), descending, parallel);
                        for &rank in ranks {
                            assert_eq!(indices[rank], expected[rank]);
                        }
                    }
                }
            }
        }
    }
}
