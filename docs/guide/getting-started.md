# Getting started

Tessera sorts slices by a primitive key: an integer up to 128 bits, an `f32` or `f64`, or `Reverse` of one of these. It is stable, runs in linear time for a fixed key width, and moves records of any type.

> [!NOTE]
> This guide describes the unreleased 0.2 API. The published 0.1 package names the full sorts `stable_sort`, `stable_sort_by_key`, `par_stable_sort` and `par_stable_sort_by_key`; 0.2 keeps those names as deprecated aliases. [Partial sorting, selection](#partial-sorting-and-selected-ranks), [argsort, chunk sorting](#index-output-and-chunks) and [partitioning](#partitions-and-groups) are new in 0.2 and require a development checkout until it is released. 0.x minor releases may still change the API; see the [roadmap](../../ROADMAP.md).

## Install

```toml
[dependencies]
tessera-order = "0.1"
```

The crate requires `std`, Rust 1.89 or newer and `fearless_simd`. Vector scans and prefix sums automatically select supported instructions; keys without a vector kernel use scalar scans. There is no `portable-simd` or `std` feature, and disabling default features does not change these requirements. Rayon remains optional; see the [Cargo feature documentation](../../rust/tessera-order/src/lib.rs).

## Sort keys

`sort` sorts a slice of keys:

```rust
let mut values: Vec<i64> = vec![42, -7, 19, -7, 3];
tessera_order::sort(&mut values);
assert_eq!(values, [-7, -7, 3, 19, 42]);
```

## Sort records by a key

`sort_by_key` sorts any records by a key that a function computes. Records with equal keys keep their input order:

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
tessera_order::sort_by_key(&mut rows, |row| row.timestamp);
let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
assert_eq!(names, ["a", "c", "d"]);
```

The key function may be called several times per record, in any order. It should return the same key for a record throughout one call. If it does not, the operation may panic or return with unspecified ordering, but every record is still in the slice exactly once.

## Descending order

Wrap the key in `Reverse`. Equal keys still keep their input order:

```rust
use std::cmp::Reverse;

let mut rows = vec![(2u32, "first"), (1, "middle"), (2, "last")];
tessera_order::sort_by_key(&mut rows, |row| Reverse(row.0));
assert_eq!(rows, [(2, "first"), (2, "last"), (1, "middle")]);
```

Do not sort ascending and then reverse the slice. That would also reverse the order of equal keys.

## Floats

Floats sort in `total_cmp` order: negative NaNs, `-inf`, negative numbers, `-0.0`, `+0.0`, positive numbers, `+inf`, positive NaNs. To treat every NaN as equal, or both zeros as equal, normalize the key:

```rust
let mut values = [0.5f64, -0.0, f64::NAN, 0.0, -1.0];
tessera_order::sort_by_key(&mut values, |&x| {
    if x.is_nan() { f64::NAN } else if x == 0.0 { 0.0 } else { x }
});
// -0.0 and 0.0 are now equal keys, so they keep their input order.
assert_eq!(values[1].to_bits(), (-0.0f64).to_bits());
assert_eq!(values[2].to_bits(), 0.0f64.to_bits());
assert!(values[4].is_nan());
```

[Keys as unsigned integers](../explain/keys.md) shows how each key type is encoded.

## Partial sorting and selected ranks

**Unreleased:** these operations and the argsort, chunk and partition operations below are not in the published 0.1 package. Use a local checkout containing the additions instead of the crates.io dependency:

```toml
[dependencies]
tessera-order = { path = "../tessera-order/rust/tessera-order" }
```

Adjust the path to your checkout. These are ordinary serial functions, without an experimental feature flag. Each is the standard operation of the same name, with equal keys resolved as the stable sort resolves them.

`k_smallest_by_key` and `k_largest_by_key` move the selected records to the front in sorted order. Equal keys keep their input order, also at the boundary:

```rust
let mut rows = [(2u32, "first"), (1, "middle"), (2, "last"), (3, "end")];
let (smallest, _) = tessera_order::k_smallest_by_key(&mut rows, 2, |r| r.0);
assert_eq!(smallest, [(1, "middle"), (2, "first")]);
let (largest, _) = tessera_order::k_largest_by_key(&mut rows, 2, |r| r.0);
assert_eq!(largest, [(3, "end"), (2, "first")]);
```

`partial_sort` sorts any rank range and only partitions the rest; an empty range `k..k` separates the `k` smallest records without sorting them. Single and multiple selection use the same order:

```rust
let mut values = [9u64, 3, 1, 7, 2];
tessera_order::partial_sort(&mut values, 1..4);
assert_eq!(&values[1..4], &[2, 3, 7]);
let (_, median, _) = tessera_order::select_nth(&mut values, 2);
assert_eq!(*median, 3);
tessera_order::select_nth_many(&mut values, &[0, 4]);
assert_eq!((values[0], values[4]), (1, 9));
```

See [serial slice semantics](../../conformance/semantics.md#serial-slice-operations) for each operation's bounds, membership, empty-range and memory contract. These APIs are serial and need no feature; they do not create a persistent incremental sorter.

## Index output and chunks

Argsort leaves records in place. Chunk sorting sorts every `chunks_mut(chunk_len)` chunk independently in one invocation, such as the rows of a row-major matrix:

```rust
let source = [(2u32, "first"), (1, "middle"), (2, "last")];
assert_eq!(tessera_order::argsort_by_key(&source, |r| r.0), [1, 0, 2]);
let mut matrix = [3u64, 1, 2, 9, 7, 8];
tessera_order::sort_chunks(&mut matrix, 3);
assert_eq!(matrix, [1, 2, 3, 7, 8, 9]);
```

## Partitions and groups

`partition` moves the records matching a predicate first, keeping input order on both sides, and calls the predicate exactly once per record:

```rust
let mut events = [(false, "a"), (true, "b"), (true, "c"), (false, "d")];
let ready = tessera_order::partition(&mut events, |event| event.0);
assert_eq!(ready, 2);
assert_eq!(events, [(true, "b"), (true, "c"), (false, "a"), (false, "d")]);
```

To group by more categories, sort by the category and traverse the groups. When the classifier is costly or must not run twice for a record, use `sort_by_cached_key`. It caches each key once; when the keys span a small range, as category IDs do, the records move once:

```rust
let mut jobs = [(2, "a"), (0, "b"), (2, "c"), (0, "d")];
tessera_order::sort_by_cached_key(&mut jobs, |job| job.0 as u8);
let groups: Vec<_> = jobs.chunk_by(|a, b| a.0 == b.0).map(<[_]>::len).collect();
assert_eq!(groups, [2, 2]);
```

See [partition semantics](../../conformance/semantics.md#binary-partition) and [cached keys](../../conformance/semantics.md#cached-keys) for callback evaluation, panic recovery and memory. Categories need not correspond to key bits or ordered ranks.

## Multi-field sorting and groups

Compose stable sorts from the least significant field to the most significant. Traverse equal grouping keys with the standard slice API rather than a separate grouping engine:

```rust
let mut rows = [(2u32, 30u64), (1, 40), (2, 10), (1, 20)];
tessera_order::sort_by_key(&mut rows, |r| r.1); // Secondary: timestamp.
tessera_order::sort_by_key(&mut rows, |r| r.0); // Primary: user.
assert_eq!(rows, [(1, 20), (1, 40), (2, 10), (2, 30)]);
let counts: Vec<_> = rows.chunk_by(|a, b| a.0 == b.0)
    .map(|group| (group[0].0, group.len())).collect();
assert_eq!(counts, [(1, 2), (2, 2)]);
```

These public calls own independent workspace lifetimes; this composition does not promise cross-call allocation reuse. Group equality must match the chosen normalized key. Ordinary iterators and collections provide counting, reduction and deduplication, with their own consuming and callback-panic contracts.

## Parallel sorting

Enable the optional `rayon` feature for `par_sort` and `par_sort_by_key`. Add a direct Rayon dependency if the application configures a custom pool:

```toml
[dependencies]
tessera-order = { version = "0.1", features = ["rayon"] }
rayon = "1.10"
```

The entry points use Rayon's current pool. A caller can select its own pool with `install`:

```rust
let pool = rayon::ThreadPoolBuilder::new().num_threads(8).build().unwrap();
let mut records = [(2u32, "first"), (1, "middle"), (2, "last")];
pool.install(|| tessera_order::par_sort_by_key(&mut records, |record| record.0));
assert_eq!(records, [(1, "middle"), (2, "first"), (2, "last")]);
```

Records need `Send`; the by-key callback needs `Fn + Sync` because calls may overlap. Stability and record preservation remain unchanged. Small inputs and one-thread pools use the serial engine, and ordered/constant fast paths do not promise parallel acceleration. The [parallel contract](../../conformance/semantics.md#rust-rayon-entry-points) covers panic recovery and the O(P√n) auxiliary-space bound.

## When to use something else

- **Orders defined by a comparator**, strings, or other variable-length keys: use `slice::sort_by`. A comparator cannot be turned into radix digits.
- **No allocator**: Tessera needs O(√n) scratch memory, so it needs `alloc`.

The exact guarantees are in the [ordering semantics](../../conformance/semantics.md).
