// SPDX-License-Identifier: BSD-2-Clause

//! Ordering tests: every key encoding, float total order, `Reverse`, length
//! thresholds, 128-bit key halves and unusual record layouts, checked against
//! the standard library's stable sort.

use std::cmp::Reverse;
use tessera_sort::{stable_sort, stable_sort_by_key};

fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut x = *state;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
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
            stable_sort(&mut values);
            assert_eq!(values, expected);
            let mut descending: Vec<_> = values.into_iter().map(Reverse).collect();
            stable_sort(&mut descending);
            expected.reverse();
            assert_eq!(descending.into_iter().map(|x| x.0).collect::<Vec<_>>(), expected);
        )*};
    }
    check!(
        u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
    );
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
            stable_sort(&mut values);
            assert_eq!(
                values.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                expected.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
            );
            let mut rows: Vec<_> = values.into_iter().enumerate().collect();
            let mut expected = rows.clone();
            expected.sort_by(|a, b| b.1.total_cmp(&a.1));
            stable_sort_by_key(&mut rows, |x| Reverse(x.1));
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
fn record_shapes_and_thresholds() {
    let mut state = 71;
    let lengths: &[usize] = if cfg!(miri) {
        &[0, 1, 2, 47, 48, 49, 81]
    } else {
        &[
            0, 1, 2, 8, 47, 48, 49, 128, 512, 4095, 4096, 4097, 16383, 16384, 16385, 65535, 65536,
        ]
    };
    for &len in lengths {
        for shape in 0..9 {
            let mut values: Vec<_> = (0..len)
                .map(|id| {
                    let random = next(&mut state);
                    let key = match shape {
                        0 => random as u128,
                        1 => (random as u128) << 64,
                        2 => ((random as u128) << 64) | next(&mut state) as u128,
                        3 => (random % 16) as u128,
                        4 => (id / 3) as u128,
                        5 => ((len - id) / 3) as u128,
                        6 => 9,
                        7 => {
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
            stable_sort_by_key(&mut values, |x| x.0);
            assert_eq!(values, expected, "len={len} shape={shape}");
        }
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
            stable_sort(&mut values);
            let sorted: Vec<u64> = values.iter().map(|x| x.to_bits()).collect();
            let wanted: Vec<u64> = expected.iter().map(|x| x.to_bits()).collect();
            assert_eq!(sorted, wanted, "f64 len={len} low={low} width={width}");

            // Only the high half varies, so a 128-bit key continues as one half.
            let mut values: Vec<u128> = (0..len).map(|id| u128::from(draw(id)) << 64).collect();
            let mut expected = values.clone();
            expected.sort();
            stable_sort(&mut values);
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
    stable_sort_by_key(&mut input, |x| x.key);
    assert_eq!(
        input.iter().map(|x| (x.key, x.id)).collect::<Vec<_>>(),
        expected
    );
    for x in &input {
        assert_eq!(x.payload, [x.id as u8; 257]);
    }
    let mut empty = [(); 100];
    stable_sort_by_key(&mut empty, |_| 0u8);
}
