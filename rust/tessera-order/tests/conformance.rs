// SPDX-License-Identifier: BSD-2-Clause

//! Ordering tests: every key encoding, float total order, `Reverse`, length
//! thresholds, 128-bit key halves and unusual record layouts, checked against
//! the standard library's stable sort.

use std::cmp::Reverse;
use std::num::NonZero;
use tessera_order::{RadixKey, sort, sort_by_cached_key, sort_by_key};

fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut x = *state;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

#[test]
#[cfg(not(miri))]
fn insertion_budget_handles_large_input() {
    // The 64 KiB probe completes with 4,096 swaps. Its scaled budget is
    // 275,000, but the intermediate product exceeds a 32-bit usize.
    let mut records: Vec<u32> = (0..1_100_000)
        .map(|id| {
            if id < 8_192 {
                id ^ 1
            } else if id < 16_384 {
                id
            } else {
                5_000_000 - id
            }
        })
        .collect();
    let mut expected = records.clone();
    expected.sort();
    sort(&mut records);
    assert_eq!(records, expected);
}

#[test]
fn integer_encodings() {
    macro_rules! check {
        ($($ty:ty),*) => {$(
            let mut state = 17;
            let mut values: Vec<$ty> = (0..if cfg!(miri) { 80 } else { 5000 })
                .map(|_| next(&mut state) as $ty).collect();
            values.extend([<$ty>::MIN, 0, <$ty>::MAX]);
            let mut expected = values.clone();
            expected.sort();
            sort(&mut values);
            assert_eq!(values, expected);
            let mut descending: Vec<_> = values.into_iter().map(Reverse).collect();
            sort(&mut descending);
            expected.reverse();
            assert_eq!(descending.into_iter().map(|x| x.0).collect::<Vec<_>>(), expected);
        )*};
    }
    check!(
        u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
    );
}

fn check_sort<K: RadixKey + Ord + core::fmt::Debug>(unsorted: &[K]) {
    let (mut actual, mut expected) = (unsorted.to_vec(), unsorted.to_vec());
    expected.sort();
    sort(&mut actual);
    assert_eq!(actual, expected);
    let mut rev: Vec<_> = unsorted.iter().copied().map(Reverse).collect();
    let mut expected_rev = rev.clone();
    expected_rev.sort();
    sort(&mut rev);
    assert_eq!(rev, expected_rev);
}

#[test]
fn primitive_extension_keys() {
    let mut state = 211;
    let len = if cfg!(miri) { 80 } else { 2048 };

    // `bool`: direct sort, `Reverse`, and stable predicate partitioning.
    let bools: Vec<_> = (0..len).map(|_| next(&mut state) & 1 != 0).collect();
    check_sort(&bools);
    let mut pred: Vec<_> = (0..len).map(|id| (next(&mut state), id)).collect();
    let mut exp = pred.clone();
    exp.sort_by_key(|r| r.0 % 3 == 0);
    sort_by_cached_key(&mut pred, |r| r.0 % 3 == 0);
    assert_eq!(pred, exp);

    // `char`: ASCII, BMP, surrogate-adjacent scalars, supplementary plane, `char::MAX`.
    let special = ['\0', 'A', '\u{d7ff}', '\u{e000}', '🦀', char::MAX];
    let mut chars: Vec<char> = (0..len)
        .map(|_| {
            loop {
                let raw = (next(&mut state) as u32) % (char::MAX as u32 + 1);
                if let Some(ch) = char::from_u32(raw) {
                    break ch;
                }
            }
        })
        .collect();
    chars.extend(special);
    check_sort(&chars);

    // All 12 `NonZero<T>` primitive integer types.
    macro_rules! check_nz {
        ($($ty:ty: [$($extra:expr),*]),* $(,)?) => {$(
            let mut v: Vec<NonZero<$ty>> = (0..len)
                .map(|_| {
                    let r = next(&mut state) as $ty;
                    NonZero::new(if r == 0 { 1 } else { r }).unwrap()
                })
                .collect();
            v.extend([NonZero::<$ty>::MIN, NonZero::<$ty>::MAX $(, NonZero::new($extra as $ty).unwrap())*]);
            check_sort(&v);
        )*};
    }
    check_nz!(
        u8: [], u16: [], u32: [], u64: [], u128: [], usize: [],
        i8: [-1, 1], i16: [-1, 1], i32: [-1, 1], i64: [-1, 1], i128: [-1, 1], isize: [-1, 1],
    );

    // Representative `[u8; N]` widths (1, padded 3, word 8, full 16 bytes).
    macro_rules! check_bytes {
        ($($n:literal),*) => {$(
            let mut v: Vec<[u8; $n]> = (0..len)
                .map(|_| {
                    let b = ((u128::from(next(&mut state)) << 64) | u128::from(next(&mut state)))
                        .to_le_bytes();
                    core::array::from_fn(|i| b[i])
                })
                .collect();
            v.extend([[0u8; $n], [0xffu8; $n]]);
            check_sort(&v);
        )*};
    }
    check_bytes!(1, 3, 8, 16);

    // Mixed tuple, keyed stability, and 128-bit `([u8; 12], char)` boundary.
    let tuples: Vec<_> = (0..len)
        .map(|_| {
            let r = (next(&mut state) % 9) as i16 - 4;
            let b = next(&mut state).to_le_bytes();
            (
                next(&mut state) & 1 != 0,
                NonZero::new(if r == 0 { -1 } else { r }).unwrap(),
                [b[0] % 4, b[1] % 5, b[2] % 7],
                Reverse(special[next(&mut state) as usize % special.len()]),
            )
        })
        .collect();
    check_sort(&tuples);
    let mut rows: Vec<_> = tuples.into_iter().enumerate().collect();
    let mut exp = rows.clone();
    exp.sort_by_key(|r| r.1);
    sort_by_key(&mut rows, |r| r.1);
    assert_eq!(rows, exp);
    let z = [0u8; 12];
    check_sort(&[
        (z, char::MAX),
        (z, '\0'),
        ([0xff; 12], 'a'),
        (z, '\u{e000}'),
    ]);
}

