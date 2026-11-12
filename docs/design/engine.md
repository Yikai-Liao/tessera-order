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
| `block.rs` and `block/` | Physical block storage, stable partition, directory costs, compaction and recovery | Logical bucket boundaries and safe completion callbacks |
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

With the optional `rayon` feature, inputs below 1 MiB and one-thread pools use the serial engine. Larger roots use the same `prepare` operation as serial sorting for order confirmation, stable reversal, budgeted insertion and root variation, then scatter disjoint shards in the current pool. Preparation returns either completion or the varying-bit mask; the parallel layer does not maintain its own extractor wrapper or copy of that routing. The shard count is at most the pool width and approximately one per 256 KiB of input. Directory adoption concatenates each bucket's prefixes in original shard order and retains shard scratch as separate allocations. Block storage addresses scratch as equal power-of-two segments, one per scattered shard, so a serial pass is the one-segment case of the same lookup, not a second representation. The partition guard owns adoption through directory construction, scratch transfer and recovery handoff; execution consumes the resulting guard and record boundaries. Its active block range separates the free prefix and suffix from the live sequence. The prefix reserves one block per bucket in a shard, but one per bucket per shard after adoption; compaction and staging consume that block range, not a logical bucket count. Adoption reuses a shard's block-end table for logical bucket boundaries; bounded finishing reads those boundaries without interpreting shard counts or partial-block multiplicity.

A sampled repeated key takes the same below/equal/above partition as the serial engine, encoded as ranks 0, 1 and 2 of a two-bit digit; the fourth bucket is unused. The existing sharded partition materializes those bands before splitting their mutable borrows. The equal band is complete, and the two outer bands finish with the original extractor in independent jobs. The rank closure uses `ByKey`, which caches classification and declines counting/filling because equal ranks do not identify record payloads. Changing-key outer bands retain the serial child route's progress guards.

Both completion strategies use one relocatable-source ledger and whole-bucket recovery state. One flat source table owns each prefix's length and atomic location; buckets retain ranges in that table, and physical owners identify its source entries. The streamed route protects each source prefix's entire final interval, evacuating any unread destination sources, then reloads and validates that source's location before copying it directly into the final band. The source may itself have moved during protection; no temporary record block is needed. After all its sources retire and its complete final band is protected, the same movement task calls the ordinary child finisher with every ownership/cache lock released. No batch queue or second sorting task retains that borrow. If the root digit leaves no candidate bits, both completion callbacks return without taking scratch. Adjacent buckets remain distinct borrows even if their addresses touch. Streamed and bounded-staging callbacks reuse serial scratch and enter the same child analysis as serial recursion: candidate variation, constant termination, order confirmation and stable reversal, then node or tile routing. Only private staging receives the existing extended tile allowance. Scheduling does not assemble a separate analysis pipeline. Serial tile-sized buckets still finish directly during serial compaction to retain cache-hot completion.

A streamed bucket can touch at most two more destination blocks than it has retired source prefixes. While it waits to finalize a block, at most one extra is already final; the adopted reserve of at least PR free blocks, with at least two shards, covers other checked-out candidates as well as the waiter. Cache holders run no callbacks while holding locks. All caches are reserved before movement, including one extra entry per bucket for a returned relocation candidate. Streamed completion marks whole buckets finished before publishing borrows; callback panics are caught until all movement and jobs join, and an outer execution failure completes unfinished bands without callbacks. It shares staging's owners, locations, free discovery and recovery handoff rather than keeping permutation and compaction recovery.

### Bounded staging

For at least 2²¹ records of at most 16 bytes, offer bounded finishing with limit `L = 16 * floor(sqrt(n))`. Disable it if `L * R < n`, and decline after scatter if any actual bucket exceeds L. With the minimum length, `L * R >= n` confines staging to the window 2²¹ ≤ n ≤ 256 R². The window is necessary, not sufficient: a uniform root near its upper end already has a bucket above L. Roots outside it use materialization. Eligible layouts use four times the balanced root block size `ceil(sqrt(20n / (2PRs)))`, with the existing 1,024-byte and 32-record floors. The factor 20 is derived as twice the representation's per-position descriptor cost, accounting for local and adopted directories coexisting. The multiplier amortizes per-source ownership work; the [measurements](evidence.md#parallel-completion-baseline) cover the whole call, not an isolated causal attribution of this parameter.

