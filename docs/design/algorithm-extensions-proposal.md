# Algorithm extensions proposal

Status: the shared foundations and first serial one-shot operations are implemented. This page owns remaining candidate interfaces and migration choices, not current API contracts. Implemented boundaries belong in the [engine reference](engine.md); the [roadmap](../../ROADMAP.md#algorithm-extension-experiment) tracks acceptance. Current guarantees belong in [semantics](../../conformance/semantics.md#serial-slice-operations). Unimplemented interfaces stay in this document, not as exported stubs or `todo!()` functions.

## Scope and direction

Extend Tessera through complete operations for stable distribution and ordered refinement, not a public algorithm framework. Stable distribution groups records by category while preserving input order within each category. Ordered refinement additionally knows that buckets occupy ordered rank intervals, and continues only where a requested result needs more work. A category is not necessarily a key prefix or a rank interval.

Design the target architecture as if the known sorting, selection, partitioning, batch and incremental consumers were required from day zero. Their result contracts, ownership and resource lifetimes determine the boundaries; the current `Sorter` and the first feature do not. Settle that target before migrating the implementation, then deliver independently verifiable slices from simple compositions to stateful algorithms. External sorting and Wavelet matrix construction receive neither an interface nor an implementation in this experiment. Their known roles remain compatible with the boundary: run generation consumes complete sorting, while Wavelet construction needs controlled stable passes, not rank refinement. A priority queue is outside this proposal.

The implemented first slice covers argsort, standard-library compositions for multiple fields and groups, segmented sorting, unordered Top-k, sorted prefixes, rank selection and sorted rank ranges. Stable binary and bounded multiway partitioning are now implemented as complete one-shot distribution calls. Incremental prefix completion follows only after the one-shot operations work. These are unreleased additions after Rust 0.1, not changes to the published release.

## Public surface and remaining proposals

Argsort, chunk sorting, partial sorting, `k_smallest` and `k_largest`, single and multiple selection, cached-key sorting and binary partition are implemented under the names of the [release surface](#release-surface-alignment). Their one contract owner is [serial slice semantics](../../conformance/semantics.md#serial-slice-operations); [public Rust docs](../../rust/tessera-order/src/algorithms.rs) provide signatures and examples, and the [engine reference](engine.md#category-distribution) owns the one-call movement behind `partition` and `sort_by_cached_key`. None of them is released; they ship with 0.2. Incremental prefix completion remains the next proposed algorithm with persistent state.

### Composition and persistent use

[Multi-field and grouping examples](../guide/getting-started.md#multi-field-sorting-and-groups) use ordinary stable sorting and slice composition, not new frameworks. Tuple keys and variable-width encodings are not implied by `RadixKey`. Consuming callbacks, reductions and deduplication retain their own destructor and panic contracts, separate from sorting's permutation guarantee. Exact statistical ranks remain caller choices rather than quantile policies in the engine.

Incremental completion remains a proposed borrowing object, not a record-removing iterator: `PrefixSorter::by_key(&mut values, key)` followed by `ensure_prefix(prefix_len) -> &[T]`. Smaller requests may reuse completed work; extension needs an exclusive borrow. Names and generics remain provisional. Keys must stay consistent throughout the session; a panic invalidates further session use while restoring all records. No live record or half-finished movement may remain in scratch between calls.

## Release surface alignment

Status: accepted by the operator and implemented for 0.2. Contracts now belong to [semantics](../../conformance/semantics.md#serial-slice-operations). This section keeps the rules and decisions that later additions follow, such as a future `_unstable` variant.

### Rules

1. **State each operation as a delta from a mature one.** A public operation's result is a legal output of a named mature operation. Tessera adds only two differences: keys follow the [`RadixKey` encoding](../../conformance/semantics.md#key-encoding) total order instead of `Ord`, and wherever record identity is observable, equal keys resolve as the complete stable sort `S` resolves them. Rustdoc and semantics express each contract as that delta, not as an independent definition.
2. **Promise stability only where it is observable.** Equal encoded direct keys are bitwise identical, so a direct-key result cannot be distinguished from a legal output of the unstable standard operation; the engine already counts and refills such keys for that reason. Stability is a contract for records moved by a key function and for returned indices.
3. **Copy the Rust shape from a Rust precedent where one exists.** Arguments, returned borrows and panic conditions follow the corresponding standard-library method. Without a Rust precedent, a precedent in another ecosystem (NumPy, pandas, CUB) fixes the semantics and the name follows rule 5; the missing precedent is no reason to withhold an operation.
4. **Publish an operation when callers need it and a composition does not serve them.** A composition does not serve them when it cannot give the same guarantee, when it costs measurably more, or when callers are unlikely to discover it. A planned language binding is a caller: the Python binding reaches the core only through the public Rust API. Adding a function later is compatible; removing a released one is not.
5. **Use the Rust naming convention.** In Rust an unsuffixed name is stable and the cheaper weaker variant carries `_unstable`: `slice::sort` and `sort_unstable`, Rayon `par_sort` and `par_sort_unstable`, and the stable sorting crates `radsort` and `glidesort`, whose free functions are `sort` and `sort_by_key`. Tessera's operations are all stable, so they take the standard method name without its `_unstable` suffix, `_by_key` as in the standard library, and `par_` for Rayon entry points; `_unstable` stays reserved for a future weaker variant. The 0.1 `stable_` prefix follows C++ (`std::stable_sort`) instead, and keeping it would make every later name a hybrid of the two conventions. There are no `_by` comparator variants, because the radix engine needs encoded keys; the crate documentation states this once. Language bindings follow their host's convention.

### Why tie identity is worth promising

Low-level in-place selection is unstable across ecosystems (`slice::select_nth_unstable`, C++ `std::nth_element`, `numpy.partition`) because comparison-based stable selection needs an O(n) buffer, index decoration or O(n log n) in-place partitioning. Libraries that return selected results do define ties: pandas `nsmallest` defaults to `keep="first"`, JAX `lax.top_k` defaults to `is_stable=True`, and NumPy and PyTorch offer stable argsort. Tessera moves records with a stable block partition, and an unstable node kernel showed no intrinsic speed benefit ([feasibility study](evidence.md#unstable-node-partition-feasibility)), so the cost that made low-level selection unstable does not apply here.

The user-visible value is consistency with the complete stable sort. Pages or batches cut by partial sorting match the full order, results are reproducible across versions and platforms, equal priorities are served in arrival order, and a stable pass on a secondary key composes with a later rank operation on the primary key. The caller's alternative, an original-index tiebreaker, is nearly free for a comparison sort but widens a radix key and adds digit passes. The promise only excludes faster unstable kernels under these names; `*_unstable` names stay available.

### Released entry points

0.2 renames `stable_sort`, `stable_sort_by_key`, `par_stable_sort` and `par_stable_sort_by_key` to `sort`, `sort_by_key`, `par_sort` and `par_sort_by_key`, with unchanged contracts. The old names remain as `#[deprecated]` forwarding functions for the 0.2 series and are removed in a later breaking release. Revision benchmarks keep calling the old names until every compared baseline has the new ones, because both library copies must offer the called name. The rename is cheapest now, while 0.1 has just been published; the cost is one breaking minor release and updates to the guide, site, benchmark labels and semantics.

### Decisions

| Current functions | Mature precedent | Decision (names under rule 5) |
|---|---|---|
| `stable_select_nth`, `stable_select_nth_by_key` | `slice::select_nth_unstable(_by_key)` | Release as `select_nth` and `select_nth_by_key`, with unchanged arguments and borrows. |
| `stable_select_nth_many*` | NumPy `partition(kth=[...])` and `argpartition` | Release as `select_nth_many` and `select_nth_many_by_key`. Repeated single selections cannot replace it: after the first, each outer region holds the right members in unspecified order, so a later selection inside it no longer sees equal keys in input order. It is also the kernel of the binding's `partition` and `argpartition`, and of quantile-style requests. |
| `stable_sort_prefix*`, `stable_sort_rank_range*` | `slice::partial_sort_unstable(_by_key)` (nightly feature `slice_partial_sort_unstable`); C++ `std::partial_sort` | Replace both with `partial_sort(values, range)` and `partial_sort_by_key(values, range, key)`, where `range: impl RangeBounds<usize>`. Adopt the standard empty-range meaning: `k..k` with `0 < k < len` partitions at `k`, which `stable_sort_rank_range` currently treats as a no-op. If the standard method changes this meaning before stabilization, follow it before releasing the name. |
| `stable_top_k*` (and the new `k_largest*`) | itertools `k_smallest`, pandas `nsmallest`, JAX `lax.top_k` and PyTorch `topk` (default) return the selected records sorted; Polars `bottom_k` leaves them unordered | Release as `k_smallest(values, k)` and `k_smallest_by_key`, returning `(&mut [T], &mut [T])` with the first region sorted ascending, as most precedents do; `k_largest` and `k_largest_by_key` are the same call with a `Reverse` key, returning the largest records sorted descending as itertools `k_largest` does; equal keys still keep input order. Both are `partial_sort(values, ..k)` behind the interface users look for, including `k == 0` and `k == len`. Callers who need membership only use the documented `partial_sort(values, k..k)`. |
| `stable_partition` | C++ `std::stable_partition`; the count return matches `partition_point` | Release as `partition`. Its contract of one predicate call per record, in input order, remains. |
| `stable_partition_by_bucket`, `MAX_PARTITION_BUCKETS` | General-purpose sorting and grouping APIs (`slice::sort_by_key`, `chunk_by`, NumPy `unique`, pandas `groupby`) discover groups from the data. Only counting-scatter primitives take a count, for an externally fixed number of partitions (CUB histograms, Spark `partitionBy`) | Do not release. Its category count both bounds the input and sets the fan-out and the O(√(nR) + R) space, so the 4096 limit is the movement mechanism exposed as a parameter; at 257 categories its peak heap already exceeded the input ([baseline](evidence.md#stable-category-partition-baseline)). Group with `sort_by_key`, or with `sort_by_cached_key` for a costly classifier, then `slice::chunk_by`. `sort_by_cached_key` follows the standard contract: at most one key call per record. It caches every encoded key once, as the standard library does, then routes by the bits that vary among the cached keys rather than by key type: when one stable distribution covers them, as with category keys, the current category movement runs once by those bits and replays the cached keys; otherwise it sorts (key, index) pairs and permutes the records once. Ordered and constant inputs return after caching. Both routes cost O(n) auxiliary space, as in the standard library. The binary `partition` keeps its own movement path. |
| `stable_sort_segments*` | Rust `slice::chunks_mut`; CUB and rocPRIM segmented sort | Replace with `sort_chunks(values, chunk_len)` and `sort_chunks_by_key`, equivalent to sorting each `values.chunks_mut(chunk_len)` and panicking on a zero length like `chunks_mut`. One invocation reuses its workspace across chunks. Its concrete consumer is the binding's `axis=-1` sort of C-contiguous rows, which all have one length. Variable-length CSR offsets wait for a consumer that needs them. |
| `stable_argsort`, `stable_argsort_by_key` | NumPy `argsort(kind="stable")`, `torch.sort(stable=True)` | Release as `argsort` and `argsort_by_key`, returning `Vec<usize>`. The element width equals NumPy's `intp`, so the binding can hand the vector to NumPy without converting it. |
| `PrefixSorter` (proposed above) | None | Remains a proposal. With no precedent to constrain it, it needs the strongest use-case evidence. |

The Python binding follows NumPy names instead ([binding design](python-binding.md#first-release)). Its `sort` and `argsort` use the by-key entries with a float encoder, its later `partition` and `argpartition` use multiple selection, and its planned `axis=-1` uses chunk sorting. Every operation the binding needs therefore keeps a `_by_key` entry. The Rust `partition` keeps the Rust and C++ meaning; the NumPy `partition` is selection.

### Implementation consequences

- `partial_sort`, `k_smallest` and `k_largest` reuse the one rank requirement. A nonempty range is requested as today; an empty `k..k` inside the input maps to the singleton `k..k+1` that Top-k uses now; `0..0` and `len..len` are no-ops without key calls or allocation. No cut list or second request type is added.
- Range-bound conversion rejects inverted, out-of-bounds and overflowing bounds such as `Excluded(usize::MAX)` before mutation or key calls.
- Each released function's rustdoc opens with its delta from the standard operation; the semantics rows become those deltas. The crate documentation gains an operation overview and the membership-only `partial_sort(values, k..k)` recipe.
- Category grouping moves to `sort_by_cached_key`, accepting the [measured losses](evidence.md#cached-key-category-grouping) against the count-taking movement: with one classifier evaluation per record and no range given in advance, the keys are cached and the records move in a second pass. Faster `sort_by_key` on category keys is general engine work, not a release gate: without a known range it must scan for varying bits before choosing a digit.
- Tests and doctests follow the renamed functions; `sort_chunks` replaces offset validation with the `chunks_mut` rule. The engine, movement and recovery are unchanged.
- Benchmark workload names (`top-k`, `prefix`, `rank-range`, `segments`, `partition-buckets`) stay. A workload is an input and an expected result, not a function: `top-k`, `prefix` and `rank-range` all call `partial_sort`, so function names would not identify them, and renaming would invalidate retained plans and results. `partition-buckets` measures `sort_by_cached_key` as the `tessera` route, which keeps the required Tessera control for that workload.

## Derive the architecture from all known consumers

Start with the required results and lifetimes, setting existing file boundaries aside. Do not start from “add a prefix length to `Sorter`” and discover selection or session needs after that patch lands.

| Consumer | Necessary knowledge | Ownership and lifetime consequence |
|---|---|---|
| Full sort, sorted prefix and sorted rank range | Which ordered positions are required, and which completed bands contain them. | One ordered-refinement executor; an unrequested band still completes physical movement but needs no further key analysis. |
| Single and multiple selection | Exact ranked record identity, including equal-key members and positions within an equal band. | Share the same ordered analysis and rank projection; do not build a second selector or decorate every payload with its original index. |
| Stable binary and category partition | The supplied category of each original record, not its global key rank. | One complete stable-distribution operation with its own input contract; no order probe, insertion attempt or rank policy. |
| Argsort | Original indices are the payload and source records stay in place. | Sort the index output through by-key extraction; do not infer direct-key refill eligibility from the index type. |
| Segments and multiple fields | Several independent or successive complete operations; allocation reuse is possible only inside an invocation that owns them. | Segmented sorting reuses one private workspace across segments. Explicit public field-by-field sorts remain separate calls; no hidden cache or cross-call workspace is implied. |
| Groups and reductions | Complete grouping-key equality and already initialized records. | Compose after sorting or partitioning; consuming user callbacks do not enter the movement transaction. |
| Incremental prefix completion | Already established bands and progress must remain useful across public calls. | Separate persistent range/progress facts from transient partition ownership. Pause only after movement has retired and every record is back in the input. |

These workflows imply five knowledge owners, not five new public abstractions: key/order semantics, ordered-subproblem analysis, result-rank requirements, stable movement, and invocation/session resource lifetime. Scheduling chooses when exclusive work runs; it does not acquire a second interpretation of any of those rules.

A subproblem carries its exclusive record extent, remaining key candidates, required rank view and established facts. Root preparation, actual variation and completed classifications must not be discarded and reconstructed at a generic entry. Samples remain advisory. Candidate progress and exact bucket meaning stay with ordered analysis; neither the rank module nor storage derives them independently.

### Resource and failure boundaries

The workspace owner is one invocation or one borrowing session, not one worker. It lends tile storage and block buffers only through their complete operations, and reuses capacity after restoration. Segmented sorting must not allocate an independent workspace per segment merely because today's public full sort does. Explicit field-by-field public calls keep their current independent lifetimes; this composition may repeat allocation and scans, and does not claim batched reuse. Parallel pooling remains a scheduling mechanism within the invocation; retained and reentrantly suspended capacity stays charged to that owner. Do not merge buffers with different live-ownership protocols just to call them one allocator.

The same ownership chain applies to every algorithm: caller owns an initialized slice; a complete partition temporarily owns its live sources and restoration; retired final bands can be lent exclusively; after completion the input owns every record again. The executor may continue a band or retain its range facts, but cannot retain the partition's raw ownership state. A published band never returns to its parent's recovery set.

An incremental session adds persistence of facts, not a second restoration protocol. Its frontier stores disjoint unfinished ranges, candidates and established classifications, not a tree of live scratch-backed records. For monotone prefix growth, collapse finished prefixes and retain unvisited siblings along the active traversal; establish the frontier bound from fan-out and bit-consuming depth before implementation. No worker-count assumption replaces this accounting.

### Trace changes through the target boundaries

| Maintenance task | Single authority and consumers | Must not require coordinated reinterpretation in |
|---|---|---|
| Change sampling or equality/digit choice | Planner and its ordered executor consume the selected decision. | Separate sort/selection planners or parallel sampling code. |
| Change tile capacity or cursor layout | Tile storage derives capacities; planner and scheduler consume them. | Per-algorithm copies of byte thresholds or cursor limits. |
| Change block geometry or recovery | Stable movement owns representation, reserves and restoration. | Public algorithm adapters, rank requests or session metadata. |
| Add another rank request | Rank requirements normalize it and project borrowed child views. | Block classification, key encoding or task scheduling. |
| Reuse scratch across segments or pauses | Invocation/session owner governs capacity and complete-operation boundaries. | A second per-algorithm pool, hidden cross-call cache or cleanup protocol. |
| Change task size | Scheduler consumes the same decision and exclusive subproblem. | Another bucket interpretation, planner or progress proof. |

The structural stages are successful only if these real changes follow their owners. Moving functions between files is not, by itself, architectural acceptance.

## Current constraints and dependency changes

The [engine reference](engine.md) owns current boundaries and invariants. Today, block classification and the completion ledger are already more general than sorting. Ordered refinement now projects borrowed rank requirements before serial and parallel child continuation. Requested tiles retain their complete finishing operations; unrequested bands complete movement without further key analysis. The [engine reference](engine.md#ordered-rank-requirements) owns this implemented boundary. The ledger publishes every nonempty bucket's final parent-relative range directly from its start and length. The returned cumulative ends also include empty buckets; callback order does not define positions.

Foundations A and B remove the former upward dependencies of key/order primitives and tile leaf planning on recursive execution. Their current owners are documented in the [engine reference](engine.md#knowledge-boundaries). This separation lets later ordered and category operations use complete tile capabilities without importing full-sort control.

The intended responsibility dependencies are:

```text
public sorting, selection and composition entry points
    -> ordered refinement: node analysis + rank requirements
        -> complete bounded-tile operations or block partition

public category partition entry points
    -> stable distribution: validation + complete tile / block operations

invocation or borrowing session
    -> private workspace capacity + completed progress facts
       (no persistent live-record movement state)

both execution backends
    -> shared key/order semantics
    -> movement-local ownership and recovery
```

This is a responsibility graph, not a claim that all Rust modules form a strict import DAG. `key` and the SIMD backend cooperate on sealed key capabilities. Lower-level operations may accept completion callbacks, but must not import rank policy or reconstruct engine decisions.

The implementation paths in this table are relative to `src/engine/`; public algorithm adapters remain one directory above.

| Owner | Planned change | Knowledge that stays private |
|---|---|---|
| Existing scalar scan module, evolved into `order.rs`, and `key.rs` | Co-locate scalar order and candidate-masked variation scans with the existing insertion loops; keep encoded digit-window meaning with keys. Update extractor, SIMD, node and tile consumers to use these authorities without forwarding layers. | Engine eligibility, insertion budgets and root routing remain in the executor; once-per-pass SIMD dispatch remains in the backend. |
| `block.rs`, `block/finish.rs`, `block/parallel.rs` | Completed: [ledger range publication](engine.md#ledger-movement), retaining the serial classifier loan. | Block geometry, physical ownership, locking, free caches, live pointers and recovery. |
| `refine.rs` | Execute one ordered-subproblem analysis and continuation contract for all rank consumers; consume rank projections before child scans or finishing and preserve prepared facts. | Candidate-mask progress, completed equal bands, run rotation and child routing. |
| Private rank-requirement module | Own one normalized representation and child projection for full intervals, prefixes, rank ranges and selected positions from the first structural design. | Bounds, implied cuts and request normalization; no key reads, record movement or scheduling. |
| Private invocation/session workspace, evolving the existing `Scratch` owner | Lend existing tile and block storage through complete operations; permit reuse across segments and eventual public pauses without changing independent calls into a shared session. | Capacity lifetime, outstanding restoration obligations and all retained or suspended allocations; no public scratch assembly. |
| Existing node planner in `refine/plan.rs` | Provide the same executable node decisions to all ordered operations; consume tile-owned capacity facts rather than owning its leaf policy. | Node sampling and partition thresholds; no second selection planner. |
| Bounded tile subsystem, independent of `refine` | Own complete tile finishing together with its existing leaf planner, cursor-derived capacities and restoration. Later supply complete bounded category distribution to the partition adapter through the same storage owner. | Short LSD plans, tile MSD and insertion rules, cursors, cached IDs, alternating live buffers and the guard. No import of recursive node execution, and no raw counting-pass assembly outside the subsystem. |
| `refine/parallel.rs` | Migrate the existing callback and primitive dependencies without changing full-sort scheduling. | Task admission, scratch pooling and suspended-allocation accounting. |
| `distribution.rs` ([implemented boundary](engine.md#category-distribution)) | Own validation, small/large execution choice, scratch lifetime and boundary output as one operation, rather than merely forwarding `block::partition`. | No public block or classifier trait, and no sorting preparation for arbitrary categories. |

Evolve existing owners rather than adding a second planner, allocator or pool. A rank-requirement owner and an operation/session workspace are justified by several concrete consumers above, not speculative extensibility. Move the existing leaf decisions with tile ownership, not into a duplicate planner or a generic storage interface. Node and tile analysis deliberately remain different complete operations: large children benefit from order confirmation; insertion-sized tile children skip scans. Preserve the [N5 boundary and LSD savings](evidence.md#n5-shared-node-and-tile-recursion), rather than imposing one recursion protocol.

Moving shared primitives and tile ownership removes dependency on the full-sort executor; it is not the whole refactor. Do not change dispatch context, scan order or insertion work while doing so. Keep `Extract::DIRECT` as the sole authority for direct-key recomputation and count/refill eligibility. Argsort indices and category IDs do not identify interchangeable payloads.

## Ordered refinement without a general policy framework

Use required ordered rank positions as the common logical representation: a normalized collection of disjoint intervals. Full sorting requests `[0,n)`, a sorted prefix `[0,k)`, a rank range `[a,b)`, and selection index i the singleton interval `[i,i+1)`. Multiple selected positions become singleton intervals, merging adjacent ones where appropriate. Each nonempty interval implies isolation of its endpoint ranks. Do not maintain a second explicit cut list that must agree with those endpoints; Top-k uses the same single-rank refinement behind its count-and-split interface, as specified by the [engine reference](engine.md#ordered-rank-requirements).

The rank module owns this meaning from the beginning, rather than starting with a prefix-specific executor and replacing it when another algorithm arrives. Its projection answers whether a child is irrelevant, entirely required or still needs partial refinement, using exact parent-relative ranges and a rank base. It does not reinterpret keys or planner decisions. Single intervals must need no heap allocation; multiple requests normalize at most once and recursion borrows views instead of building vectors per child. An inline interval versus a borrowed normalized list is an internal representation choice, not a different algorithm policy.

Settle this contract and route full sorting through it in the structural adaptation. Implement additional public request constructors and algorithm entry points only with their tests; do not export stubs or add unimplemented runtime branches for future stages. The design is global, while delivery remains incremental. No per-record policy dispatch, public plugin trait or separately maintained selection interpreter is needed.

For each child, decide whether work is required before scanning its keys. Unrelated children receive no further analysis; fully requested children use the existing sort; partial children continue with their local request. Completed equality bands satisfy internal ranks through their preserved order. Preserve the four-bucket `Rank` classifier and bit-consuming progress for equality, digit and run splits, including inconsistent extractors. A smaller requested result does not replace the candidate-bit termination proof.

Requested tiles are fully sorted. Retain insertion, tile MSD, short LSD plans and the all-small-bucket shortcut; optimizing tile-local selection is a separate measured change. Root preparation and covering direct-key counting may also finish more than requested. A first tile-local policy, which took a top-digit MSD pass for partial requests and finished only the requested bands, measured slower on every partial operation in the continuous suite (random 65,536-record u64: select-many +59%, rank range +38%, select, prefix and Top-k +19–20%; row_u64 +6–24%), with argsort, chunks and partitions unchanged. It was not merged; a replacement must beat complete tile sorting on those cases.

Skipping suffix refinement does not skip parent movement: every band must finish placement and restore ownership. Preserve serial cache-hot small-child completion, and recurse into large children after parent partition completion so scratch can be reused. Do not move all recursion into ledger callbacks or admit recursive parallel work. Incremental completion later retains only unfinished ranges and progress facts across calls, collapsing completed prefixes instead of keeping an unrestricted partition tree.

## Alternatives and code-size controls

| Construction | Assessment |
|---|---|
| Full stable sort, then inspect the desired positions | Correctness reference and fallback; requires no new engine but cannot demonstrate pruning savings. |
| Separate sorting and selection engines | Small initial patch, but duplicates sampling, bucket meaning, progress and child analysis. Do not choose it. |
| General `Algorithm` / `Policy` traits or a public request builder | Adds lifecycle obligations, mode combinations and generic instantiations before concrete consumers justify them. Do not choose it. |
| Prefix-specific execution first, generalize after adding more algorithms | Reject as the target design: the known selection and session consumers already reveal the required ownership and request boundaries. A smaller first patch does not justify settling a local architecture. |
| Shared complete movement operations, globally designed rank requirements and invocation/session ownership | Chosen direction. It removes the full-sort assumption from control and lifetime boundaries without duplicating movement or recovery. Implement consumers incrementally inside this settled structure. |

Keep existing implementations rather than cloning them for new names. Add modules only for a complete independently changing responsibility; do not create wrappers to make the diagram look layered. Avoid redundant convenience APIs, per-record original-position decoration, persistent task trees and new allocation pools. Tests should extend existing contract and recovery coverage where possible, not build a parallel test framework. If a stage needs a second planner, wider scratch residency, a dependency or platform change, stop and revise this proposal before broadening it.

## Implementation sequence

The [roadmap](../../ROADMAP.md#algorithm-extension-experiment) owns progress and acceptance status. This order moves from existing-operation compositions to new one-shot algorithms and finally persistent state. It does not require unrelated existing acceptance debt to be resolved first.

| Stage | Complete deliverable | Separation and acceptance |
|---|---|---|
| Foundation A | Shared order primitives: [implemented boundary](engine.md#insertion-and-scans). | Retain existing scan, dispatch, routing and allocation behavior. Validate current serial and Rayon entry points. |
| Foundation B | Tile-owned leaf analysis and storage: [implemented boundary](engine.md#knowledge-boundaries). Encoded digits and candidate scans live at their lower-level owners. | Relocate existing implementations without wrapper layers or rule changes. Preserve LSD alternation, scan-free small leaves and order-aware large children; validate the existing tile and node consumers. |
| Foundation C | Final band ranges: [implemented boundary](engine.md#ledger-movement), including serial, parallel, recovery and trace consumers. | Preserve movement, scans, allocations and scheduling. Check ranges against independent bucket counts, including empty buckets and concurrent completion. |
| Foundation D | Shared [rank requirements, ordered execution and invocation workspace](engine.md#ordered-rank-requirements); existing full sorting is the first complete consumer. | No prefix-only execution branch or second engine. Preserve root and child facts, tile fast paths, serial scratch reuse and current parallel admission. Demonstrate the target change paths above before adding public algorithms. |
| 1 | Stable argsort; documented multi-field and grouping compositions. | Use existing sorting. Verify index identity and ties; no new movement kernel or custom grouping engine. |
| 2 | Serial segmented sorting with private scratch reuse. | Validate all offsets before writes. Finish and restore one segment before reusing storage for the next; account for retained capacity. |
| 3 | Stable binary partition, then bounded multiway partition. | Separate complete slices. Cover arbitrary payloads, classifier failure, small and zero-sized inputs, invalid categories and full boundary output. |
| 4 | Stable sorted prefixes and unordered Top-k. | Use the shared node executor and fully sorted requested tiles. Zero and full-length requests follow their [implemented contracts](../../conformance/semantics.md#serial-slice-operations); no persistent state. |
| 5 | Stable single selection, then sorted rank ranges. | Extend the same request authority; verify ranked record identity and both outer memberships, not just selected key values. |
| 6 | Multiple-rank selection. | Normalize once and share early partitions; no independent selection loop per target and no per-child request-vector allocation. |
| 7 | Incremental prefix completion. | First establish borrowing, panic invalidation and frontier-space invariants; then reuse one-shot analysis. Do not add insertion, record-removing iteration or PQ operations. |

The sequence is an implementation order, not permission to postpone the target dependency design until each feature arrives. A public function is exported only with its complete implementation, rustdoc and tests. Keep structural changes separate from algorithm, routing and capacity changes. After each stage, remove temporary entry points and update the owning current documents; replace implemented proposal sections with links rather than keeping a second contract.

## Benchmark workloads and the existing tools CLI

The first one-shot operations use the maintained [operation comparison contract](../development/benchmarking.md#serial-operation-comparisons) and [shared kernel boundary](benchmark-tooling.md#operation-workloads). Public algorithms, not private study entries, are timed end to end. Current contracts belong there; no second command family or experimental library feature is needed.

Category workloads use deterministic encoded-key categories, complete boundary and stable-member verification, and explicitly labelled sorting-based references in the maintained operation pipeline. A sorting composition is not a direct output-buffer partition baseline. A future incremental session needs a recorded request sequence and verification of each published prefix, reporting both whole-session cost and extension latency without replacing the persistent workload with independent Top-k calls. Admit those shapes through the same capabilities, provenance, allocation and recovery boundaries before measuring them. Keep new figures deferred unless separately requested; preserve the existing full-sort figures.

## Validation and evidence

Follow [engine validation](../../CONTRIBUTING.md#validation), retaining current owned-record, mutable-reference, inconsistent-key, panic and allocation coverage. Compare successful partial results against an independent complete stable sort that retains record identities. Cover boundary ties, ordered and reversed input, skew, narrow and wide keys, and the key-encoding consequences relevant to each new path. Run affected serial and Rayon tests, Miri and figure checks; report unavailable checks rather than treating them as passed.

Measure structural stages against their immediate baseline with the maintained [source-attribution controls](../development/benchmarking.md#additional-source-attribution-controls), starting with a bounded representative selection. Existing full-sort regressions remain relevant even when a new algorithm saves work. Preserve the figures; update the routing explanation and any affected model when behavior changes.

New-operation measurements must verify equivalent outputs and include prefix/rank density, duplicates at boundaries, record width, extractor cost and peak allocation as appropriate. An unstable selection is not an equivalent stable-member baseline without tie handling. Use complete stable sorting as a semantic reference; report cheaper non-equivalent baselines explicitly.

Do not assume the current benchmark CLI measures unimplemented operations. Add only bounded support to the maintained pipeline when a stage needs it; keep verification, provenance, allocation accounting and reporting shared. A larger benchmark redesign needs a separate scope decision. Raw measurements, commands and experiment chronology stay outside Git. Functional implementation may be retained with explicitly pending performance acceptance, but it supplies no performance claim.

The proposal itself has no runtime or performance evidence. Upon implementation, public guarantees move to [semantics](../../conformance/semantics.md), boundaries and recovery to the [engine reference](engine.md), routing to the [explanation](../explain/routing.md), and rejected constructions or accepted losses to [engine evidence](evidence.md).

## Further engine capabilities

The private engine boundary serves two current operations: ordered refinement and complete stable category distribution. Future algorithms should consume whichever facts their output needs; full materialization is not a universal intermediate. Keep refinements private until real callers have established their contracts.

- **Exact key facts.** Cached sorting already computes order and varying bits while evaluating each key once. An advanced engine entry should consume those proven facts without rescanning the cache, while retaining descending and budgeted insertion completion. Prefer a typed summary owned alongside the encoded keys over independent masks or flags whose provenance callers must pair. Do not let an advisory sample stand for exact variation.
- **Full-key counts.** Separate the ownership-free `KeyCounts` summary from filling destination runs when a counts/unique consumer exists. A bucket denotes a complete key only when all outside bits are proven equal; radix-prefix counts alone do not identify an equal-key group. Serial and parallel filling should consume the same summary, and user callbacks must run after movement restoration.
- **Read-only small results.** A kth value or k indices can use prefix counts, then collect bounded candidates into tile storage. Size candidates by their encoded-key/index representation. Do not build n indices solely for interface uniformity; include repeated extraction, scans, output and candidate storage in its eventual cost model. Multiple requested quantiles need a separate bound on active-prefix state.

These capabilities do not require a generic strategy trait, a persistent session or public block/scratch APIs. Start with one real consumer per capability and retire redundant scans or intermediates with it. Composite keys, weighted statistics and persistent prefix sessions retain their own semantics and remain separate extensions.
