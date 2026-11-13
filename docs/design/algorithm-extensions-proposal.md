# Algorithm extensions proposal

Status: experiment in progress; new public interfaces are not implemented. This page specifies candidate Rust interfaces, internal dependency changes and implementation order. Implemented boundaries belong in the [engine reference](engine.md); the [roadmap](../../ROADMAP.md#algorithm-extension-experiment) tracks acceptance. It does not add public exports or change the current [semantics](../../conformance/semantics.md). Unimplemented interfaces stay in this document, not as exported stubs or `todo!()` functions.

## Scope and direction

Extend Tessera through complete operations for stable distribution and ordered refinement, not a public algorithm framework. Stable distribution groups records by category while preserving input order within each category. Ordered refinement additionally knows that buckets occupy ordered rank intervals, and continues only where a requested result needs more work. A category is not necessarily a key prefix or a rank interval.

Design the target architecture as if the known sorting, selection, partitioning, batch and incremental consumers were required from day zero. Their result contracts, ownership and resource lifetimes determine the boundaries; the current `Sorter` and the first feature do not. Settle that target before migrating the implementation, then deliver independently verifiable slices from simple compositions to stateful algorithms. External sorting and Wavelet matrix construction receive neither an interface nor an implementation in this experiment. Their known roles remain compatible with the boundary: run generation consumes complete sorting, while Wavelet construction needs controlled stable passes, not rank refinement. A priority queue is outside this proposal.

The first implementation sequence covers argsort, standard-library compositions for multiple fields and groups, segmented sorting, stable binary and bounded multiway partitioning, sorted prefixes, rank selection and sorted rank ranges. Incremental prefix completion follows only after the one-shot operations work. This is an experiment, not a commitment to include every interface in Rust 0.1.

## Proposed Rust interfaces

Keep the existing `stable_sort` and `stable_sort_by_key` unchanged. New functions use slices, `usize` lengths and indices, `Range<usize>` where appropriate, and ordinary closures. In-place operations return `()` unless they naturally return a boundary or the standard library's selection tuple. Do not introduce public planners, workspaces, policies, bucket descriptors or result handles for one-shot calls.

Names retain the crate's explicit `stable_` convention. Selection follows the argument and return shape of slice `select_nth_unstable`, but has the stable ranked-member contract below. These are proposed signatures, not implemented API:

```rust
pub fn stable_argsort<K: RadixKey>(values: &[K]) -> Vec<usize>;
pub fn stable_argsort_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &[T],
    key: F,
) -> Vec<usize>;

pub fn stable_sort_segments<K: RadixKey>(values: &mut [K], offsets: &[usize]);
pub fn stable_sort_segments_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    offsets: &[usize],
    key: F,
);

pub fn stable_partition<T, F: FnMut(&T) -> bool>(
    values: &mut [T],
    predicate: F,
) -> usize;
pub fn stable_partition_by_bucket<T, F: FnMut(&T) -> usize>(
    values: &mut [T],
    bucket_count: usize,
    classify: F,
) -> Vec<usize>;

pub fn stable_sort_prefix<K: RadixKey>(values: &mut [K], prefix_len: usize);
pub fn stable_sort_prefix_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    prefix_len: usize,
    key: F,
);

pub fn stable_select_nth<K: RadixKey>(
    values: &mut [K],
    index: usize,
) -> (&mut [K], &mut K, &mut [K]);
pub fn stable_select_nth_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    index: usize,
    key: F,
) -> (&mut [T], &mut T, &mut [T]);

pub fn stable_sort_rank_range<K: RadixKey>(values: &mut [K], ranks: Range<usize>);
pub fn stable_sort_rank_range_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    ranks: Range<usize>,
    key: F,
);

pub fn stable_select_nth_many<K: RadixKey>(values: &mut [K], indices: &[usize]);
pub fn stable_select_nth_many_by_key<T, K: RadixKey, F: FnMut(&T) -> K>(
    values: &mut [T],
    indices: &[usize],
    key: F,
);
```

`Range` above is `core::ops::Range`; `RadixKey` is the existing sealed key trait. Do not change its supported types, the MSRV or dependencies as part of these interfaces. No parallel extension API is proposed yet; existing Rayon full-sort calls remain supported throughout the refactor.

### Shared behavior

Ordering and key extraction follow the existing [key contract](../../conformance/semantics.md#key-extraction), including encoded float order and `Reverse`. New by-key calls retain `FnMut` and the current freedom to evaluate keys repeatedly. Success conditions below assume consistent keys. Changing keys may invalidate ordering, but must not invalidate record ownership or bounded progress. A key or classifier panic restores every input record before unwinding, in unspecified order; it does not promise a valid partial result.

Validate lengths, indices, ranges, segment offsets and bucket-count limits before mutation or callback evaluation. Invalid arguments panic, as with slice indexing and selection. A classifier returning an invalid bucket is detected when that record is classified, under recovery protection; do not add a preliminary classification pass just to validate all results. The library does not roll back user callback side effects.

The proposed partition functions evaluate the predicate or classifier once per record on success. Serial classification follows input order. They do not reuse sorting probes or key fast paths. Tile execution caches those category decisions; block execution already classifies each input record once per pass. Empty and zero-sized inputs still produce the defined counts and boundaries.

### Result contracts

Let `S` be a conceptual complete stable sort of the original records. Record identity matters, not merely equality of keys. No implementation is required to materialize `S` or attach original indices to moved records.

| Operation | Successful result and boundaries | Additional output or metadata |
|---|---|---|
| Argsort | Return a permutation of `0..values.len()` in stable encoded-key order; do not reorder the source. | The index output is O(n), separate from sorting scratch. |
| Segmented sort | Sort each segment independently and stably. Offsets must be nondecreasing, start at zero and end at the slice length; repeated offsets permit empty segments. An empty slice accepts `[0]`. | Offsets are borrowed. Reuse private scratch across segments. |
| Binary partition | Return the number of matching records. Matching records precede nonmatching records; both groups retain their original order. | One boundary. |
| Multiway partition | Categories are `0..bucket_count` in ascending category order, stable within each category. Return `bucket_count + 1` offsets, starting at zero and ending at the input length, including empty categories. | O(R) output for R categories; R also affects scratch. |
| Sorted prefix | `values[..prefix_len]` equals `S[..prefix_len]`. The suffix contains exactly the other records, in unspecified order. Accept `0..=len`; zero is a no-op without key calls or allocation. | No result allocation. |
| Single selection | The middle record equals `S[index]`; the preceding slice contains exactly the members of `S[..index]`, and the following slice the remaining members. Neither side needs internal order. Require `index < len`. | The three mutable borrows are disjoint, as in standard-library selection. |
| Sorted rank range | For a nonempty range, `values[ranks.clone()]` equals `S[ranks.clone()]`, and the prefix and suffix contain their corresponding ranked members without internal ordering guarantees. Require `start <= end <= len`. An empty range is a no-op and does not establish an outer rank boundary. | No result allocation. These are sorted ranks, not original positions; sorting a physical subslice remains ordinary slice composition. |
| Multiple selection | Each requested position holds `S[index]`; regions before, between and after requested ranks contain their corresponding ranked members without internal ordering guarantees. Accept unordered and duplicate indices; validate all, normalize once, and borrow views during recursion. An empty request is a no-op. | At most O(m) request metadata for m indices, not a vector of record references. |

Stable selection means stable membership and ranked identity. It does not mean that all selected records of different keys retain their original input order. For an unsorted minimum-k set, use the prefix before index k from single selection when `0 < k < len`; handle the empty and full set directly. Do not add a second top-k wrapper with the same semantics. Statistical quantile definitions and interpolation stay outside the engine; multiple selection accepts exact integer ranks.

Multiway partitioning requires `bucket_count > 0` and is initially bounded by the block representation's existing fan-out limit. The implementation must document the supported maximum before exporting the API. Positive counts need not be powers of two: internal padding must not expose extra categories or silently mask invalid IDs. A single category needs no record movement but still evaluates the classifier. Do not advertise arbitrary category counts under the sort's fixed-radix space bound.

### Capabilities that need no new public framework

Lexicographic multiple-field sorting initially uses repeated stable sorts from the least significant field to the most significant field. Tuple keys and variable-width encodings are not implied by the current `RadixKey` contract. Group traversal initially uses a completed sort and slice `chunk_by`; equality must match the chosen normalized or encoded grouping key, not merely a digit. Standard iterators and `Vec` operations supply counting, reduction and deduplication where appropriate. Their consuming, destructor and callback-panic contracts are separate from sorting's permutation guarantee.

Incremental completion eventually needs a borrowing object, not an iterator that immediately moves records out. The candidate shape is `PrefixSorter::by_key(&mut values, key)` followed by `ensure_prefix(prefix_len) -> &[T]`. Smaller requests may reuse completed work; extending a prefix requires an exclusive borrow of the object. Names and exact generics remain provisional until its lifecycle stage. The key must remain consistent for the whole session; a panic invalidates further use of that session while restoring its records. No half-finished movement or live records may remain in scratch between public calls.

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

The [engine reference](engine.md) owns current boundaries and invariants. Today, block classification and the completion ledger are already more general than sorting. The full-sort assumption lives in `Split::partition`, large-child continuation, tile finishing and the parallel root callback: each continues all unfinished children. The ledger already knows every bucket's final start and length before publishing any band.

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

| Owner | Planned change | Knowledge that stays private |
|---|---|---|
| Existing scalar scan module, evolved into `order.rs`, and `key.rs` | Co-locate scalar order and candidate-masked variation scans with the existing insertion loops; keep encoded digit-window meaning with keys. Update extractor, SIMD, node and tile consumers to use these authorities without forwarding layers. | Engine eligibility, insertion budgets and root routing remain in the executor; once-per-pass SIMD dispatch remains in the backend. |
| `block.rs`, `block/finish.rs`, `block/parallel.rs` | Publish `(bucket, range_in_parent, child)` through the internal completion callback; retain the classifier loan in the serial form. Derive the range from ledger-owned facts. | Block geometry, physical ownership, locking, free caches, live pointers and recovery. |
| `engine.rs` | Execute one ordered-subproblem analysis and continuation contract for all rank consumers; consume rank projections before child scans or finishing and preserve prepared facts. | Candidate-mask progress, completed equal bands, run rotation and child routing. |
| Private rank-requirement module | Own one normalized representation and child projection for full intervals, prefixes, rank ranges and selected positions from the first structural design. | Bounds, implied cuts and request normalization; no key reads, record movement or scheduling. |
| Private invocation/session workspace, evolving the existing `Scratch` owner | Lend existing tile and block storage through complete operations; permit reuse across segments and eventual public pauses without changing independent calls into a shared session. | Capacity lifetime, outstanding restoration obligations and all retained or suspended allocations; no public scratch assembly. |
| Existing node planner in `engine/plan.rs` | Provide the same executable node decisions to all ordered operations; consume tile-owned capacity facts rather than owning its leaf policy. | Node sampling and partition thresholds; no second selection planner. |
| Bounded tile subsystem, moved out of `engine/` | Own complete tile finishing together with its existing leaf planner, cursor-derived capacities and restoration. Later supply complete bounded category distribution to the partition adapter through the same storage owner. | Short LSD plans, tile MSD and insertion rules, cursors, cached IDs, alternating live buffers and the guard. No import of recursive node execution, and no raw counting-pass assembly outside the subsystem. |
| `engine/parallel.rs` | Migrate the existing callback and primitive dependencies without changing full-sort scheduling. | Task admission, scratch pooling and suspended-allocation accounting. |
| Future public partition implementation | Own validation, small/large execution choice, scratch lifetime and boundary output as one operation, rather than merely forwarding `block::partition`. | No public block or classifier trait, and no sorting preparation for arbitrary categories. |

Evolve existing owners rather than adding a second planner, allocator or pool. A rank-requirement owner and an operation/session workspace are justified by several concrete consumers above, not speculative extensibility. Move the existing leaf decisions with tile ownership, not into a duplicate planner or a generic storage interface. Node and tile analysis deliberately remain different complete operations: large children benefit from order confirmation; insertion-sized tile children skip scans. Preserve the [N5 boundary and LSD savings](evidence.md#n5-shared-node-and-tile-recursion), rather than imposing one recursion protocol.

Moving shared primitives and tile ownership removes dependency on the full-sort executor; it is not the whole refactor. Do not change dispatch context, scan order or insertion work while doing so. Keep `Extract::DIRECT` as the sole authority for direct-key recomputation and count/refill eligibility. Argsort indices and category IDs do not identify interchangeable payloads.

## Ordered refinement without a general policy framework

Use required ordered rank positions as the common logical representation: a normalized collection of disjoint intervals. Full sorting requests `[0,n)`, a sorted prefix `[0,k)`, a rank range `[a,b)`, and selection index i the singleton interval `[i,i+1)`. Multiple selected positions become singleton intervals, merging adjacent ones where appropriate. Each nonempty interval implies isolation of its endpoint ranks. Do not maintain a second explicit cut list that must agree with those endpoints; unsorted top-k uses the single-selection composition already specified.

The rank module owns this meaning from the beginning, rather than starting with a prefix-specific executor and replacing it when another algorithm arrives. Its projection answers whether a child is irrelevant, entirely required or still needs partial refinement, using exact parent-relative ranges and a rank base. It does not reinterpret keys or planner decisions. Single intervals must need no heap allocation; multiple requests normalize at most once and recursion borrows views instead of building vectors per child. An inline interval versus a borrowed normalized list is an internal representation choice, not a different algorithm policy.

Settle this contract and route full sorting through it in the structural adaptation. Implement additional public request constructors and algorithm entry points only with their tests; do not export stubs or add unimplemented runtime branches for future stages. The design is global, while delivery remains incremental. No per-record policy dispatch, public plugin trait or separately maintained selection interpreter is needed.

For each child, decide whether work is required before scanning its keys. Unrelated children receive no further analysis; fully requested children use the existing sort; partial children continue with their local request. Completed equality bands satisfy internal ranks through their preserved order. Preserve the four-bucket `Rank` classifier and bit-consuming progress for equality, digit and run splits, including inconsistent extractors. A smaller requested result does not replace the candidate-bit termination proof.

Initially, any requested tile is fully sorted. Retain insertion, tile MSD, short LSD plans and the all-small-bucket shortcut; optimizing tile-local selection is a separate measured change. Root preparation and covering direct-key counting may also finish more than requested. Do not change those policies in the structural stages.

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
| Foundation C | Provide final band ranges and migrate every serial, parallel, recovery and trace consumer. | Another structural commit: no new public algorithm, scan, allocation policy or scheduling rule. Check callback ranges against independent cumulative bucket lengths, including sparse buckets. |
| Foundation D | Establish the common rank-requirement authority, ordered execution boundary and private workspace handoff; migrate existing full sorting as the first complete consumer. | No prefix-only execution branch or second engine. Preserve root and child facts, tile fast paths, serial scratch reuse and current parallel admission. Demonstrate the target change paths above before adding public algorithms. |
| 1 | Stable argsort; documented multi-field and grouping compositions. | Use existing sorting. Verify index identity and ties; no new movement kernel or custom grouping engine. |
| 2 | Serial segmented sorting with private scratch reuse. | Validate all offsets before writes. Finish and restore one segment before reusing storage for the next; account for retained capacity. |
| 3 | Stable binary partition, then bounded multiway partition. | Separate complete slices. Cover arbitrary payloads, classifier failure, small and zero-sized inputs, invalid categories and full boundary output. |
| 4 | Stable sorted prefixes. | Use the shared node executor and fully sorted requested tiles. Zero and full-length requests follow their proposed contracts; no persistent state. |
| 5 | Stable single selection, then sorted rank ranges. | Extend the same request authority; verify ranked record identity and both outer memberships, not just selected key values. |
| 6 | Multiple-rank selection. | Normalize once and share early partitions; no independent selection loop per target and no per-child request-vector allocation. |
| 7 | Incremental prefix completion. | First establish borrowing, panic invalidation and frontier-space invariants; then reuse one-shot analysis. Do not add insertion, record-removing iteration or PQ operations. |

The sequence is an implementation order, not permission to postpone the target dependency design until each feature arrives. A public function is exported only with its complete implementation, rustdoc and tests. Keep structural changes separate from algorithm, routing and capacity changes. After each stage, remove temporary entry points and update the owning current documents; replace implemented proposal sections with links rather than keeping a second contract.

## Benchmark workloads and the existing tools CLI

Extend the existing `bun run tools` pipeline, not a second benchmark executable or an algorithm-specific command family. Its current input generation, `Variant::prepare` capability boundary, restorable `Kernel`, process-isolated allocation measurement, retained builds, journals and offline statistics remain useful. The assumptions to remove are that every case means a full sort, every output is a reordered input buffer, and exact full-reference equality is always the right checker.

A workload is an operation with validated parameters plus an input case. An implementation identity remains an implementation identity: do not encode argsort or Top-k by inventing sorting-algorithm names or abusing the serial/parallel mode. Resolve parameters once into the prepared inventory; include them in grouping, sample identity, memory evidence, resume validation and report labels. Requested rank fractions become exact ranks before any adapter runs, with their rounding rule recorded. Do not compare or aggregate ratios across different result contracts.

| Operation | Parameter axes | Verification and equivalent baseline |
|---|---|---|
| Full sort | Existing input and mode axes. | Current exact stable-reference or unstable permutation checker. Preserve existing measurements and figures. |
| Argsort | Input representation and distribution; the returned index vector is part of the operation. | Verify an exact stable index permutation against source keys. Compare with index construction plus standard stable index sorting, not direct record sorting. |
| Segments | Validated segment offsets or a reproducible segment-length specification. | Reference-sort each segment independently. Include public-call allocation and initialization; do not time only one isolated segment as a batch API. |
| Binary / multiway partition | A deterministic classifier and category count, with a reproducible category distribution. | Verify complete category boundaries and stable member identity. Use a stable reference distribution; distinguish sort-based and output-buffer baselines by their full costs. |
| Prefix / rank range | Prefix length or nonempty rank interval, including boundary-heavy ties. | Compare required positions and all outer memberships with complete stable sorting. Charge the full public operation, not only the final prefix sort. |
| Single / multiple selection | Exact target indices or a normalized index specification. | Verify selected record identities and memberships of all intervening regions. Unstable selection is a separately labelled weaker-contract comparison, not the stable anchor. |
| Incremental prefix | A recorded sequence of increasing requests. | Verify every published prefix. Report whole-session cost and individual extension latency; do not substitute repeated fresh Top-k calls or mix persistent setup with steady-state samples. |

`Kernel::measure_batch` remains the common clock boundary, but its documented meaning becomes operation execution, not sorting. Each prepared kernel owns the required output shape, restoration and checker. An argsort kernel may share immutable source records across repetitions but must create the public result inside the timed call, retain it through verification and account for its memory; resetting and destroying a previous output happen outside the timed region. A separate allocation process reports peak call allocation, with mandatory output bytes explicitly distinguished from extra workspace. Partition and partial-order checks must inspect identities, not just key values or locally sorted windows.

`Variant::prepare` remains the authority for supported operation/input/implementation combinations. Reuse typed record semantics and generation; do not duplicate support matrices in TypeScript, the catalog and worker code. Preparation returns the concrete inventory and reasons unsupported selections were omitted or rejected. Required controls must implement the same operation; an unavailable old-revision API is an explicit capability limitation, never silent fallback to complete sorting. Hosted full-sort tracking stays unchanged until an operation has local verification and evidence support.

Keep task-oriented commands: `plan` previews selection, `compare algorithms` compares implementations of one operation, `compare revisions` compares sources implementing the same operation, and `report` regenerates retained data. Add an operation selector and only the relevant validated parameters when the first new kernel exists. Default existing invocations to full sorting. Show the selected operation, resolved parameters, valid implementations and experiment size in preview output; fail incompatible parameter combinations before building. No interactive wizard is required for iteration. Rework command spelling only if this complete workflow cannot be expressed clearly; the old sorting-only parser is not the design authority.

Persisted formats need an explicit version and workload identity before non-sort samples are admitted. Continue reading historical sorting results as sorting without changing their IDs or figures; never infer a new operation from an algorithm label or report text. Admit the first new operation end to end (plan, preparation, timing, verification, memory, resume and offline report) before adding more axes. This benchmark slice follows the engine foundations and accompanies the first new public operation.

## Validation and evidence

Follow [engine validation](../../CONTRIBUTING.md#validation), retaining current owned-record, mutable-reference, inconsistent-key, panic and allocation coverage. Compare successful partial results against an independent complete stable sort that retains record identities. Cover boundary ties, ordered and reversed input, skew, narrow and wide keys, and the key-encoding consequences relevant to each new path. Run affected serial and Rayon tests, Miri and figure checks; report unavailable checks rather than treating them as passed.

Measure structural stages against their immediate baseline with the maintained [source-attribution controls](../development/benchmarking.md#additional-source-attribution-controls), starting with a bounded representative selection. Existing full-sort regressions remain relevant even when a new algorithm saves work. Preserve the figures; update the routing explanation and any affected model when behavior changes.

New-operation measurements must verify equivalent outputs and include prefix/rank density, duplicates at boundaries, record width, extractor cost and peak allocation as appropriate. An unstable selection is not an equivalent stable-member baseline without tie handling. Use complete stable sorting as a semantic reference; report cheaper non-equivalent baselines explicitly.

Do not assume the current benchmark CLI measures unimplemented operations. Add only bounded support to the maintained pipeline when a stage needs it; keep verification, provenance, allocation accounting and reporting shared. A larger benchmark redesign needs a separate scope decision. Raw measurements, commands and experiment chronology stay outside Git. Functional implementation may be retained on the experimental branch with explicitly pending performance acceptance, but it supplies no performance claim.

The proposal itself has no runtime or performance evidence. Upon implementation, public guarantees move to [semantics](../../conformance/semantics.md), boundaries and recovery to the [engine reference](engine.md), routing to the [explanation](../explain/routing.md), and rejected constructions or accepted losses to [engine evidence](evidence.md).