A bounded bucket gathers all its prefixes into private `MaybeUninit<T>` staging in stable directory order. Each prefix is protected by its physical-owner mutex until the copy transfers ownership. Its source block is then free, and the staging slice becomes the unique live owner. The shared child finisher owns the staged-bucket allowance: it extends the effective tile capacity to the bucket length but at most 65,535 records, only for that operation. The scheduler neither interprets cursor limits nor repeats byte-to-record conversion.

Before writing a final band, a worker protects every full input block it touches. A block holds either a logical source, no source, or finalized fragments. An unread source moves to a free block under both physical-owner locks, acquired in increasing ID order; its atomic logical location is published before the locks release. Gather validates the owner after loading that location and retries if relocation won the race. A finalized block holds no unread source, so adjacent final bands can write disjoint fragments without overlapping record access. The input tail has no partition source.

Every bucket's free cache is registered before jobs begin and survives until all jobs and recovery finish. Writers try their own cache, the reserve and peer caches; peer locks use nonblocking acquisition. They wait without physical-owner locks, and no cache or owner lock spans user code. If there are P shards and R buckets, adoption starts with at least PR free blocks. A bucket gathering k source prefixes frees k blocks and touches at most k + 1 destination blocks, so at least PR − R genuinely free blocks remain. Production completion has at least two shards, leaving at least R. A waiting bucket has no local candidate; at most R − 1 other buckets can each hold one unpublished or checked-out candidate. Usable blocks therefore remain discoverable or held only by callback-free work that can publish progress.

A key panic is caught per staged bucket after the serial sorter restores its record set. That bucket still writes its band, and other buckets may continue sorting. All jobs join before a caught key panic is resumed. If the outer execution itself unwinds, recovery completes unfinished bands without further key calls before resuming that panic. Completed bands are not written twice. Stale staging and source bytes are never treated as live records.

Staging is released after each completed band. Ordinary scheduling retains only active buffers, but reentrant key callbacks can suspend up to R staged buckets, and since staging requires `L * R >= n` they can together approach the input size. Root scratch uses O(√(nP)) space and staging adds O(R√n); R is fixed by the twelve-bit root cap. Together with bounded histogram storage, the conservative public bound is O(P√n), not a promise of P simultaneous buffers.

### Narrow direct keys

When the root digit covers all remaining variation in at most eight bits, direct keys use the same `KeyCounts` summary as serial counting: one count and one representative per bucket. Each worker summarizes a disjoint mutable shard, and a reduction combines the summaries. The summary pairs disjoint destination runs with their representatives; serial sorting fills them directly and parallel sorting schedules their fills. Only `Send` is required. Equal sealed encodings mean identical record bits, so run filling preserves the exact multiset, including NaN payloads. User-key records decline this shortcut because equal keys do not identify their payloads. Narrowed 128-bit extractors delegate with the original bit offset. Histograms use O(PR) space; no n-record cache is built.

## Insertion and scans

Every insertion pass, whether a budgeted probe or the finish of a tile or bucket, runs one loop, `insertion_loop`: it scans for the first adjacent pair whose keys descend, moves the later record left, and resumes the scan after it. A probe returns its swap count and may stop at its budget. A finishing pass, `finish_insertion`, has no budget and returns `()`, so the unused count is discarded inside the inlined loop rather than returned across a target-feature boundary.

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

The extractor owns this distinction through `Extract::CACHE_BUCKETS`, not an independent mode passed through the serial and parallel executors. Its unsafe implementation contract permits opting out only when key extraction is deterministic and panic-free; narrowed extractors inherit that guarantee. Tile allocation, histogram and scatter consume the same associated constant. Adding an extractor no longer requires choosing a cache mode at each sort entry.

The research prototype skipped the stored bucket numbers for `Copy` records by filling scratch with valid records on the first pass. The port stores them for every record type; the [measured cost](evidence.md#port-parity-baseline) is within 3% of the prototype forced onto cached IDs.

## Panic safety

Key functions are user code and may panic. On a panic:

- the block partition rebuilds each block's offsets from the bucket fill counts and the number of complete input blocks, then moves every live record back into the input slice;
- a tile keeps a pointer to the complete record set from the last finished pass and copies it back if needed;
- the panic is then resumed.

The slice holds every record afterwards, in an unspecified order. No record is dropped twice or leaked.

Every build catches a key-function panic during block partitioning, restores the complete record set and resumes the panic. There is no alternate abort-only recovery implementation.

## Stack usage

Each tile-local MSD level keeps a cursor table of up to 32 KiB on the stack. The depth is bounded by ⌈key bits / 6⌉, at most 22 levels for 128-bit keys. Actual frame sizes depend on the compiler.