#[test]
fn float_total_order() {
    macro_rules! check {
        ($f:ty, $u:ty, $special:expr) => {{
            let mut state = 91;
            let mut values: Vec<$f> = (0..if cfg!(miri) { 80 } else { 9000 })
                .map(|_| <$f>::from_bits(next(&mut state) as $u))
                .collect();
            values.extend($special.into_iter().map(<$f>::from_bits));
            let mut expected = values.clone();
            expected.sort_by(<$f>::total_cmp);
            sort(&mut values);
            assert_eq!(
                values.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                expected.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
            );
            let mut rows: Vec<_> = values.into_iter().enumerate().collect();
            let mut expected = rows.clone();
            expected.sort_by(|a, b| b.1.total_cmp(&a.1));
            sort_by_key(&mut rows, |x| Reverse(x.1));
            assert_eq!(
                rows.iter()
                    .map(|x| (x.0, x.1.to_bits()))
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|x| (x.0, x.1.to_bits()))
                    .collect::<Vec<_>>()
            );
        }};
    }
    check!(
        f32,
        u32,
        [
            0, 1, 0x80000000, 0x80000001, 0x7f800000, 0xff800000, 0x7fc00001, 0x7f800001,
            0xffc00001
        ]
    );
    check!(
        f64,
        u64,
        [
            0,
            1,
            0x8000000000000000,
            0x8000000000000001,
            0x7ff0000000000000,
            0xfff0000000000000,
            0x7ff8000000000001,
            0xfff8000000000001
        ]
    );
}

#[test]
fn descending_ties_stay_stable() {
    let lengths: &[usize] = if cfg!(miri) {
        &[4, 49, 129]
    } else {
        &[4, 49, 129, 8192]
    };
    for &len in lengths {
        for start in [0, len / 2, len - 2] {
            for run_len in [2, 3] {
                if start + run_len > len {
                    continue;
                }
                let mut records: Vec<_> = (0..len).map(|id| ((len - id) as u64, id)).collect();
                let key = records[start].0;
                for record in &mut records[start + 1..start + run_len] {
                    record.0 = key;
                }
                let mut expected = records.clone();
                expected.sort_by_key(|record| record.0);
                sort_by_key(&mut records, |record| record.0);
                assert_eq!(
                    records, expected,
                    "len={len} start={start} run_len={run_len}"
                );
            }
        }
    }
}

#[test]
fn record_shapes_and_thresholds() {
    let mut state = 71;
    // Exercise length boundaries with full-width keys, then key shapes on
    // one multi-tile input instead of multiplying independent dimensions.
    let lengths: &[usize] = if cfg!(miri) {
        &[0, 1, 2, 47, 48, 49, 81]
    } else {
        &[0, 1, 2, 47, 48, 49, 2047, 2048, 2049, 8193, 65536]
    };
    let shape_len = if cfg!(miri) { 81 } else { 65536 };
    for (len, shape) in lengths.iter().map(|&len| (len, 2)).chain(
        (0..6)
            .filter(|&shape| shape != 2)
            .map(|shape| (shape_len, shape)),
    ) {
        let mut values: Vec<_> = (0..len)
            .map(|id| {
                let random = next(&mut state);
                let key = match shape {
                    0 => random as u128,
                    1 => (random as u128) << 64,
                    2 => ((random as u128) << 64) | next(&mut state) as u128,
                    3 => (random % 16) as u128,
                    4 => {
                        if random % 100 < 98 {
                            0
                        } else {
                            random as u128
                        }
                    }
                    _ => ((random % 8) as u128) * 0x01010101010101010101010101010101,
                };
                (key, id)
            })
            .collect();
        let mut expected = values.clone();
        expected.sort_by_key(|x| x.0);
        sort_by_key(&mut values, |x| x.0);
        assert_eq!(values, expected, "len={len} shape={shape}");
    }
}

