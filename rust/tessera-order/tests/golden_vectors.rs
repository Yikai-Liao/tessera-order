// SPDX-License-Identifier: BSD-2-Clause

//! Cross-language key and stable ordering vectors.

use std::cmp::Reverse;
use std::fmt::{Debug, LowerHex};
use std::num::NonZero;
use tessera_order::{
    RadixKey, argsort, argsort_by_key, select_nth, select_nth_by_key, sort, sort_by_cached_key,
    sort_by_key,
};

const VECTOR_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../conformance/vectors/ordering.json"
);

const FLOAT32: &[u128] = &[
    0x7fc0_0002,
    0x8000_0000,
    0xff80_0001,
    0,
    0xffc0_0001,
    0xff80_0000,
    0x7f80_0001,
    0xbf80_0000,
    0x7f80_0000,
    0x3f80_0000,
    0x8000_0001,
    1,
    0x8000_0000,
    0x7fc0_0002,
];
const FLOAT64: &[u128] = &[
    0x7ff8_0000_0000_0002,
    0x8000_0000_0000_0000,
    0xfff0_0000_0000_0001,
    0,
    0xfff8_0000_0000_0001,
    0xfff0_0000_0000_0000,
    0x7ff0_0000_0000_0001,
    0xbff0_0000_0000_0000,
    0x7ff0_0000_0000_0000,
    0x3ff0_0000_0000_0000,
    0x8000_0000_0000_0001,
    1,
    0x8000_0000_0000_0000,
    0x7ff8_0000_0000_0002,
];

fn integer_patterns(width: usize) -> [u128; 7] {
    let sign = 1 << (width - 1);
    let mask = u128::MAX >> (128 - width);
    [sign + 1, 0, sign - 1, mask, sign, 1, sign + 1]
}

fn byte_array<const N: usize>(raw: u128) -> [u8; N] {
    raw.to_be_bytes()[16 - N..].try_into().unwrap()
}

fn vector<K>(name: &str, width: usize, raw: &[u128], from_raw: impl Fn(u128) -> K) -> String
where
    K: RadixKey,
    K::Encoded: Debug + LowerHex,
{
    let keys: Vec<K> = raw.iter().copied().map(from_raw).collect();
    let encoded: Vec<_> = keys.iter().map(|key| key.encode()).collect();
    for &key in &encoded {
        assert_eq!(K::decode(key).encode(), key, "{name} decode");
    }
    // Input positions are record IDs. Compute the stable reference independently
    // of Tessera's movement and verify both direct and keyed operations against it.
    let mut expected: Vec<_> = (0..keys.len()).collect();
    expected.sort_by_key(|&id| encoded[id]);
    let sorted_encodings: Vec<_> = expected.iter().map(|&id| encoded[id]).collect();

    let mut direct = keys.clone();
    sort(&mut direct);
    assert_eq!(
        direct.iter().map(|key| key.encode()).collect::<Vec<_>>(),
        sorted_encodings,
        "{name} sort"
    );
    assert_eq!(argsort(&keys), expected, "{name} argsort");

    let rows: Vec<_> = keys.iter().copied().enumerate().collect();
    let ids = |rows: &[(usize, K)]| rows.iter().map(|row| row.0).collect::<Vec<_>>();
    let mut sorted = rows.clone();
    sort_by_key(&mut sorted, |row| row.1);
    assert_eq!(ids(&sorted), expected, "{name} sort_by_key");
    sorted.clone_from(&rows);
    sort_by_cached_key(&mut sorted, |row| row.1);
    assert_eq!(ids(&sorted), expected, "{name} sort_by_cached_key");
    let permutation = argsort_by_key(&rows, |row| row.1);
    assert_eq!(permutation, expected, "{name} argsort_by_key");

    let storage_bits = size_of::<K::Encoded>() * 8;
    let raw_digits = width / 4;
    let storage_digits = storage_bits / 4;
    let input: Vec<_> = raw.iter().map(|v| format!("{v:0raw_digits$x}")).collect();
    let bits: Vec<_> = encoded
        .iter()
        .map(|v| format!("{v:0storage_digits$x}"))
        .collect();
    let mask = K::VARYING_MASK;
    for (rank, &selected_id) in expected.iter().enumerate() {
        let mut direct = keys.clone();
        let (before, nth, after) = select_nth(&mut direct, rank);
        assert_eq!(nth.encode(), sorted_encodings[rank], "{name} rank {rank}");
        let mut before_bits: Vec<_> = before.iter().map(|key| key.encode()).collect();
        before_bits.sort_unstable();
        assert_eq!(before_bits, sorted_encodings[..rank], "{name} rank {rank}");
        let mut after_bits: Vec<_> = after.iter().map(|key| key.encode()).collect();
        after_bits.sort_unstable();
        assert_eq!(
            after_bits,
            sorted_encodings[rank + 1..],
            "{name} rank {rank}"
        );

        let mut selection = rows.clone();
        let (before, nth, after) = select_nth_by_key(&mut selection, rank, |row| row.1);
        assert_eq!(nth.0, selected_id, "{name} selected ID at rank {rank}");
        let (mut before_ids, mut after_ids) = (ids(before), ids(after));
        // Stable-sort the output by just the key to detect reordered ties anywhere.
        let mut selection_ids = ids(&selection);
        selection_ids.sort_by_key(|&id| encoded[id]);
        assert_eq!(selection_ids, expected, "{name} stable rank {rank}");
        let (mut before_expected, mut after_expected) =
            (expected[..rank].to_vec(), expected[rank + 1..].to_vec());
        before_expected.sort_unstable();
        after_expected.sort_unstable();
        before_ids.sort_unstable();
        after_ids.sort_unstable();
        assert_eq!(before_ids, before_expected, "{name} before rank {rank}");
        assert_eq!(after_ids, after_expected, "{name} after rank {rank}");
    }
    format!(
        r#"    {{
      "name": "{name}",
      "width_bits": {width},
      "storage_bits": {storage_bits},
      "varying_mask": "{mask:0storage_digits$x}",
      "input": {input:?},
      "encoded": {bits:?},
      "stable_order": {expected:?}
    }}"#
    )
}

