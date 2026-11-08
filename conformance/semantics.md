# Ordering semantics

Status: implemented by the unreleased Rust crate; the planned C++ implementation follows the same rules. Shared test vectors in `conformance/vectors/` will be added with the benchmark runner; until then the Rust tests generate their own inputs.

## Sort contracts

| Contract | Guarantee |
|---|---|
| Stable | The output is ordered by key, and records with equal keys appear in their input order. |
| Unstable | The output is ordered by key. The order of records with equal keys is unspecified. |

Release 0.1 provides the stable contract only. The unstable contract is reserved for a dedicated unstable kernel in a later release.

Both contracts output a permutation of the input: no record is lost, duplicated or modified, whatever the key function returns.

## Key extraction

- The key function may be called any number of times per record, in any order.
- During one sort, it should return the same key for the same record. If it does not, the order of the result is unspecified, but the output is still a permutation of the input and no undefined behavior occurs.
- If the key function panics (Rust) or throws (C++), the error propagates to the caller. The slice then holds every record exactly once, in an unspecified order. Without the Rust `std` feature, a key-function panic during block partitioning aborts, even if the host enables unwinding; panics elsewhere may unwind with every record preserved. No particular panic strategy is required of the final binary.

## Key encoding

Each supported key maps to an unsigned integer of the same width. Records are ordered by that integer.

| Key type | Encoding |
|---|---|
| Unsigned integers | Unchanged |
| Signed integers | The sign bit is flipped |
| `f32`, `f64` | If the sign bit is set, all bits are inverted; otherwise only the sign bit is flipped. This matches Rust's `total_cmp`. |
| 128-bit integers | Compared as a high 64-bit half, then a low 64-bit half. Signed 128-bit keys flip the sign bit of the high half. |
| `Reverse<K>` | All bits of the encoding of `K` are inverted |

### Consequences for floats

The resulting order is: negative NaNs, `-inf`, negative numbers, `-0.0`, `+0.0`, positive numbers, `+inf`, positive NaNs. NaNs of the same sign are ordered by payload.

Applications that need all NaNs to compare equal, or `-0.0` equal to `+0.0`, should normalize the key in the key function, for example by mapping every NaN to one canonical NaN and `-0.0` to `+0.0`.

### Descending order

Descending order is expressed by reversing the key, not by reversing an ascending result. Reversing the result would also reverse the order of equal keys and break stability.

## Space

Implementations document three quantities separately:

1. the scratch memory one call needs;
2. fixed-size storage on the stack;
3. capacity a reusable workspace retains between calls.

A workspace reuses capacity only. Every call re-establishes which scratch records are live; bytes left by an earlier call are never treated as records. Sorting with a workspace produces the same result as the corresponding free function, and a workspace stays usable after a key function panics.

In Rust, the heap memory comes from `alloc`, so a `no_std` build needs a global allocator. One call needs O(√n) block scratch for a fixed record size and key width, a tile of up to 64 KiB of records (at least one record), up to two bytes of cached bucket ID per tile record, and at most 65,535 encoded probe keys of up to 16 bytes. Each tile-local MSD level holds up to 32 KiB of cursors on the stack, with at most 22 levels for 128-bit keys. `Workspace::retained_bytes()` reports the retained heap capacity and `shrink_to_fit()` releases all of it.

## Rust Rayon entry points

The optional `rayon` feature enables `std` and exposes `par_stable_sort` and `par_stable_sort_by_key`. Small inputs and one-thread pools use the serial engine; large inputs partition the root across disjoint shards, permute blocks in parallel and sort complete root buckets in the caller's pool. Both entry points preserve ordering, stability, records and panic recovery ([proposal](../docs/proposals/rayon-parallel-sort.md)).

With `P` root shards, parallel calls use O(√(nP)) auxiliary scratch for fixed record size and key width; for a fixed pool width this is O(√n). The root digit is bounded by 12 bits (4096 buckets), so per-bucket workspaces remain bounded independently of pool width. They do not allocate an n-record buffer. Records require `Send` but do not require `Sync`. The by-key entry point requires `Fn + Sync` rather than `FnMut`; key evaluations may run concurrently and in any order. The serial APIs and `Workspace` retain their existing bounds.

## Language adapters

The rules above apply to Rust and C++. The [planned Python binding](../docs/proposals/python-binding.md) has its own contract: it sorts every NaN last in both directions, treats `-0.0` and `+0.0` as equal, keeps the original value bits, and calls a list's key function exactly once per element. These rules belong to the Python adapter and do not change the Rust encoding or key-extraction guarantees.
