# Engine reference

This document describes the engine's boundaries and invariants in `rust/tessera-sort`. Routing rules live on the [explanation pages](../explain/index.md), and [engine evidence](evidence.md) keeps failed experiments and open measured losses.

## Vocabulary

| Term | Meaning |
|---|---|
| Key | The unsigned integer a record sorts by. Signed integers, floats and `Reverse` keys are encoded into unsigned keys whose integer order matches the requested order. |
| Digit | A window of key bits handled by one pass, described by a shift and a width. |
| Subproblem | A record slice and a candidate mask of key bits still to be ordered. A completed digit leaves that mask even if a key function changes its answers. |
| Bucket | A logical, stable classification group. It may span physical blocks; after materialization it is a contiguous subproblem, not necessarily a task or a tile. |
| Block | A physical storage and ownership-transfer unit used by the stable partition. Its size and directory costs belong to the block mechanism. |
| Node | A subproblem larger than the tile capacity. Nodes are split by a stable block partition or existing ordered runs. |
| Tile | A subproblem that fits a resource-bounded record capacity. The byte budget is converted once to a record count; routing compares lengths rather than reinterpreting cursor limits or byte products. [Tiles](../explain/tiles.md) defines the budget and limits. Tiles are finished in scratch memory. |
| Cursor table | One `u16` position per bucket; its size is bounded by the tile budget. |

## Knowledge boundaries

| Owner | Complete responsibility | Consumers need to know |
|---|---|---|
| `engine.rs` | Root preparation, candidate-bit progress, recursive node execution. Serial and parallel entries take the extractor by value and wrap it in one owned `KeyHalf` (`key.rs`), which owns truncation and counting-window translation and is cloneable whenever the wrapped extractor is | Public sort contracts and complete partition/finishing operations |
| `engine/plan.rs` | Bounded sampling, node and tile decisions, digit work estimates | Records, candidate variation and effective tile capacity go in; an executable decision comes out |
| `engine/tile.rs` | Capacity normalization, scratch allocation, cached classification, counting passes and restoration | Finish a cursor-bounded slice; no raw pointers, cursor layout or separately armed guard cross the boundary |
| `block.rs` and `block/` | Physical block storage, stable bucket chains, ledger costs, completion and recovery | A `Classify` bucket index per record goes in; logical bucket boundaries and safe completion callbacks come out |
| `engine/parallel.rs` | Task batching and scheduling of the shared decisions | Exclusive bucket borrows and the planner's decision, not a second cost model |

A maintenance change to sampled occupancy or a leaf estimate stays in planning; it does not require changing record movement. A change to tile scratch layout stays with its counting and recovery operation. Tile cursor size, maximum record count, alignment padding and table capacity derive from its cursor type and the shared byte budget; planning consumes those facts rather than pricing an independent two-byte representation. Block representation costs stay derived beside the physical representation, not copied into the strategy model.

## Cost model

The planner separates exact work from advisory estimates and empirical budgets. These are not interchangeable scores: a bucket-table entry, a record pass and a parallel task have different costs. The current model keeps their established decisions explicit rather than introducing unmeasured conversion weights.

| Decision | Facts and constraints | Estimate or empirical policy |
|---|---|---|
| Flat digit plan | Remaining varying positions and maximum width | Fewest passes, then fewest table slots over legal splits |
| Tile table width | Cursor size and actual tile bytes | A minimum width and a bucket-per-record cap |
| Tile MSD versus LSD | Whether the width-bounded windows cover the remaining bits | Finish short flat plans; otherwise partition the highest digit and inspect actual children |
| Node digit width | Effective tile capacity and block fan-out limit | Target children of a fraction of a tile; widen a concentrated digit using sampled occupancy |
| Equality partition | Repeated complete keys in the same bounded sample | A sample-share threshold, sampled only when a node spans several tiles |
| Block size | Descriptor bytes, bucket count, record size and input length | Storage owns its balanced-memory formula, scale and amortization floors |