fn raw_float32(raw: u128) -> f32 {
    f32::from_bits(raw as u32)
}

fn raw_float64(raw: u128) -> f64 {
    f64::from_bits(raw as u64)
}

fn vectors() -> String {
    let mut cases = Vec::new();
    macro_rules! integers {
        ($($kind:ident: $width:literal),+ $(,)?) => {$(
            cases.push(vector(stringify!($kind), $width, &integer_patterns($width), |x| x as $kind));
        )+};
    }
    integers!(u8: 8, u16: 16, u32: 32, u64: 64, u128: 128);
    integers!(i8: 8, i16: 16, i32: 32, i64: 64, i128: 128);
    // Fixed-width equivalents make the file identical on 32- and 64-bit hosts.
    let word_bits = usize::BITS as usize;
    let raw_words = integer_patterns(word_bits);
    let (unsigned, signed) = if word_bits == 32 {
        (
            vector("u32", 32, &raw_words, |x| x as u32),
            vector("i32", 32, &raw_words, |x| x as i32),
        )
    } else {
        (
            vector("u64", 64, &raw_words, |x| x as u64),
            vector("i64", 64, &raw_words, |x| x as i64),
        )
    };
    let word_name = if word_bits == 32 { "32" } else { "64" };
    assert_eq!(
        vector(&format!("u{word_name}"), word_bits, &raw_words, |x| x
            as usize),
        unsigned
    );
    assert_eq!(
        vector(&format!("i{word_name}"), word_bits, &raw_words, |x| x
            as isize),
        signed
    );
    cases.push(vector("nonzero_u16", 16, &[0xffff, 1, 0x8000, 2, 1], |x| {
        NonZero::new(x as u16).unwrap()
    }));
    cases.push(vector(
        "nonzero_i16",
        16,
        &[0xffff, 1, 0x8000, 0x7fff, 1],
        |x| NonZero::new(x as i16).unwrap(),
    ));
    cases.push(vector("bool", 8, &[1, 0, 1, 0], |x| x != 0));
    cases.push(vector(
        "char",
        32,
        &[0x10ffff, 0x10000, 0, 0xe000, 0xd7ff, 0],
        |x| char::from_u32(x as u32).unwrap(),
    ));
    cases.push(vector("f32", 32, FLOAT32, raw_float32));
    cases.push(vector("f64", 64, FLOAT64, raw_float64));
    macro_rules! arrays {
        ($($n:literal),+) => {$(
            cases.push(vector(
                concat!("bytes", stringify!($n)),
                $n * 8,
                &integer_patterns($n * 8),
                byte_array::<$n>,
            ));
        )+};
    }
    arrays!(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);
    cases.push(vector("reverse_f64", 64, FLOAT64, |x| {
        Reverse(raw_float64(x))
    }));
    cases.push(vector("reverse_bytes3", 24, &integer_patterns(24), |x| {
        Reverse(byte_array::<3>(x))
    }));
    cases.push(vector("reverse_bool", 8, &[1, 0, 1, 0], |x| {
        Reverse(x != 0)
    }));
    cases.push(vector("reverse_reverse_f32", 32, FLOAT32, |x| {
        Reverse(Reverse(raw_float32(x)))
    }));
    cases.push(vector(
        "pair_cross64",
        88,
        &[
            0x01_8000_0000_0102_0304_0506,
            0x00_7fc0_0001_0f0e_0d0c_0b0a,
            0x00_8000_0000_ffff_ffff_ffff,
            0x00_0000_0000_0000_0000_0000,
            0x00_ff80_0001_0102_0304_0506,
            0x01_8000_0000_0102_0304_0506,
            0x01_8000_0000_0102_0304_0505,
        ],
        |x| (x >> 80 != 0, (raw_float32(x >> 48), byte_array::<6>(x))),
    ));
    cases.push(vector("tuple16", 128, &integer_patterns(128), |x| {
        let [a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p] = x.to_be_bytes();
        (a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p)
    }));
    format!(
        "{{\n  \"schema\": 2,\n  \"cases\": [\n{}\n  ]\n}}\n",
        cases.join(",\n")
    )
}

#[test]
#[cfg_attr(miri, ignore = "reads conformance vectors from disk")]
fn golden_vectors() {
    let generated = vectors();
    // Run this same test with TESSERA_UPDATE_ORDERING_VECTORS=1 to regenerate.
    if std::env::var("TESSERA_UPDATE_ORDERING_VECTORS").as_deref() == Ok("1") {
        std::fs::write(VECTOR_PATH, &generated).unwrap();
    }
    let committed = std::fs::read_to_string(VECTOR_PATH).unwrap();
    if committed != generated {
        let line = committed
            .lines()
            .zip(generated.lines())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| committed.lines().count().min(generated.lines().count()));
        panic!("ordering.json differs from Rust at line {}", line + 1);
    }
}
