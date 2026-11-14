# Ordering semantics

Status: full-sort contracts are implemented by the Rust crate since 0.1, under the `stable_` names that 0.2 renames; the slice operations and binary partition below are unreleased additions for 0.2. The planned C++ implementation follows the same rules. Cross-language golden vectors in `conformance/vectors/` are planned with the C++ runner on the [roadmap](../ROADMAP.md); until then the Rust tests generate their own inputs.

## Sort contracts

| Contract | Guarantee |
|---|---|
| Stable | The output is ordered by key, and records with equal keys appear in their input order. |
| Unstable | The output is ordered by key. The order of records with equal keys is unspecified. |

The Rust entry points implement the stable contract. An unstable API is not implemented.

Both contracts output a permutation of the input: no record is lost, duplicated or modified, whatever the key function returns.

Stability applies to every equal-key run, including one at the start of an otherwise strictly descending input. Sorting `[(3, a), (3, b), (2, c), (1, d)]` by its numeric key must produce `[(1, d), (2, c), (3, a), (3, b)]`.

## Key extraction

- The key function may be called any number of times per record, in any order.
- During one sort, it should return the same key for the same record. If it does not, the sort may panic (Rust) or throw (C++), or return with the result in unspecified order, as Rust's standard sorts may for an inconsistent order. Either way the slice holds every record exactly once, no undefined behavior occurs, and changing keys do not cause unbounded partition recursion.
- If the key function panics (Rust) or throws (C++), the error propagates to the caller. The slice then holds every record exactly once, in an unspecified order. No particular panic strategy is required of the final binary.

### Cached keys

`sort_by_cached_key` follows the stable contract with a stronger extraction rule, as [`slice::sort_by_cached_key`](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_by_cached_key) does. Its key function runs exactly once per record, in input order, when the input has at least two records that are not zero-sized; otherwise it runs not at all. Key consistency is therefore not a caller obligation. A key panic propagates with every record still in the slice, in unspecified order; recovery does not call the key function. Callback side effects are not rolled back.

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

