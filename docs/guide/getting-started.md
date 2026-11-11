# Getting started

Tessera sorts slices by a primitive key: an integer up to 128 bits, an `f32` or `f64`, or `Reverse` of one of these. It is stable, runs in linear time for a fixed key width, and moves records of any type.

> [!IMPORTANT]
> Tessera is in pre-release, and nothing is on crates.io yet. The examples use the implemented 0.1 API. See the [roadmap](../../ROADMAP.md).

## Install

```toml
[dependencies]
tessera-sort = "0.1"
```

The crate requires `std`, Rust 1.89 or newer and `fearless_simd`. Vector scans and prefix sums automatically select supported instructions; keys without a vector kernel use scalar scans. There is no `portable-simd` or `std` feature, and disabling default features does not change these requirements. Rayon remains optional; see the [Cargo feature documentation](../../rust/tessera-sort/src/lib.rs).

## Sort keys

`stable_sort` sorts a slice of keys:

```rust
let mut values: Vec<i64> = vec![42, -7, 19, -7, 3];
tessera_sort::stable_sort(&mut values);
assert_eq!(values, [-7, -7, 3, 19, 42]);
```

## Sort records by a key

`stable_sort_by_key` sorts any records by a key that a function computes. Records with equal keys keep their input order:

```rust
struct Row {
    timestamp: i64,
    name: String,
}

let mut rows = vec![
    Row { timestamp: 30, name: "c".into() },
    Row { timestamp: 10, name: "a".into() },
    Row { timestamp: 30, name: "d".into() },
];
tessera_sort::stable_sort_by_key(&mut rows, |row| row.timestamp);
let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
assert_eq!(names, ["a", "c", "d"]);
```

The key function may be called several times per record, in any order. It should return the same key for a record throughout one call. If it does not, the order is unspecified, but every record is still in the slice exactly once.

## Descending order

Wrap the key in `Reverse`. Equal keys still keep their input order:

```rust
use std::cmp::Reverse;

let mut rows = vec![(2u32, "first"), (1, "middle"), (2, "last")];
tessera_sort::stable_sort_by_key(&mut rows, |row| Reverse(row.0));
assert_eq!(rows, [(2, "first"), (2, "last"), (1, "middle")]);
```

Do not sort ascending and then reverse the slice. That would also reverse the order of equal keys.

## Floats

Floats sort in `total_cmp` order: negative NaNs, `-inf`, negative numbers, `-0.0`, `+0.0`, positive numbers, `+inf`, positive NaNs. To treat every NaN as equal, or both zeros as equal, normalize the key:

```rust
let mut values = [0.5f64, -0.0, f64::NAN, 0.0, -1.0];
tessera_sort::stable_sort_by_key(&mut values, |&x| {
    if x.is_nan() { f64::NAN } else if x == 0.0 { 0.0 } else { x }
});
// -0.0 and 0.0 are now equal keys, so they keep their input order.
assert_eq!(values[1].to_bits(), (-0.0f64).to_bits());
assert_eq!(values[2].to_bits(), 0.0f64.to_bits());
assert!(values[4].is_nan());
```

[Keys as unsigned integers](../explain/keys.md) shows how each key type is encoded.

## Parallel sorting

Enable the optional `rayon` feature for `par_stable_sort` and `par_stable_sort_by_key`. Add a direct Rayon dependency if the application configures a custom pool:

```toml
[dependencies]
tessera-sort = { version = "0.1", features = ["rayon"] }
rayon = "1.10"
```

The entry points use Rayon's current pool. A caller can select its own pool with `install`:

```rust
let pool = rayon::ThreadPoolBuilder::new().num_threads(8).build().unwrap();
let mut records = [(2u32, "first"), (1, "middle"), (2, "last")];
pool.install(|| tessera_sort::par_stable_sort_by_key(&mut records, |record| record.0));
assert_eq!(records, [(1, "middle"), (2, "first"), (2, "last")]);
```

Records need `Send`; the by-key callback needs `Fn + Sync` because calls may overlap. Stability and record preservation remain unchanged. Small inputs and one-thread pools use the serial engine, and ordered/constant fast paths do not promise parallel acceleration. The [parallel contract](../../conformance/semantics.md#rust-rayon-entry-points) covers panic recovery and the O(P√n) auxiliary-space bound.

## When to use something else

- **Orders defined by a comparator**, strings, or other variable-length keys: use `slice::sort_by`. A comparator cannot be turned into radix digits.
- **No allocator**: Tessera needs O(√n) scratch memory, so it needs `alloc`.

The exact guarantees are in the [ordering semantics](../../conformance/semantics.md).