The values behind each row are on the [routing](../explain/routing.md), [tile](../explain/tiles.md) and [block partition](../explain/block-partition.md) pages; this table fixes only which facts and which policies the planner keeps apart.

Node samples choose partitions, never certify ownership, equality of actual records or recursion completion. Tiles do not sample or estimate populations: full child scans restrict actual variation to the remaining candidate mask, and constant children finish after that scan. Node execution and parallel scheduling consume decisions without reproducing the planner’s thresholds.

## Pipeline

The [explanation pages](../explain/index.md) own the routing rules, thresholds and their reasoning, and their figures run tested models of the same code. This table maps each stage to its page and code owner; change a stage's rule on its page, and the code, model and page move together.

| Stage | Rule and rationale | Code owner |
|---|---|---|
| Order probe, budgeted insertion, varying bits, constant and flat inputs | [Routing an input](../explain/routing.md) | `engine.rs` |
| Ordered children, bucket-order split, equality and top-digit partitions, counted window | [Nodes and tiles](../explain/routing.md#nodes-and-tiles) | `engine.rs`, `engine/plan.rs` |
| Stable block partition, block size, scratch bound | [The block partition](../explain/block-partition.md) | `block.rs`, `block/` |
| Tile leaves, digit planning, cached bucket IDs | [Finishing tiles](../explain/tiles.md) | `engine/tile.rs`, `engine/plan.rs` |
| Key encoding and varying bits | [Keys as unsigned integers](../explain/keys.md) | `key.rs` |

Structural facts that cut across those pages: `insertion_probe` owns root insertion eligibility, its window and budget, including unlimited finishing for insertion-sized tiles; `finish_ordered` owns ascending completion and strict or stable descending reversal for the initial probe, deferred flat confirmation and recursive children; root and child variation share one candidate-masked scan; serial and parallel execution consume one `NodePlan` decision, once (parallel roots shard equality and digit partitions; existing-run splits execute serially without resampling); a `Direct` extractor may finish a covering digit by counting because every sealed encoding is a bijection on bit patterns, while `ByKey` declines because a user key does not identify the payload; and scratch is created only after the root fast paths, so confirmed ordered and constant inputs never allocate.

## Parallel root completion

With the optional `rayon` feature, small inputs and one-thread pools use the serial engine. Larger roots share `prepare` with serial sorting, including order confirmation, stable reversal, budgeted insertion and variation. The [parallel layout](../explain/block-partition.md#parallel-roots) owns byte gates, shard clamps and block sizing. The source ledger is built directly from the scattered shards: bucket prefixes in original shard order, over each shard's own scratch allocation. Shard guards own recovery until every ledger table exists, then hand it to the ledger without a fallible step; no separate packed output is allocated.

A repeated-key decision partitions with the `Rank` classifier, whose bucket is the below/equal/above rank plus one at or above the sampled key's top-varying-bit boundary. It runs through the same sharded partition and final-band callbacks as a digit decision, which uses the `KeyDigit` classifier; callbacks receive the bucket index and skip the complete equal bucket, and every other bucket continues without the top bit.

Every block partition uses one relocatable-source ledger. A flat source table owns initialized prefix lengths and atomic physical locations; buckets hold source ranges and final input ranges, not private record buffers. A complete source transfer protects the destination interval, except when the initial source itself occupies a touched block. After protection it reloads and locks the source's verified current owner. If that source stayed in the explicitly skipped block, an overlap-safe copy transfers its prefix and marks that block final; otherwise the source is disjoint and becomes free. A relocated source's new owner lock also observes its skipped old block becoming final. Physical-owner locks follow increasing ID order. Final means no unread source remains, not that all fragments of the block are filled. Adjacent bands may share block fragments but never live record slots.

Once all a band's sources retire and every destination block is protected, that movement task lends the final band with all locks released. One callback owns child analysis: candidate-masked variation, constant completion, order confirmation, stable reversal, then node or tile routing. Tile storage owns the [whole-child allowance](../explain/tiles.md#parallel-child-allowance); larger children retain ordinary descendant tiles. A completed candidate returns without taking scratch. No batch queue or second sorting job retains the borrow. Serial partitions keep their existing cache-hot tile callbacks.

### Progress, recovery and retained scratch

Every free cache is registered before jobs start and remains discoverable until movement and recovery finish. Writers try their own cache, the shared reserve and peer caches; peer acquisition is nonblocking. A free search holds no physical-owner lock, and no cache, owner or scratch-pool lock spans user code. After retiring g source prefixes, a bucket touches at most g + 2 destination blocks; while waiting to protect the next block, at most one extra is already final. With q actual shards and R buckets, the ledger starts with at least qR free blocks. At least qR − R genuinely free blocks remain, covering the other buckets' at most R − 1 checked-out candidates when q ≥ 2. Lock holders perform finite movement without callbacks or Rayon scheduling. This rules out an internal resource wait cycle, conditional on returning callbacks and eventual execution of runnable work, not arbitrary user blocking or starvation.

Serial completion uses exclusive `RefCell` metadata, preserving non-`Send` records and mutable extractors. Parallel completion uses mutexes through the same exclusive-access contract. A completion guard takes restoration responsibility after all tables and boundaries exist, and finishes unpublished bands if execution unwinds. Constructing this ledger during scatter-panic recovery can allocate; once constructed, its recovery moves records without allocating or calling user code.

The bucket's `completed` flag records movement completion before its borrow is published, not successful sorting. A callback panic is caught after the serial sorter restores its slice, and all jobs join before propagation. After an outer execution failure joins all jobs, the completion guard finishes unpublished bands without callbacks. Recovery never touches a published band. Once all ledger tables and bucket ends exist, each shard clears `owns_recovery` to disarm its partition guard before movement begins: the ledger then owns restoring the whole input. Shard scratch stays alive until completion and recovery return; stale source bytes are never treated as live records.

Root scratch, the shard links and the overlapping ledger use O(√(nP)) space for pool width P. There is no bucket-sized staging term. Reentrant callbacks can still suspend up to R finishers, not merely P: each checked-out or pooled scratch allocation remains charged, including tile records, cached IDs, block scratch and serial continuation metadata. The tile allowance bounds one complete child independently of n; it is not a bound on all finisher storage. Nested public calls own separate scratch, without excluding suspended outer allocations. The equality digit bounds serial node depth by key width. The [P5 analysis](evidence.md#p5-recursive-parallel-nodes) separately requires retiring all parent capacities before admitting another parallel phase; a depth bound alone does not justify recursive parallel admission.

Serial output advances in bucket order. After g retired prefixes, its written length is at most g block lengths, so at most g complete input blocks are final. With at least R initially free blocks, the free count stays at least R; the next prefix needs to protect at most one new complete input block. This one-active-band argument does not require the parallel two-shard premise.

### Narrow direct keys

When the root digit covers all remaining variation in at most eight bits, direct keys use the same `KeyCounts` summary as serial counting: one count per bucket and one encoded key with its window bits cleared. Each worker summarizes a disjoint mutable shard, and a reduction combines the summaries. The summary pairs disjoint destination runs with keys decoded from those outside bits and each bucket's digit; serial sorting fills them directly and parallel sorting schedules their fills. Only `Send` is required. Sealed encodings are bit-exact bijections, so decoded keys preserve the exact multiset, including NaN payloads, signed zeros and `Reverse`. User-key records decline this shortcut because equal keys do not identify their payloads. Narrowed 128-bit extractors delegate with the original bit offset. Histograms use O(PR) space; no n-record cache is built.

## Insertion and scans

Every insertion pass, whether a budgeted probe or the finish of a tile or bucket, runs one loop, `insertion_loop`: it scans for the first adjacent pair whose keys descend, moves the later record left, and resumes the scan after it. A probe returns its swap count and may stop at its budget. A finishing pass, `finish_insertion`, has no budget and returns `()`, so the unused count is discarded inside the inlined loop rather than returned across a target-feature boundary. The loop receives an inversion operation alongside the extractor: scalar scans use that same mutable extractor, while direct-key passes capture the backend selected before entering the loop. Direct encoding stays with `Direct`; no insertion-only extractor transports a SIMD token or supplies an independent extraction policy.

The scan depends on the extractor, not on a backend feature:

| Pass | Implementation |
|---|---|
| Key function (`stable_sort_by_key`) | One record at a time, keeping the previous key, so the key function runs once per scanned record |
| Primitive direct keys, budgeted probe and insertion finish | Vector scan, with the instruction set selected once per pass |
| Primitive direct keys, ascending confirmation | The vector scan, dispatched once after checking the encoded predecessor |
| `Reverse<K>` direct keys | The same explicit vector scan as `K`, comparing the underlying keys with the pair sides exchanged |

Slices shorter than 64 records and 128-bit and pointer-sized keys scan one pair at a time. Each direct key type supplies its native lane count and an encoded-order pair mask; one scan owns block traversal, mask aggregation and the scalar tail. Integer comparison and float bit encoding use explicit `fearless_simd` vector operations rather than depending on automatic vectorization. `Reverse<K>` keeps the underlying lane count and reverses comparison by swapping transparent shared views, including for nested `Reverse`. Confirmation, probes and finishing remain shared. The insertion loop is inlined into the dispatched function, so its scan runs with the selected target features.

Dispatch stays inside one insertion pass: per-scan dispatch repeats overhead on short runs, and moving a SIMD token outside its target-feature context loses the intended code generation. Every build requires `std` and uses `fearless_simd`, with no backend feature switch. User-key tiles retain scalar callbacks rather than materializing a second key array. The [SIMD constraints](evidence.md#rejected-or-on-hold) were measured outside the paired protocol and establish no formal speedup claim.

## Record handling

Records are moved by bitwise copies between the input and `MaybeUninit<T>` scratch. Every record has exactly one live owner at any time; other physical copies are stale bytes that are never dropped, borrowed or returned.

Each counting pass reads the keys twice: once for the histogram and once to scatter the records. How the second read happens depends on who computes the key.

- **User key functions** (`stable_sort_by_key`). The histogram stores a two-byte bucket number per record, and scatter reads it instead of calling the key function again. Bucket ranges are disjoint, so each output slot is written exactly once whatever the key function returns. This keeps `String`, `Box<T>` and `&mut T` records safe and every record type on one path.
- **Library-encoded keys** (`stable_sort`). The keys are the records and are encoded by Tessera, so they are deterministic. Scatter recomputes them and stores nothing.

The extractor owns this distinction through `Extract::DIRECT`, not an independent mode passed through the serial and parallel executors. It is present only when every record is its own sealed key; its unsafe implementation contract then requires deterministic, panic-free extraction, and narrowed extractors inherit it with a translated key offset. Tile allocation, histogram and scatter consume this constant, and so does window counting, which refills runs only where equal keys make records interchangeable. Classification caching and count eligibility are therefore one fact. Adding an extractor no longer requires choosing a cache mode at each sort entry.

The research prototype skipped the stored bucket numbers for `Copy` records by filling scratch with valid records on the first pass. The port stores them for every record type; the [measured cost](evidence.md#port-parity-baseline) is within 3% of the prototype forced onto cached IDs.

## Panic safety

Key functions are user code and may panic. On a panic:

- the block partition rebuilds each block's offsets from the bucket fill counts and the number of complete input blocks, then moves every live record back into the input slice;
- a tile keeps a pointer to the complete record set from the last finished pass and copies it back if needed;
- the panic is then resumed.

The slice holds every record afterwards, in an unspecified order. No record is dropped twice or leaked.

Every build catches a key-function panic during block partitioning, restores the complete record set and resumes the panic. There is no alternate abort-only recovery implementation.

## Stack usage

Each tile-local MSD level keeps a cursor table of up to 32 KiB on the stack. The depth is bounded by ⌈key bits / 6⌉, at most 22 levels for 128-bit keys. Node recursion is bounded by the key width as well: equality, run and digit splits all remove the top varying bit from every unfinished child, so a node nests at most once per key bit, together with its frame and partition boundary vector. Actual frame sizes depend on the compiler.
