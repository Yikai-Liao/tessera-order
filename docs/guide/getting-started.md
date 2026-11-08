# Getting started

Tessera sorts slices by a primitive key: an integer up to 128 bits, an `f32` or `f64`, or `Reverse` of one of these. It is stable, runs in linear time for a fixed key width, and moves records of any type.

> [!IMPORTANT]
> Tessera is in pre-release, and nothing is on crates.io yet. The examples use the implemented 0.1 API. See the [roadmap](../../ROADMAP.md).

## Install

```toml
[dependencies]
tessera-sort = "0.1"
```

The crate has no required dependencies; the optional `portable-simd` feature adds `fearless_simd` (see the [Cargo feature documentation](../../rust/tessera-sort/src/lib.rs)). The `std` feature is on by default. Without it, the crate uses `core` and `alloc` and needs a global allocator:

```toml
[dependencies]
tessera-sort = { version = "0.1", default-features = false }
```

Without `std`, a key-function panic during block partitioning aborts, even if the program unwinds on panic. Sorting, ordering and memory bounds are the same either way.

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

## Sorting many slices

The free functions allocate scratch memory during each call. To sort many slices, keep the scratch in a `Workspace`:

```rust
use tessera_sort::Workspace;

let mut workspace = Workspace::new();
for batch in [vec![3u32, 1, 2], vec![9, 7, 8]] {
    let mut batch = batch;
    workspace.stable_sort(&mut batch);
    assert!(batch.is_sorted());
}
```

See [Reusing scratch memory](workspace.md) for the details.

## When to use something else

- **Orders defined by a comparator**, strings, or other variable-length keys: use `slice::sort_by`. A comparator cannot be turned into radix digits.
- **Parallel sorting**: Tessera is single-threaded.
- **No allocator**: Tessera needs O(√n) scratch memory, so it needs `alloc`.

The exact guarantees are in the [ordering semantics](../../conformance/semantics.md).
