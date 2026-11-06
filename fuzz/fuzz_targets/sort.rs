// SPDX-License-Identifier: BSD-2-Clause

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::cmp::Reverse;
use tessera_sort::{Workspace, stable_sort, stable_sort_by_key};

fuzz_target!(|data: &[u8]| check(data));

fn check(data: &[u8]) {
    if data.len() < 5 {
        return;
    }
    let len = if data[0..2] == [255, 255] {
        1_000_000
    } else {
        usize::from(u16::from_le_bytes([data[0], data[1]])) % 8193
    };
    let payload = &data[4..];
    let input: Vec<(i8, usize)> = (0..len)
        .map(|i| (payload[i % payload.len()] as i8, i))
        .collect();
    let mut expected = input.clone();
    expected.sort_by_key(|record| record.0);

    let mut actual = input.clone();
    stable_sort_by_key(&mut actual, |record| record.0);
    // Stable sorting must preserve every record and the order of equal keys.
    // mutant: ext/tessera-sort-sort/src/fuzz/fuzz_targets/sort.rs:28 `stable_sort_by_key(&mut actual, |record| record.0);` -> `let _ = &mut actual;`
    // input: unsorted
    if actual != expected {
        std::process::abort();
    }

    let mut workspace = Workspace::new();
    actual = input.clone();
    workspace.stable_sort_by_key(&mut actual, |record| record.0);
    // A reused workspace must sort with the same stable ordering.
    // mutant: ext/tessera-sort-sort/src/fuzz/fuzz_targets/sort.rs:38 `workspace.stable_sort_by_key(&mut actual, |record| record.0);` -> `let _ = &mut actual;`
    // input: unsorted
    if actual != expected {
        std::process::abort();
    }
    expected.sort_by_key(|record| Reverse(record.0));
    actual = input.clone();
    workspace.stable_sort_by_key(&mut actual, |record| Reverse(record.0));
    // Descending order must retain the original order of equal keys.
    // mutant: ext/tessera-sort-sort/src/fuzz/fuzz_targets/sort.rs:47 `workspace.stable_sort_by_key(&mut actual, |record| Reverse(record.0));` -> `let _ = &mut actual;`
    // input: unsorted
    if actual != expected {
        std::process::abort();
    }

    let mut expected_keys: Vec<i8> = input.iter().map(|record| record.0).collect();
    expected_keys.sort();
    let mut keys: Vec<i8> = input.iter().map(|record| record.0).collect();
    stable_sort(&mut keys);
    // Sorting keys directly must produce the same values as a reference sort.
    // mutant: ext/tessera-sort-sort/src/fuzz/fuzz_targets/sort.rs:58 `stable_sort(&mut keys);` -> `let _ = &mut keys;`
    // input: unsorted
    if keys != expected_keys {
        std::process::abort();
    }
    keys = input.iter().map(|record| record.0).collect();
    let mut key_workspace = Workspace::new();
    key_workspace.stable_sort(&mut keys);
    // The workspace and free function must have the same direct-sort result.
    // mutant: ext/tessera-sort-sort/src/fuzz/fuzz_targets/sort.rs:67 `key_workspace.stable_sort(&mut keys);` -> `key_workspace.stable_sort(&mut [] as &mut [i8]);`
    // input: unsorted
    if keys != expected_keys {
        std::process::abort();
    }

    let mut calls = 0u8;
    actual = input.clone();
    stable_sort_by_key(&mut actual, |record| {
        calls = calls.wrapping_add(1);
        record.0.wrapping_add(calls as i8)
    });
    actual.sort_by_key(|record| record.1);
    // Even an inconsistent key function must leave every record intact.
    // mutant: ext/tessera-sort-sort/src/fuzz/fuzz_targets/sort.rs:81 `actual.sort_by_key(|record| record.1);` -> `actual.pop(); actual.sort_by_key(|record| record.1);`
    // input: unsorted
    if actual != input {
        std::process::abort();
    }

    let wide_input: Vec<(i128, usize)> = (0..len)
        .map(|i| {
            let bytes = std::array::from_fn(|j| payload[(i + j) % payload.len()]);
            (i128::from_le_bytes(bytes), i)
        })
        .collect();
    let mut wide_expected = wide_input.clone();
    wide_expected.sort_by_key(|record| record.0);
    let mut wide_actual = wide_input;
    stable_sort_by_key(&mut wide_actual, |record| record.0);
    // Wide signed keys must have stable numerical ordering.
    // mutant: ext/tessera-sort-sort/src/fuzz/fuzz_targets/sort.rs:98 `stable_sort_by_key(&mut wide_actual, |record| record.0);` -> `let _ = &mut wide_actual;`
    // input: unsorted
    if wide_actual != wide_expected {
        std::process::abort();
    }

    let float_input: Vec<f64> = (0..len)
        .map(|i| {
            let bytes = std::array::from_fn(|j| payload[(i + j) % payload.len()]);
            f64::from_bits(u64::from_le_bytes(bytes))
        })
        .collect();
    let mut float_expected = float_input.clone();
    float_expected.sort_by(f64::total_cmp);
    let mut float_actual = float_input;
    stable_sort(&mut float_actual);
    // Float ordering follows total_cmp and preserves each value's bits.
    // mutant: ext/tessera-sort-sort/src/fuzz/fuzz_targets/sort.rs:115 `stable_sort(&mut float_actual);` -> `let _ = &mut float_actual;`
    // input: unsorted
    if !float_actual
        .iter()
        .map(|value| value.to_bits())
        .eq(float_expected.iter().map(|value| value.to_bits()))
    {
        std::process::abort();
    }
}
