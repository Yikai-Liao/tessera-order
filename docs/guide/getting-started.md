# Getting started

Tessera sorts slices by a fixed-width key: an integer up to 128 bits, an `f32` or `f64`, `Reverse` of a supported key, or a tuple of supported keys up to 128 encoded bits ([key encoding](../../conformance/semantics.md#key-encoding)). Every operation is stable, runs in linear time for a fixed key width, and accepts records of any type.

This guide follows the main branch, which will become 0.2. [Versions](#versions) lists what 0.1 names differently and what it lacks.

## Install

```toml
[dependencies]
tessera-order = "0.1"
```

Tessera needs `std` and Rust 1.89 or newer. It selects supported vector instructions automatically; the only public feature is the optional [`rayon`](#parallel-sorting).

<a id="choose-an-operation"></a>

## Choose an operation

| You want to | Call |
|---|---|
| sort numbers | `sort` |
| sort records by a field | `sort_by_key` |
| sort by a key that is costly to compute | `sort_by_cached_key` |
| know the sorted order without moving records | `argsort`, `argsort_by_key` |
| find the record at one rank, such as the median | `select_nth`, `select_nth_by_key` |
| use several threads | `par_sort`, `par_sort_by_key` |

All of them place equal keys exactly as a stable sort would. Serial calls use O(√n) extra memory and parallel calls O(P√n) for P threads; in addition, argsort returns an index per record and cached-key sorting stores a key per record.

## Sort keys

```rust
let mut values: Vec<i64> = vec![42, -7, 19, -7, 3];
tessera_order::sort(&mut values);
assert_eq!(values, [-7, -7, 3, 19, 42]);
```

## Sort records by a key

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

Records with equal keys keep their input order, so `"c"` stays before `"d"`.

The key function may run several times per record, in any order, and must return the same key for a record throughout the call. If it does not, the call may panic or leave the order unspecified, but every record is still in the slice exactly once.

## Descending order

Wrap the key in `Reverse`. Equal keys still keep their input order:

```rust
use std::cmp::Reverse;

let mut rows = vec![(2u32, "first"), (1, "middle"), (2, "last")];
tessera_order::sort_by_key(&mut rows, |row| Reverse(row.0));
assert_eq!(rows, [(2, "first"), (2, "last"), (1, "middle")]);
```

Sorting ascending and then reversing the slice would also reverse the order of equal keys.

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

<a id="multi-field-sorting-and-groups"></a>

## Sort by several fields

When the fields fit within 128 encoded bits, return them as a tuple ([key encoding](../../conformance/semantics.md#key-encoding)). Tessera orders tuples lexicographically in one call, and `Reverse` works on individual fields:

```rust
use std::cmp::Reverse;

let mut rows = [(2u32, 30u64), (1, 40), (2, 10), (1, 20)];
// Primary: user ascending; secondary: timestamp descending (96 encoded bits).
tessera_order::sort_by_key(&mut rows, |r| (r.0, Reverse(r.1)));
assert_eq!(rows, [(1, 40), (1, 20), (2, 30), (2, 10)]);
let counts: Vec<_> = rows.chunk_by(|a, b| a.0 == b.0)
    .map(|group| (group[0].0, group.len())).collect();
assert_eq!(counts, [(1, 2), (2, 2)]);
```

After sorting, `chunk_by` walks the groups. Its equality must match the sort key.

When the combined keys exceed 128 encoded bits, sort in separate calls from the least significant field to the most significant. Stability keeps the earlier order within each group:

```rust
let mut events = [(2u128, 30u64), (1, 40), (2, 10), (1, 20)];
tessera_order::sort_by_key(&mut events, |e| e.1); // Secondary: timestamp.
tessera_order::sort_by_key(&mut events, |e| e.0); // Primary: 128-bit ID.
assert_eq!(events, [(1, 20), (1, 40), (2, 10), (2, 30)]);
```

<a id="index-output-and-chunks"></a>

## Get sorted indices

`argsort` and `argsort_by_key` leave the source in place and return the indices in stable sorted order:

```rust
let source = [(2u32, "first"), (1, "middle"), (2, "last")];
assert_eq!(tessera_order::argsort_by_key(&source, |r| r.0), [1, 0, 2]);
```

<a id="partial-sorting-and-selected-ranks"></a>

## Select one rank

`select_nth` and `select_nth_by_key` place the record at a zero-based rank and return the records before it, the record itself and the records after it. The answer is the one a full stable sort would give, including when equal keys straddle the rank, and both sides keep input order among equal keys. An out-of-range index panics before any record moves.

```rust
let mut rows = [(2u32, "first"), (1, "middle"), (2, "last")];
let (before, selected, after) = tessera_order::select_nth_by_key(&mut rows, 1, |r| r.0);
assert_eq!(before, [(1, "middle")]);
assert_eq!(*selected, (2, "first"));
assert_eq!(after, [(2, "last")]);
```

<a id="partitions-and-groups"></a>

## Group by a cached key

`sort_by_cached_key` calls the key function at most once per record: once each, in input order, unless the slice has fewer than two records or the records are zero-sized, which need no calls. Use it when the key is costly or must not run twice. Grouping by a small category is its best case: when the keys differ only in a few bits, the records are distributed in one pass.

```rust
let mut jobs = [(2, "a"), (0, "b"), (2, "c"), (0, "d")];
tessera_order::sort_by_cached_key(&mut jobs, |job| job.0 as u8);
let groups: Vec<_> = jobs.chunk_by(|a, b| a.0 == b.0).map(<[_]>::len).collect();
assert_eq!(groups, [2, 2]);
```

## Parallel sorting

Enable the `rayon` feature for `par_sort` and `par_sort_by_key`:

```toml
[dependencies]
tessera-order = { version = "0.1", features = ["rayon"] }
rayon = "1.10"  # only to configure a custom pool
```

They run in Rayon's current pool; `install` selects another:

```rust
let pool = rayon::ThreadPoolBuilder::new().num_threads(8).build().unwrap();
let mut records = [(2u32, "first"), (1, "middle"), (2, "last")];
pool.install(|| tessera_order::par_sort_by_key(&mut records, |record| record.0));
assert_eq!(records, [(1, "middle"), (2, "first"), (2, "last")]);
```

Records need `Send`, and the key function needs `Fn + Sync` because calls overlap. Small inputs and one-thread pools run serially, and sorted or constant inputs need not get faster with more threads. Extra memory is O(P√n) for P threads.

## When to use something else

- **A comparator**, strings or other variable-length keys: use `slice::sort_by`. A comparator cannot be turned into radix digits.
- **No allocator**: Tessera needs O(√n) scratch memory.

## Versions

The published 0.1 release has only the four full sorts, named `stable_sort`, `stable_sort_by_key`, `par_stable_sort` and `par_stable_sort_by_key`. Main renames them and keeps the old names as deprecated aliases. Cached-key sorting, argsort, selection and tuple keys are new and unreleased. 0.x minor releases may still change the API; see the [roadmap](../../ROADMAP.md).

<a id="try-the-development-api"></a>

To try the new operations now, depend on a local checkout. They need no feature flag:

```toml
[dependencies]
tessera-order = { path = "../tessera-order/rust/tessera-order" }
```

### Migrate from earlier development snapshots

These functions appeared only in unreleased checkouts. For `_by_key` variants, use the keyed operations with the same substitutions.

| Earlier API | Replacement |
|---|---|
| `partition` | `sort_by_cached_key(&mut records, \|r\| u8::from(!predicate(r)))` puts matches first and preserves order on both sides. Count matches during extraction to find the split. If the predicate must run for every record, handle slices shorter than two and zero-sized records separately. Cached sorting stores one key per record. |
| `k_smallest`, `k_largest` | Sort and take `[..k]`, sorting by `Reverse` for the largest. When `0 < k < len`, you can instead select rank `k` and sort the returned `before` slice, using the reversed order in both calls for the largest. |
| `partial_sort`, `select_nth_many` | Sort once, then take the range or read the positions. For one position, use `select_nth`. |
| `sort_chunks` | Sort each of `records.chunks_mut(chunk_len)`, including a short final chunk. Pass `&mut key` to each call if the extractor is stateful. |

## Exact guarantees

The [ordering semantics](../../conformance/semantics.md) state every contract: [serial operations](../../conformance/semantics.md#serial-slice-operations), [cached keys](../../conformance/semantics.md#cached-keys), [the parallel entry points](../../conformance/semantics.md#rust-rayon-entry-points), panics and memory.
