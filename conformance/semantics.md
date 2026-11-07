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

## Language adapters

The rules above apply to Rust and C++. The [planned Python binding](../docs/proposals/python-binding.md) has its own contract: it sorts every NaN last in both directions, treats `-0.0` and `+0.0` as equal, keeps the original value bits, and calls a list's key function exactly once per element. These rules belong to the Python adapter and do not change the Rust encoding or key-extraction guarantees.

## Optional Rust parallel API

The `rayon` Cargo feature enables `par_stable_sort` and
`par_stable_sort_by_key` and implies `std`. The default features are unchanged;
`default-features = false` without `rayon` remains `no_std` with `alloc` and has
no runtime dependencies.

Parallel sorting preserves the same encoding, stability and permutation
contracts. Records need `Send`, not `Sync`, and are not cloned. A parallel key
function implements `Fn + Sync` because independent buckets may call it
concurrently. It need not implement `Send` or `Clone`. A key-function panic
propagates only after all started tasks have completed or unwound and restored
every record. When multiple tasks panic, which panic is propagated is unspecified.

The parallel implementation finishes the root stable block partition before
starting any bucket tasks. It uses at most P workspaces, bounded by the active
Rayon pool size and input grain count, rather than an O(n) output array. Its
heap scratch is O(√n + √(Pn) + P) for a fixed record size and key width, excluding
Rayon's pool and thread stacks. The P term includes per-worker tile and fixed
metadata storage. For fixed P this remains O(√n); it is not a worker-count-
independent O(√n) claim. Each active worker has the sequential engine's bounded
tile-local cursor stack. Parallel free functions release their scratch on return.