Implementations document the scratch memory one call needs and fixed-size storage on the stack separately. Each Rust call owns its scratch and releases it before returning, including when a key function unwinds. Complete in-place sorting of already ordered or constant inputs does not allocate, except with [cached keys](#cached-keys). The serial extensions below distinguish mandatory output and request metadata from sorting workspace.

The Rust implementation requires `std` and allocates heap scratch through the global allocator. One call needs O(√n) block scratch for a fixed record size and key width, plus storage whose size does not depend on `n`: a tile of records (at least one record), up to two bytes of cached bucket ID per tile record, and cursor tables on the stack, one per tile-local MSD level, with depth bounded by the key width. Node partitions also nest at most once per key bit, whatever the key distribution. [Finishing tiles](../docs/explain/tiles.md) defines the tile and cursor-table sizes.

`sort_by_cached_key` caches every encoded key, O(n) like the standard library's cached-key sort, also for ordered or constant inputs: their order is known only after every key has been evaluated. Ordered inputs need no further storage. When the cached keys vary only within a window that one stable distribution covers, the records move once by that window, with the bound above and no further key storage. Otherwise the keys are paired with record indices and the pairs are sorted, which adds one pair per record and the sort scratch for the pairs.

## Serial slice operations

These entry points are implemented without a feature gate. Each has a direct-key form and a `_by_key` form accepting `FnMut(&T) -> K`; `k_largest` also has both. Ordering and extractor behavior follow the contracts above; records need not implement `Clone`, `Send` or `Sync`. Bounds are validated before mutation or key calls. Invalid arguments panic. An extractor panic restores every record before propagating, without promising a valid partial result. User callback side effects are not rolled back.

Let `S` be a conceptual complete stable sort of the original records. Identity matters, not merely equality of keys; implementations need not materialize `S` or decorate records with indices. Each entry returns a result its mature counterpart may return, with equal keys resolved as `S` resolves them.

| Entry | Counterpart | Successful result | Bounds and additional memory |
|---|---|---|---|
| `argsort` | NumPy `argsort(kind="stable")` | A permutation of source indices in the order of `S`. Source records are never moved. | O(n) index output plus the workspace for sorting those indices. A panic drops the partial output. |
| `sort_chunks` | `sort` on each `chunks_mut(chunk_len)` | Every chunk of `chunk_len` records, and a shorter last chunk, is sorted independently. One invocation reuses private workspace across chunks. | `chunk_len` must be nonzero. Workspace is bounded by one chunk. On a panic, earlier chunks may be sorted and later ones remain unvisited. |
| `partial_sort` | `slice::partial_sort_unstable` (nightly) | For a nonempty range, `values[range]` equals `S[range]`, and the records before and after it are exactly the members of `S` there, in unspecified order. An empty range `k..k` with `0 < k < len` leaves exactly `S[..k]` before `k`, unordered. | The range resolves like slice indexing, so `start <= end <= len`. An empty range at either end is a no-op without keys or allocation. These are global ranks, not original subslice positions. No output allocation. |
| `k_smallest` | itertools `k_smallest` | Returns disjoint borrows of `S[..k]`, in order, and the remaining records, in unspecified order. This is `partial_sort(..k)`. | Require `k <= len`. Zero is a no-op without keys or allocation; full length is a complete stable sort. No output allocation. |
| `k_largest` | itertools `k_largest` | `k_smallest` under `Reverse` of the key: the `k` largest records first, in descending order, equal keys in input order. | As `k_smallest`. |
| `select_nth` | `slice::select_nth_unstable` | Returns disjoint borrows of the preceding records, `S[index]` and the following records. Each outer region contains exactly its members of `S`, in unspecified order. | Require `index < len`. No output allocation. |
| `select_nth_many` | NumPy `partition(kth=indices)` | Every requested position holds `S[index]`; each intervening region contains exactly its members of `S`, in unspecified order. | Accept unordered and repeated indices, all less than `len`. Normalize once, with O(m) request metadata for m indices. Empty requests are no-ops without keys or allocation. |

Stable membership and ranked identity mean that when a boundary cuts an equal-key run, earlier input members are chosen first. They do not promise input order across different keys. Statistical quantile definitions and interpolation remain caller choices; multiple selection accepts exact integer ranks.

In-place rank operations retain the full sort's [workspace bound](#space), not O(k) or O(requested output length). All scratch and request metadata are invocation-owned and released on return or unwind. Only argsort in this section returns an allocated output. No public workspace, resumable session or parallel extension API is implied.

## Binary partition

`partition`, the C++ `std::stable_partition`, moves the records matching a predicate before the others, keeping original order in both groups, and returns the matching count. The predicate runs exactly once per record, in original input order, including zero-sized records and singleton inputs; empty input makes no calls. A predicate panic propagates after every record is back in the input, in unspecified order; recovery does not call the predicate, and callback side effects are not rolled back. Auxiliary space is O(√n) for a fixed record size, plus bounded tile storage, all released before return or unwind. No `Clone`, `Send` or `Sync` record bound is required.

No entry takes a category count. Grouping by more categories uses `sort_by_cached_key` or `sort_by_key`, followed by `slice::chunk_by`.

## Rust Rayon entry points

The optional `rayon` feature exposes `par_sort` and `par_sort_by_key`. All builds require `std` and use the same `fearless_simd` backend; Rayon is the only optional dependency. Small inputs and one-thread pools use the serial engine; large inputs partition the root across disjoint shards and sort root buckets in the caller's pool. Root completion streams source prefixes directly into exclusive final bands through one ownership ledger, then sorts each completed band. Both entry points preserve ordering, stability, records and panic recovery. All tasks restore their records before a key-function panic reaches the caller; which panic wins is unspecified. The entry points use Rayon's current pool rather than constructing a private pool; see [parallel usage](../docs/guide/getting-started.md#parallel-sorting) and [root completion](../docs/design/engine.md#parallel-root-completion).

With pool width `P`, parallel calls use O(P√n) auxiliary space for fixed record size and key width. Root scratch uses O(√(nP)). Each of at most `R` root-bucket finishers may own serial scratch, including when suspended by a reentrant callback; `R` is the fixed root fan-out, not the worker count. A complete child may use the [bounded parallel tile allowance](../docs/explain/tiles.md#parallel-child-allowance); larger children keep ordinary tiles. Checked-out and cached scratch both count toward the call, and nested public invocations have separate bounds. Direct-key counting adds O(PR) histogram storage. No contiguous n-record output buffer or private bucket staging is allocated. Rayon runtime storage and thread stacks are separate. Records require `Send`, not `Sync`, `Copy` or `Clone`. The by-key entry point requires `Fn + Sync` rather than `FnMut`; the function need not be `Send` or `Clone`, and key values remain local to each task. Key evaluations may run concurrently and in any order. The serial APIs retain their existing bounds.

## Language adapters

The rules above apply to Rust and C++. The [planned Python binding](../docs/design/python-binding.md) has its own contract: it sorts every NaN last in both directions, treats `-0.0` and `+0.0` as equal, keeps the original value bits, and calls a list's key function exactly once per element. These rules belong to the Python adapter and do not change the Rust encoding or key-extraction guarantees.
