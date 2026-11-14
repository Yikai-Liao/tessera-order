# Ordering semantics

Status: implemented by the unreleased Rust crate; the planned C++ implementation follows the same rules. Cross-language golden vectors in `conformance/vectors/` are planned with the C++ runner on the [roadmap](../ROADMAP.md); until then the Rust tests generate their own inputs.

## Sort contracts

| Contract | Guarantee |
|---|---|
| Stable | The output is ordered by key, and records with equal keys appear in their input order. |
| Unstable | The output is ordered by key. The order of records with equal keys is unspecified. |

Release 0.1 provides the stable contract only. This experimental branch exposes doc-hidden `prototype_unstable_sort` and `prototype_unstable_sort_by_key` entries with the unstable contract and the same key-encoding, extraction and permutation guarantees below. They are not a supported release API; their scope and memory evidence are in the [feasibility study](../docs/design/evidence.md#unstable-node-partition-feasibility). Parallel unstable sorting is not implemented.

Both contracts output a permutation of the input: no record is lost, duplicated or modified, whatever the key function returns.

Stability applies to every equal-key run, including one at the start of an otherwise strictly descending input. Sorting `[(3, a), (3, b), (2, c), (1, d)]` by its numeric key must produce `[(1, d), (2, c), (3, a), (3, b)]`.

## Key extraction

- The key function may be called any number of times per record, in any order.
- During one sort, it should return the same key for the same record. If it does not, the order of the result is unspecified, but the output is still a permutation of the input and no undefined behavior occurs. Changing keys do not cause unbounded partition recursion.
- If the key function panics (Rust) or throws (C++), the error propagates to the caller. The slice then holds every record exactly once, in an unspecified order. No particular panic strategy is required of the final binary.

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

Implementations document the scratch memory one call needs and fixed-size storage on the stack separately. Each Rust call owns its scratch and releases it before returning, including when a key function unwinds. Complete in-place sorting of already ordered or constant inputs does not allocate. The serial extensions below distinguish mandatory output and request metadata from sorting workspace.

The Rust implementation requires `std` and allocates heap scratch through the global allocator. One call needs O(√n) block scratch for a fixed record size and key width, plus storage whose size does not depend on `n`: a tile of records (at least one record), up to two bytes of cached bucket ID per tile record, a probe-key buffer of at most one tile of encoded keys, and cursor tables on the stack, one per tile-local MSD level, with depth bounded by the key width. Node partitions also nest at most once per key bit, whatever the key distribution. [Finishing tiles](../docs/explain/tiles.md) defines the tile and cursor-table sizes.

## Serial slice operations

These entry points are implemented without a feature gate. Each has a direct-key form and a `_by_key` form accepting `FnMut(&T) -> K`. Ordering and extractor behavior follow the contracts above; records need not implement `Clone`, `Send` or `Sync`. Bounds are validated before mutation or key calls. Invalid arguments panic. An extractor panic restores every record before propagating, without promising a valid partial result. User callback side effects are not rolled back.

Let `S` be a conceptual complete stable sort of the original records. Identity matters, not merely equality of keys; implementations need not materialize `S` or decorate records with indices.

| Entry | Successful result | Bounds and additional memory |
|---|---|---|
| `stable_argsort` | Return a permutation of source indices in stable encoded-key order. Source records are never moved. | O(n) index output plus the workspace for sorting those indices. A panic drops the partial output. |
| `stable_sort_segments` | Stably sort every segment independently. One invocation reuses private workspace across segments. | Borrowed offsets are nondecreasing, start at zero and end at the slice length; repeated offsets permit empty segments, and an empty slice accepts `[0]`. Workspace is bounded by the largest segment. On a panic, earlier segments may be sorted and later ones remain unvisited. |
| `stable_top_k` | Return disjoint mutable slices of the smallest k records and the remainder. Each contains exactly its corresponding stable ranked members, in unspecified internal order. | Accept `0..=len`. Zero and full length are no-ops without keys or allocation. No output allocation. |
| `stable_sort_prefix` | `values[..prefix_len]` equals `S[..prefix_len]`; the suffix contains exactly the other members, in unspecified order. This is sorted Top-k. | Accept `0..=len`. Zero is a no-op without keys or allocation; full length is a complete stable sort. No output allocation. |
| `stable_select_nth` | Return disjoint mutable borrows of the preceding records, `S[index]` and following records. Each outer region contains exactly its corresponding stable ranked members, without internal ordering guarantees. | Require `index < len`. No output allocation. |
| `stable_sort_rank_range` | For a nonempty range, `values[ranks.clone()]` equals `S[ranks.clone()]`. Prefix and suffix contain exactly their corresponding ranked members, in unspecified order. | Require `start <= end <= len`. An empty range is a no-op without keys or allocation and establishes no outer boundary. These are global ranks, not original subslice positions. No output allocation. |
| `stable_select_nth_many` | Every requested position holds `S[index]`; each intervening region contains exactly its corresponding ranked members, in unspecified order. | Accept unordered and repeated indices, all less than `len`. Normalize once, with O(m) request metadata for m indices. Empty requests are no-ops without keys or allocation. |

Stable Top-k and selection mean stable membership and ranked identity: when a boundary cuts an equal-key run, earlier input members are chosen first. They do not promise input order across different keys. For largest-k results, reverse the key; do not reverse a stable result. Statistical quantile definitions and interpolation remain caller choices; multiple selection accepts exact integer ranks.

In-place rank operations retain the full sort's [workspace bound](#space), not O(k) or O(requested output length). All scratch and request metadata are invocation-owned and released on return or unwind. Only argsort returns an allocated output. No public workspace, resumable session, category partition or parallel extension API is implied.

## Rust Rayon entry points

The optional `rayon` feature exposes `par_stable_sort` and `par_stable_sort_by_key`. All builds require `std` and use the same `fearless_simd` backend; Rayon is the only optional dependency. Small inputs and one-thread pools use the serial engine; large inputs partition the root across disjoint shards and sort root buckets in the caller's pool. Root completion streams source prefixes directly into exclusive final bands through one ownership ledger, then sorts each completed band. Both entry points preserve ordering, stability, records and panic recovery. All tasks restore their records before a key-function panic reaches the caller; which panic wins is unspecified. The entry points use Rayon's current pool rather than constructing a private pool; see [parallel usage](../docs/guide/getting-started.md#parallel-sorting) and [root completion](../docs/design/engine.md#parallel-root-completion).

With pool width `P`, parallel calls use O(P√n) auxiliary space for fixed record size and key width. Root scratch uses O(√(nP)). Each of at most `R` root-bucket finishers may own serial scratch, including when suspended by a reentrant callback; `R` is the fixed root fan-out, not the worker count. A complete child may use the [bounded parallel tile allowance](../docs/explain/tiles.md#parallel-child-allowance); larger children keep ordinary tiles. Checked-out and cached scratch both count toward the call, and nested public invocations have separate bounds. Direct-key counting adds O(PR) histogram storage. No contiguous n-record output buffer or private bucket staging is allocated. Rayon runtime storage and thread stacks are separate. Records require `Send`, not `Sync`, `Copy` or `Clone`. The by-key entry point requires `Fn + Sync` rather than `FnMut`; the function need not be `Send` or `Clone`, and key values remain local to each task. Key evaluations may run concurrently and in any order. The serial APIs retain their existing bounds.

## Language adapters

The rules above apply to Rust and C++. The [planned Python binding](../docs/design/python-binding.md) has its own contract: it sorts every NaN last in both directions, treats `-0.0` and `+0.0` as equal, keeps the original value bits, and calls a list's key function exactly once per element. These rules belong to the Python adapter and do not change the Rust encoding or key-extraction guarantees.
