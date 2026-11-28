# Ordering semantics

Status: the Rust crate has supported stable full sorting since 0.1, under the `stable_` names that 0.2 renames. Cached-key sorting, argsort and single-rank selection are additions for 0.2. Cross-language golden vectors in `conformance/vectors/` are planned with the C++ runner on the [roadmap](../ROADMAP.md); until then the Rust tests generate their own inputs.

## Sort contracts

`sort`, `sort_by_key`, `sort_by_cached_key`, and the in-place [Rayon sorting entry points](#rust-rayon-entry-points) order the input by key and keep records with equal keys in their input order. The engine moves records without cloning, dropping or modifying them. Changes made by the caller through a key function are retained.

Stability applies to every equal-key run, including one at the start of an otherwise strictly descending input. Sorting `[(3, a), (3, b), (2, c), (1, d)]` by its numeric key must produce `[(1, d), (2, c), (3, a), (3, b)]`.

## Key extraction

Except for [cached keys](#cached-keys), the key function may be called any number of times per record, in any order. It must return the same key for a record throughout the call to obtain the specified result. As with an inconsistent order in Rust's [`slice::sort_by_key`], inconsistent keys may cause a panic or a normal return without ordering, stability or rank guarantees. Neither outcome loses or duplicates records or causes undefined behavior. Changing keys cannot cause unbounded partition recursion. If argsort returns normally, its output is still a permutation of the source indices.

### Cached keys

`sort_by_cached_key`, like Rust's [`slice::sort_by_cached_key`], evaluates the key at most once per record. Tessera specifies the order: on success it calls the key once per record in input order, unless the slice has fewer than two records or the record type is zero-sized, in which case it makes no calls. The single returned key for each record determines its ordering; the function need not be repeatable.

## Panics and unwinding

Tessera follows Rust's slice ownership guarantee, described in [`slice::sort_by_key`] and [`slice::sort_by_cached_key`]. If a key-function panic unwinds, the panic propagates after the slice again holds every original record exactly once, including changes made through interior mutability. Recovery itself does not call the key function or roll back callback side effects. Record order is unspecified: neither input order nor a sorted or partitioned partial result is guaranteed. These guarantees describe unwinding, not recovery from a process abort.

Argsort never moves the source records. If its key function unwinds, the partial index output is discarded and changes through interior mutability remain visible in the source.

[`slice::sort_by_key`]: https://doc.rust-lang.org/stable/std/primitive.slice.html#method.sort_by_key
[`slice::sort_by_cached_key`]: https://doc.rust-lang.org/stable/std/primitive.slice.html#method.sort_by_cached_key
[`slice::select_nth_unstable_by_key`]: https://doc.rust-lang.org/stable/std/primitive.slice.html#method.select_nth_unstable_by_key

## Key encoding

Each supported key maps to an unsigned integer that covers its encoded bits. Records are ordered by that integer.

| Key type | Encoding |
|---|---|
| `bool` | `0u8` for `false`, `1u8` for `true` |
| `char` | Unicode scalar value as `u32` |
| Unsigned integers | Unchanged |
| Signed integers | The sign bit is flipped |
| `NonZero<T>` (primitive integers) | Same encoding as `T` |
| `f32`, `f64` | If the sign bit is set, all bits are inverted; otherwise only the sign bit is flipped. This matches Rust's `total_cmp`. |
| 128-bit integers | Compared as a high 64-bit half, then a low 64-bit half. Signed 128-bit keys flip the sign bit of the high half. |
| `[u8; 1]` … `[u8; 16]` | Big-endian bytes in the narrowest covering unsigned integer, with any unused high bits zeroed |
| `Reverse<K>` | All effective bits of the encoding of `K` are inverted |
| Tuples of 2 to 16 fields (up to 128 encoded bits) | Fields are concatenated in order into the narrowest covering unsigned integer, with the first field in the highest bits and any unused high bits zeroed |

### Consequences for floats

The resulting order is: negative NaNs, `-inf`, negative numbers, `-0.0`, `+0.0`, positive numbers, `+inf`, positive NaNs. NaNs of the same sign are ordered by payload.

Applications that need all NaNs to compare equal, or `-0.0` equal to `+0.0`, should normalize the key in the key function, for example by mapping every NaN to one canonical NaN and `-0.0` to `+0.0`.

### Descending order

Descending order is expressed by reversing the key, not by reversing an ascending result. Reversing the result would also reverse the order of equal keys and break stability.

## Space

Implementations document the scratch memory one call needs and fixed-size storage on the stack separately. Each Rust call owns its scratch and releases it before returning or on unwind. Complete in-place sorting of already ordered or constant inputs does not allocate, except with [cached keys](#cached-keys).

The Rust implementation requires `std` and allocates heap scratch through the global allocator. One call needs O(√n) block scratch for a fixed record size and key width, plus storage whose size does not depend on `n`: a tile of records (at least one record), up to two bytes of cached bucket ID per tile record, a probe-key buffer of at most one tile of encoded keys, and cursor tables on the stack, one per tile-local MSD level, with depth bounded by the key width. Node partitions also nest at most once per key bit, whatever the key distribution. [Finishing tiles](../docs/explain/tiles.md) defines the tile and cursor-table sizes.

`sort_by_cached_key` caches every encoded key, O(n) like the standard library's cached-key sort, also for ordered or constant inputs: their order is known only after every key has been evaluated. Ordered inputs need no further storage. When the cached keys vary only within a window that one stable distribution covers, the records move once by that window, with the bound above and no further key storage. Otherwise the engine resolves a stable source permutation using one index per record plus sorting workspace. It releases the cached keys before applying the permutation.

## Serial slice operations

`argsort` and `select_nth` are available without a feature gate. Each has a direct-key form and a `_by_key` form accepting `FnMut(&T) -> K`; the latter does not require `Clone`, `Send` or `Sync` records. Let `S` be a conceptual complete stable sort of the input. Both operations use record identity, not just key equality, to define their results.

- `argsort` returns the source indices in the order of `S`. Equal keys keep increasing source-index order. It allocates O(n) for the output, plus workspace for sorting the indices.
- `select_nth` returns three disjoint mutable borrows: the records before the selected position, the record `S[index]`, and the records after it. Each outer region contains exactly its members of `S`. Equal-key records keep their original relative order throughout the slice. Within a region, the order of records with different keys is unspecified. It uses the full sort's [workspace bound](#space) and allocates no output. As with Rust's [`slice::select_nth_unstable_by_key`], `index >= len` panics, including every index on an empty slice. Tessera checks the index before moving records or calling the key function.

## Rust Rayon entry points

The optional `rayon` feature exposes `par_sort`, `par_sort_by_key`, `par_argsort` and `par_argsort_by_key`. All builds require `std` and use the same `fearless_simd` backend; Rayon is the only optional dependency. Small inputs and one-thread pools use the serial engine; large inputs partition the root across disjoint shards and sort root buckets in the caller's pool. Root completion streams source prefixes directly into exclusive final bands through one ownership ledger, then sorts each completed band. The same ordering, stability and [unwind guarantees](#panics-and-unwinding) apply. Other tasks may continue key evaluation after one task panics. Tasks join and all records are restored before the panic reaches the caller; if multiple tasks panic, which panic propagates is unspecified. The entry points use Rayon's current pool rather than constructing a private pool; see [parallel usage](../docs/guide/getting-started.md#parallel-sorting) and [root completion](../docs/design/engine.md#parallel-root-completion).

With pool width `P`, parallel calls use O(P√n) auxiliary space for fixed record size and key width. Root scratch uses O(√(nP)). Each of at most `R` root-bucket finishers may own serial scratch, including when suspended by a reentrant callback; `R` is the fixed root fan-out, not the worker count. A complete child may use the [bounded parallel tile allowance](../docs/explain/tiles.md#parallel-child-allowance); larger children keep ordinary tiles. Checked-out and cached scratch both count toward the call, and nested public invocations have separate bounds. Direct-key counting adds O(PR) histogram storage. No contiguous n-record output buffer or private bucket staging is allocated for in-place sorting; `par_argsort` and `par_argsort_by_key` allocate the O(n) index output plus the same O(P√n) auxiliary workspace. Rayon runtime storage and thread stacks are separate. In-place sorting records require `Send`, not `Sync`, `Copy` or `Clone`; parallel argsort borrows source records across workers and requires `Sync`, not `Send`, `Copy` or `Clone`. The by-key entry points require `Fn + Sync` rather than `FnMut`; the function need not be `Send` or `Clone`, and key values remain local to each task. Key evaluations may run concurrently and in any order. The serial APIs retain their existing bounds.

## Ordered collections

`TesseraMap<K, V>` and `TesseraSet<K>` provide ordered maps and sets keyed by any [`RadixKey`](#key-encoding). `TesseraSet<K>` delegates directly to `TesseraMap<K, ()>`.

- **Key ordering and equivalence.** Keys are ordered and deduplicated strictly by their [encoded unsigned representation](#key-encoding). In particular, `-0.0` and `+0.0` are distinct keys, and NaN values compare equal only when their bit patterns match.
- **Duplicate resolution.** Batch construction (`From<Vec<_>>`, `FromIterator`) and `extend_vec` resolve equal keys with last-value-wins semantics, matching sequential `insert`. Single-key `insert` replaces an existing value and returns the previous value.
- **Lazy batch adoption and refinement.** Small batches build eagerly; large batches (`>= 16,384` records) adopt the incoming `Vec` allocation in O(1) without shrinking or copying, and refine only touched key buckets during `&self` queries (`get`, `contains_key`, `range`, `first`, `last`, `select_nth`). `is_empty` runs in O(1) without refining unrefined buckets.
- **Exact length versus provisional diagnostics.** `len` and `select_nth` on a batch constructed via `From<Vec<_>>`, `FromIterator`, or extended via `extend_vec` resolve remaining unrefined buckets once so returned lengths and ranks are exact. `from_unique` assumes distinct keys so `len` and `select_nth` stay lazy; if duplicate keys are present anyway, refinement still deduplicates them safely (last value wins), while untouched buckets keep their raw counts until refined. `stats()` inspects internal structure without refining and reports provisional counts until all buckets are refined.
- **Reference stability and thread safety.** `TesseraMap<K, V>` is `Send` when `V: Send` and `Sync` when `V: Send + Sync`. Published leaves live in stable-address storage and never move or mutate during `&self` operations, so `&V` references returned by `&self` reads remain valid across concurrent reads and lazy refinement of other buckets.
- **Ownership and unwinding.** Every owned value is dropped once. Overwritten duplicate values discovered during lazy refinement are dropped only after the refined subtree is published and internal locks are released, so a reentrant value destructor cannot deadlock and a panicking destructor leaves the map consistent and recoverable. During container teardown, a single panicking value destructor does not skip dropping remaining published or unrefined values or leak backing allocations.

## Language adapters

The planned C++ implementation follows the same ordering rules. The [planned Python binding](../docs/design/python-binding.md) has its own contract: it sorts every NaN last in both directions, treats `-0.0` and `+0.0` as equal, keeps the original value bits, and calls a list's key function exactly once per element. These rules belong to the Python adapter and do not change the Rust encoding or key-extraction guarantees.