#[test]
fn narrow_direct_ranges() {
    // Above one tile, a node whose keys differ only inside one eight-bit window
    // is finished by counting rather than by the block partition. Float keys
    // check what the fill has to preserve: equal encoded keys are equal bit
    // patterns, signed zeros and NaN payloads included.
    let lengths: &[usize] = if cfg!(miri) { &[257] } else { &[8193, 65_537] };
    let mut state = 4242;
    for &len in lengths {
        // Windows at the bottom, in the middle, in the exponent and at the top
        // of the key.
        for (low, width) in [(0usize, 8usize), (0, 3), (9, 8), (52, 8)] {
            let value_mask = (1u64 << width) - 1;
            let base = next(&mut state) & !(value_mask << low);
            let draw = |id: usize| {
                let random = (id as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
                base | ((random & value_mask) << low)
            };
            let mut values: Vec<f64> = (0..len).map(|id| f64::from_bits(draw(id))).collect();
            let mut expected = values.clone();
            expected.sort_by(f64::total_cmp);
            sort(&mut values);
            let sorted: Vec<u64> = values.iter().map(|x| x.to_bits()).collect();
            let wanted: Vec<u64> = expected.iter().map(|x| x.to_bits()).collect();
            assert_eq!(sorted, wanted, "f64 len={len} low={low} width={width}");

            // Only the high half varies, so a 128-bit key continues as one half.
            let mut values: Vec<u128> = (0..len).map(|id| u128::from(draw(id)) << 64).collect();
            let mut expected = values.clone();
            expected.sort();
            sort(&mut values);
            assert_eq!(values, expected, "u128 len={len} low={low} width={width}");
        }
    }
}
#[test]
fn unusual_record_layouts() {
    #[repr(align(128))]
    struct Wide {
        key: u64,
        id: usize,
        payload: [u8; 257],
    }
    let len = if cfg!(miri) { 130 } else { 4097 };
    let mut state = 9;
    let mut input: Vec<_> = (0..len)
        .map(|id| Wide {
            key: next(&mut state) % 16,
            id,
            payload: [id as u8; 257],
        })
        .collect();
    let mut expected: Vec<_> = input.iter().map(|x| (x.key, x.id)).collect();
    expected.sort_by_key(|x| x.0);
    sort_by_key(&mut input, |x| x.key);
    assert_eq!(
        input.iter().map(|x| (x.key, x.id)).collect::<Vec<_>>(),
        expected
    );
    for x in &input {
        assert_eq!(x.payload, [x.id as u8; 257]);
    }
    let mut empty = [(); 100];
    sort_by_key(&mut empty, |_| 0u8);
}

#[test]
fn tuple_keys_preserve_lexicographic_order_stability_and_bits() {
    let len = if cfg!(miri) { 64 } else { 2048 };
    let mut state = 1337;

    // Mixed signed, unsigned, reversed and float fields across non-power-of-2
    // total widths (i16 + u8 = 24 bits inside u32, plus nested Reverse<(u16, u8)>).
    let mut rows: Vec<_> = (0..len)
        .map(|id| {
            let a = (next(&mut state) % 5) as i16 - 2;
            let b = (next(&mut state) % 7) as u8;
            let c = (next(&mut state) % 9) as u16;
            let d = (next(&mut state) % 4) as u8;
            (a, b, c, d, id)
        })
        .collect();
    let mut expected = rows.clone();
    expected.sort_by_key(|r| (r.0, Reverse((r.2, r.3)), r.1));
    sort_by_key(&mut rows, |r| (r.0, Reverse((r.2, r.3)), r.1));
    assert_eq!(rows, expected);

    // Direct tuple sort including floats with signed zeros and distinct NaN payloads,
    // verifying both ordering and bit-exact counted/terminal decoding.
    let special_f32: [u32; 7] = [
        0x0000_0000,
        0x8000_0000,
        0x3f80_0000,
        0xbf80_0000,
        0x7fc0_0001,
        0x7fc0_0002,
        0xffc0_0003,
    ];
    let mut float_tuples: Vec<(u8, f32, Reverse<i8>)> = (0..len)
        .map(|i| {
            let tag = (next(&mut state) % 3) as u8;
            let f = if i < special_f32.len() * 4 {
                f32::from_bits(special_f32[i % special_f32.len()])
            } else {
                f32::from_bits(next(&mut state) as u32)
            };
            let tail = (next(&mut state) % 5) as i8 - 2;
            (tag, f, Reverse(tail))
        })
        .collect();
    let mut expected_floats = float_tuples.clone();
    expected_floats.sort_by(|x, y| {
        x.0.cmp(&y.0)
            .then_with(|| x.1.total_cmp(&y.1))
            .then_with(|| y.2.0.cmp(&x.2.0))
    });
    sort(&mut float_tuples);
    assert_eq!(
        float_tuples
            .iter()
            .map(|&(a, b, c)| (a, b.to_bits(), c))
            .collect::<Vec<_>>(),
        expected_floats
            .iter()
            .map(|&(a, b, c)| (a, b.to_bits(), c))
            .collect::<Vec<_>>()
    );

    // Exact-width nested tuple: 8 + 24 = 32 bits in u32. Shifting by storage width
    // (32 instead of 24) would discard the leading u8 and misorder these pairs.
    let mut exact_nested = [(1u8, (0u16, 0u8)), (0u8, (0u16, 1u8))];
    sort(&mut exact_nested);
    assert_eq!(exact_nested, [(0u8, (0u16, 1u8)), (1u8, (0u16, 0u8))]);

    // Only bit 0 varies, so the tile counts these keys and decodes them. Decoding the
    // inner field with its 32-bit storage width would turn every leading 1 into 0.
    let mut counted: Vec<_> = (0..128u8).map(|i| (1u8, (0u16, i & 1))).collect();
    let mut expected = counted.clone();
    expected.sort();
    sort(&mut counted);
    assert_eq!(counted, expected);

    let mut wide = [(u64::MAX, 0u64), (0, u64::MAX)];
    sort(&mut wide);
    assert_eq!(wide, [(0, u64::MAX), (u64::MAX, 0)]);

    type Sixteen = (
        u8,
        u8,
        u8,
        u8,
        u8,
        u8,
        u8,
        u8,
        u8,
        u8,
        u8,
        u8,
        u8,
        u8,
        u8,
        u8,
    );
    let mut sixteen: Vec<Sixteen> = (0..len.min(128))
        .map(|_| {
            let a = next(&mut state).to_le_bytes();
            let b = next(&mut state).to_le_bytes();
            (
                a[0] % 3,
                a[1] % 3,
                a[2],
                a[3],
                a[4],
                a[5],
                a[6],
                a[7],
                b[0],
                b[1],
                b[2],
                b[3],
                b[4],
                b[5],
                b[6],
                b[7],
            )
        })
        .collect();
    let to_arr = |t: &Sixteen| {
        [
            t.0, t.1, t.2, t.3, t.4, t.5, t.6, t.7, t.8, t.9, t.10, t.11, t.12, t.13, t.14, t.15,
        ]
    };
    let mut expected_16: Vec<[u8; 16]> = sixteen.iter().map(to_arr).collect();
    expected_16.sort();
    sort(&mut sixteen);
    assert_eq!(sixteen.iter().map(to_arr).collect::<Vec<_>>(), expected_16);
}

#[test]
#[allow(deprecated)]
fn deprecated_stable_names_forward_to_the_new_names() {
    let input: Vec<(u8, u32)> = (0..200).map(|i| ((i * 7 % 13) as u8, i)).collect();
    let mut expected = input.clone();
    sort_by_key(&mut expected, |x| x.0);
    let mut actual = input.clone();
    tessera_order::stable_sort_by_key(&mut actual, |x| x.0);
    assert_eq!(actual, expected);
    let mut keys: Vec<u32> = input.iter().map(|x| x.1 * 31 % 17).collect();
    let mut expected = keys.clone();
    sort(&mut expected);
    tessera_order::stable_sort(&mut keys);
    assert_eq!(keys, expected);
    #[cfg(feature = "rayon")]
    {
        let mut actual = input.clone();
        tessera_order::par_stable_sort_by_key(&mut actual, |x| x.0);
        let mut expected = input.clone();
        sort_by_key(&mut expected, |x| x.0);
        assert_eq!(actual, expected);
        let mut keys: Vec<u32> = input.iter().map(|x| x.1 * 31 % 17).collect();
        let mut expected = keys.clone();
        sort(&mut expected);
        tessera_order::par_stable_sort(&mut keys);
        assert_eq!(keys, expected);
    }
}
