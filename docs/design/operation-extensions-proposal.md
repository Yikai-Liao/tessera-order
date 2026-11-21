# Operation extensions proposal

This page records the unreleased 0.2 scope decision and constraints on future extensions. The [guide](../guide/getting-started.md) shows how to use the retained operations, the [engine reference](engine.md) owns their implementation, and the [roadmap](../../ROADMAP.md#operation-extension-experiment) tracks open acceptance work.

## Scope and direction

Keep stable full sorting, cached-key sorting, optional Rayon full sorting, argsort and single-rank selection. Argsort provides a different output without moving source records; single-rank selection can avoid finishing unrelated bands while preserving the same selected record and side membership as a full stable sort. Both have complete, useful contracts without exposing a second planner, workspace or movement protocol.

0.2 also admits two extensions that fill gaps in that surface without a new contract decision; see [0.2 additions](#0-2-additions).

<a id="public-surface-and-remaining-proposals"></a>
<a id="released-entry-points"></a>

## Release surface alignment

The 0.1 names `stable_sort`, `stable_sort_by_key`, `par_stable_sort` and `par_stable_sort_by_key` remain deprecated aliases for the shorter 0.2 names. Stability remains the default; reserve `_unstable` for a genuinely weaker tie contract.

### Rules

Admit an operation only when a mature precedent establishes its **name, parameters, and result** together. The result includes what is returned and what happens to the input. State the stronger tie behavior as a small delta from that precedent: with Tessera's [key encoding](../../conformance/semantics.md#key-encoding), every result must still be legal for the precedent, with ties resolved as a full stable sort resolves them. When conventions differ on what callers pass, what moves, or what comes back, keep the mechanism private rather than choosing a public signature because it fits the engine.

### Why tie identity is worth promising

[Rust slice selection](https://doc.rust-lang.org/std/primitive.slice.html#method.select_nth_unstable) takes one index, rearranges a mutable slice and returns three borrows. Tessera keeps that shape; stability determines which record is selected and which equal-key records go on either side, without promising either side is sorted. [NumPy's `argsort`](https://numpy.org/doc/stable/reference/generated/numpy.argsort.html) takes data to order and returns indices without rearranging the source; for a one-dimensional slice there is no axis choice, and Tessera makes stable tie order the default. Equal encoded direct keys are indistinguishable, but equal-key payloads and their indices are not.

### Decisions

The following families were removed before 0.2. A familiar term did not establish the whole contract:

| Family | Ambiguity |
|---|---|
| Predicate partition | [Rust's `Iterator::partition`](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.partition) takes a predicate and returns two collections; the draft Tessera function mutated a slice and returned a split index. [NumPy's `partition`](https://numpy.org/doc/stable/reference/generated/numpy.partition.html) takes ranks, not a predicate, and returns a reordered copy. The earlier count-taking category variant also exposed an internal fan-out limit. |
| Top-k | [`itertools::k_smallest`](https://docs.rs/itertools/latest/itertools/trait.Itertools.html#method.k_smallest) takes a count and returns a sorted `Vec`; the draft Tessera calls returned a sorted prefix and the remainder as borrows into a mutated slice. Requiring a sorted prefix rather than just membership is a separate contract choice. |
| Partial sort | [C++ `partial_sort`](https://eel.is/c++draft/partial.sort) specifies a prefix ending at `middle`; [Rust's experimental version](https://doc.rust-lang.org/std/primitive.slice.html#method.partial_sort_unstable) takes an arbitrary rank range. The range form also needs to distinguish a no-op empty range from a membership cut. |
| Multiple selection | NumPy accepts a sequence of ranks and returns an array; the draft `select_nth_many` took a rank slice and returned `()`, unlike the three borrows from single selection. A batch API must decide what duplicate and unsorted requests mean and whether it returns selected records or only mutates the input. |
| Chunk sorting | A fixed `chunk_len` has to define what happens to a short tail, as in Rust's [`chunks_mut` versus `chunks_exact_mut`](https://doc.rust-lang.org/std/primitive.slice.html#method.chunks_mut). An offset list instead describes variable-length segments. A precedent for iterating chunks does not establish a separate sorting operation or its result. |

See the [migration guide](../guide/getting-started.md#migrate-from-earlier-development-snapshots) for compositions and their costs. The [serial operation baseline](evidence.md#serial-slice-operation-baseline) and [category baseline](evidence.md#stable-category-partition-baseline) preserve the earlier results; performance alone does not settle the API contract.

<a id="0-2-additions"></a>

### 0.2 additions

An addition must save work that a composition of existing calls or the standard library cannot, meet the precedent rule above, and reuse an existing engine operation. The retained surface already covers every output the engine produces serially: moved records, cached keys, indices and one rank. The gaps are combinations of those outputs with capabilities already present, so 0.2 adds two.

- **Parallel argsort**: `par_argsort` and `par_argsort_by_key` under the `rayon` feature. NumPy's `argsort` fixes the result and Rayon's `par_` prefix the execution; the result equals `argsort`'s, ties included. Large index sorts over columns are the main dataframe and database consumer, and they are serial today. The engine argsort already sorts `usize` words through an extractor over the source slice, with key prefixes packed into spare index bits. The parallel form must reach those words through the existing parallel root rather than a second scatter. Its records are shared, so `T: Sync` replaces `par_sort_by_key`'s `T: Send`, and the key function is `Fn + Sync` as there.
- **Tuple keys**: `RadixKey` for `(A, B)` and longer tuples of keys whose encodings fit together in 128 bits, ordered like the tuple's `Ord`: the first field's encoding takes the high bits. Every existing entry point gains multi-column ordering without a new function; callers no longer pack signed or float fields by hand. A tuple too wide for 128 bits has no implementation, so it fails to compile. Only key encoding changes; varying-bit detection already discards constant high bits, and `Reverse` applies per field or to the whole tuple.

Not admitted, with the reason:

| Candidate | Reason |
|---|---|
| `par_sort_by_cached_key` | Rayon has the name, but it needs parallel cached-key distribution, which the engine lacks. Revisit after the parallel gaps above, with an expensive-key consumer. |
| Index selection (NumPy `argpartition`) | Building every index to return one rank saves little; it waits for [read-only small results](#further-engine-capabilities). |
| `par_select_nth` | No precedent; single-rank selection is bandwidth-bound and already skips unrelated bands. |
| `sort_unstable` | A radix engine saves nothing by dropping stability, so the name would not denote a weaker contract. |
| Unique values and counts | No Rust slice precedent fixes the result shape; it waits for a consumer of [full-key counts](#further-engine-capabilities). |
| Variable-length and string keys | They need a different representation than fixed-width encoded keys; separate work after 0.2. |

<a id="composition-and-persistent-use"></a>

No incremental prefix session is planned for 0.2. A future session would retain only completed range and progress facts across calls, not records in scratch or a live partition. It would need a concrete workload and a bound on retained state before an interface is proposed. External sorting and Wavelet matrix construction remain separate work.

<a id="implementation-consequences"></a>
<a id="derive-the-architecture-from-all-known-consumers"></a>
<a id="resource-and-failure-boundaries"></a>
<a id="trace-changes-through-the-target-boundaries"></a>
<a id="current-constraints-and-dependency-changes"></a>
<a id="alternatives-and-code-size-controls"></a>

## Ordered refinement without a general policy framework

The engine owns complete [sort and selection](engine.md#ordered-rank-requirements) and [distribution](engine.md#category-distribution) operations. Full sorting or one local rank is enough to express every retained ordered request. Rank projection stays with refinement; it does not influence partition planning or movement. Requested leaves remain fully sorted: an initial [tile-local selection experiment](evidence.md#tile-local-selection-experiment) did not improve on the complete tile sort. Cached-key distribution needs only the reordered records, so it does not return a boundary directory.

Keep planning, tile capacity, movement and recovery with their existing [owners](engine.md#knowledge-boundaries). Adding a public request builder, a policy trait, or separate full-sort and selection kernels would either make callers assemble operations the engine already owns or duplicate decisions that those operations share.

<a id="implementation-sequence"></a>
<a id="benchmark-workloads-and-the-existing-tools-cli"></a>

## Validation and evidence

Use [engine validation](../../CONTRIBUTING.md#validation). Check selected identity, both side memberships and argsort ties against an independent stable sort. Use the [maintained benchmark pipeline](../development/benchmarking.md#serial-operation-comparisons) for complete public calls, including output allocation; an unstable selection is not an equivalent stable-member baseline without tie handling. Evaluate structural changes separately from routing changes with the [source-attribution controls](../development/benchmarking.md#additional-source-attribution-controls). Preserve the existing figures and keep historical results and their limitations in [engine evidence](evidence.md).

## Further engine capabilities

Investigate these only with a concrete consumer and independent acceptance; none expands the 0.2 surface:

- **Exact key facts.** Reuse the order and variation facts already produced by cached-key extraction without dropping descending or budgeted insertion completion. Keep exact facts with their encoded keys rather than passing unrelated flags or treating a sample as proof.
- **Full-key counts.** A count or unique consumer could use the histogram before destination runs are filled. A bucket denotes a complete key only when the bits outside its digit are proven equal.
- **Read-only small results.** A kth value or a small set of indices could use prefix counts and bounded tile-local candidates. Account for repeated extraction and scans as well as output and candidate storage; do not build all indices solely to return a few.
