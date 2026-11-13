# Engine reference

This page owns the engine's code boundaries and invariants in `rust/tessera-sort`. Routing rules, thresholds and their reasons live on the [explanation pages](../explain/index.md); [engine evidence](evidence.md) keeps failed experiments and open measured losses.

## Vocabulary

| Term | Meaning |
|---|---|
| Key | The unsigned integer a record sorts by. Signed integers, floats and `Reverse` keys are encoded into unsigned keys whose integer order is the requested order. |
| Digit | A window of key bits handled by one pass: a shift and a width. |
| Subproblem | A record slice and a candidate mask of key bits still to be ordered. A completed digit leaves the mask even if a key function changes its answers. |
| Bucket | A logical, stable classification group. It may span physical blocks; once complete it is a contiguous subproblem, not necessarily a task or a tile. |
| Block | A physical storage and ownership-transfer unit of the stable partition. Its size and metadata costs belong to the block mechanism. |
| Node | A subproblem larger than the tile capacity, split by a stable block partition or by existing ordered runs. |
| Tile | A subproblem within a record capacity derived once from the byte budget; routing compares lengths, not bytes. [Tiles](../explain/tiles.md) defines the budget. Tiles finish in scratch memory. |
| Cursor table | One `u16` position per bucket of a tile pass, bounded by the tile and cursor budgets. |

## Knowledge boundaries

| Owner | Complete responsibility | Consumers need to know |
|---|---|---|
| `engine.rs` | Root preparation, candidate-bit progress, recursive node execution. Both entries take the extractor by value and wrap a narrowed 128-bit key in one `KeyHalf` (`key.rs`), which owns truncation and counting-window translation | Public sort contracts and complete partition and finishing operations |
| `order.rs` | Scalar order scans and stable insertion loops, independent of recursive execution | Extractors supply keys; direct-key callers supply a scan in their selected SIMD context |
| `engine/plan.rs` | Bounded sampling, node and tile decisions, digit work estimates | Records, candidate variation and tile capacity go in; an executable decision comes out |
| `engine/tile.rs` | Capacity, scratch allocation, cached classification, counting passes and restoration | Finish a cursor-bounded slice; no raw pointer, cursor layout or guard crosses the boundary |
| `block.rs`, `block/` | Physical block storage, stable bucket chains, ledger costs, completion and recovery | A `Classify` bucket per record goes in; bucket boundaries and completed bands come out |
| `engine/parallel.rs` | Shard count, task scheduling and scratch pooling for the shared decisions | Exclusive bucket borrows and the planner's decision, not a second cost model |

A change to sampled occupancy or a leaf estimate stays in planning and does not touch record movement. A change to tile scratch layout stays with its counting and recovery operation. Tile cursor size, record capacity, alignment padding and table capacity derive from the cursor type and the shared byte budget, and planning consumes those facts. Block metadata costs are derived beside the physical representation (`finish::serial_descriptor_bytes`), not copied into the strategy model.

## Cost model

The planner keeps exact facts, advisory estimates and empirical budgets apart. A bucket-table entry, a record pass and a parallel task have different costs, and no unmeasured weight converts between them.

| Decision | Facts and constraints | Estimate or empirical policy |
|---|---|---|
| Flat digit plan | Remaining varying positions and maximum width | Fewest passes, then fewest table slots over legal splits |
| Tile table width | Cursor size and actual tile bytes | A minimum width and a bucket-per-record cap |
| Tile MSD versus LSD | Whether the width-bounded windows cover the remaining bits | Finish short flat plans; otherwise partition the highest digit and inspect actual children |
| Node digit width | Tile capacity and block fan-out limit | Target children of a fraction of a tile; widen a concentrated digit using sampled occupancy |
| Equality partition | Repeated complete keys in the bounded sample | A sample-share threshold, sampled only on nodes of several tiles |
| Block size | Metadata bytes, bucket count, record size and input length | Storage owns its balanced-memory formula, scale and floors |

The values are on the [routing](../explain/routing.md), [tile](../explain/tiles.md) and [block partition](../explain/block-partition.md) pages.

Node samples choose partitions; they never certify ownership, equality of actual records or recursion completion. Tiles do not sample: full child scans restrict actual variation to the remaining candidate mask, and constant children finish after that scan. Execution and scheduling consume decisions without reproducing the planner's thresholds.

## Pipeline

| Stage | Rule and rationale | Code owner |
|---|---|---|
| Order probe, budgeted insertion, varying bits, constant and flat inputs | [Routing an input](../explain/routing.md) | `engine.rs` |
| Child order, equality, run and top-digit partitions, counted window | [Nodes and tiles](../explain/routing.md#nodes-and-tiles) | `engine.rs`, `engine/plan.rs` |
| Stable block partition, completion ledger, block size | [The block partition](../explain/block-partition.md) | `block.rs`, `block/` |
| Tile leaves, digit planning, cached bucket IDs | [Finishing tiles](../explain/tiles.md) | `engine/tile.rs`, `engine/plan.rs` |
| Key encoding and varying bits | [Keys as unsigned integers](../explain/keys.md) | `key.rs` |

A stage's rule changes on its page, in the code and in the page's model together. Structural facts across stages:

- `prepare` is shared by the serial and parallel entries. `insertion_probe` owns insertion eligibility, its window and budget, including unlimited finishing for insertion-sized inputs.
- `finish_ordered` owns ascending completion and strict or stable descending reversal, for the root probe, deferred flat confirmation and partition children.
- Root and child variation share one candidate-masked scan.
- Serial and parallel execution consume one `NodePlan` decision once. Parallel roots shard equality and digit partitions; an existing-run split runs serially without resampling.
- A `Direct` extractor may finish a covering digit by counting, because every sealed encoding is a bijection on bit patterns. `ByKey` declines, because a user key does not identify its payload.
- Scratch is created only after the root fast paths, so confirmed ordered and constant inputs never allocate.

## Parallel root completion

With the optional `rayon` feature, inputs below the parallel byte threshold and one-thread pools use the serial engine. Larger roots share `prepare` with serial sorting. The [parallel layout](../explain/block-partition.md#parallel-roots) owns the byte gates, shard clamps and block sizing.

Each shard scatters its own input range with its own scratch. `Partition` borrows the reusable `Buffers` directly: record scratch stays in that allocation, and its capacity cannot change until completion and recovery finish. The source ledger is built directly from the shard chains: bucket prefixes in original shard order, over each shard's own scratch. Shard guards own recovery until every ledger table exists, then hand it over without a fallible step; no packed output is allocated.

An equality decision partitions with the `Rank` classifier (the below/equal/above rank plus one at or above the sampled key's top-varying-bit boundary), a digit decision with `KeyDigit`. Both use the same sharded partition and band callbacks. The callback receives the bucket index, skips the complete equal bucket, and continues every other bucket without the top bit.

### Ledger movement

Serial and parallel partitions use one relocatable-source ledger. A flat source table owns initialized prefix lengths and atomic physical locations; buckets hold source ranges and final input ranges, not private buffers.

- A source transfer first protects every destination block, except the source's own initial block if it lies inside the destination. It then reloads and locks the source's verified current owner.
- If the source is still in its skipped block, an overlap-safe copy moves the prefix and marks that block final. Otherwise the source is disjoint from its destination, and its block becomes free. A relocated source's new owner lock also observes its old block becoming final.
- Physical-owner locks are taken in increasing ID order. *Final* means no unread source remains, not that the block is filled. Adjacent bands may share block fragments but never live record slots.

Once all of a band's sources retire and every destination block is protected, the movement task lends the band with all locks released. One callback owns child analysis: candidate-masked variation, constant completion, order confirmation, stable reversal, then node or tile routing. Tile storage owns the [whole-child allowance](../explain/tiles.md#parallel-child-allowance); larger children keep ordinary descendant tiles. No queue or second job retains the borrow. Serial partitions finish tile-sized bands in the same callback position while they are cache-hot.

### Progress, recovery and retained scratch

Every free cache is registered before jobs start and stays discoverable until movement and recovery finish. Writers try their own cache, then the shared reserve, then peers without blocking. A free search holds no physical-owner lock, and no cache, owner or scratch-pool lock spans user code.

After retiring g source prefixes, a bucket touches at most g + 2 destination blocks; while waiting to protect the next one, at most one extra is final. With q shards and R buckets, the ledger starts with at least qR free blocks, so at least qR − R genuinely free blocks remain to cover the other buckets' at most R − 1 checked-out candidates when q ≥ 2. Lock holders perform finite movement without callbacks or Rayon scheduling. This rules out an internal resource wait cycle, given returning callbacks and eventual execution of runnable work; it does not cover arbitrary user blocking or starvation.

Serial output advances in bucket order. After g retired prefixes, at most g complete input blocks are final; with at least R initially free blocks the free count stays at least R, and the next prefix protects at most one new block. This one-active-band argument needs no two-shard premise.

One partition also supports unordered bucket jobs: every bucket has only full source blocks followed by at most one partial prefix. The first nonempty band starts at zero, so it stays block-aligned until its last prefix. If all unfinished movement jobs wait for a free candidate, that band has at most g₀ final blocks after g₀ retired sources; each other band has at most gᵢ + 1. Thus the total is at most g + R − 1 and the initial R free blocks leave at least one genuinely free block. It is either in the reserve or in a discoverable cache; a waiting job holds an empty cache, and callbacks hold none. A job holding a candidate or owner lock instead has finite movement left, so it is not part of this all-waiting state. Relocation preserves prefix lengths, and final blocks are never reused as sources.

Serial completion uses exclusive `RefCell` metadata, which keeps non-`Send` records and mutable extractors; parallel completion uses mutexes under the same exclusive-access contract. A bucket empties its pending-source range after all movement, before publishing its borrow; recovery skips that retired range even if child sorting unwinds.

- A callback panic unwinds through its child sorter, which restores that band first. Parallel jobs join before the parent completion guard restores unpublished bands; no per-bucket panic payload is stored.
- After any execution failure, once all jobs join, the completion guard finishes every unpublished band without callbacks. Recovery never touches a published band.
- Once all ledger tables and bucket ends exist, each shard clears `owns_recovery` before movement begins; the ledger then owns restoring the whole input. Shard scratch stays alive until completion and recovery return, and stale source bytes are never treated as live records.
- Building the ledger during scatter-panic recovery can allocate; once built, its recovery moves records without allocating or calling user code.

Root scratch, the shard links and the ledger use O(√(nP)) space for pool width P. Reentrant callbacks can suspend up to R finishers, not merely P, and each checked-out or pooled scratch allocation stays charged: tile records, cached IDs, block scratch and serial continuation metadata. The tile allowance bounds one complete child independently of n, not all finisher storage. Nested public calls own separate scratch. The [P5 analysis](evidence.md#p5-recursive-parallel-nodes) requires retiring all parent capacities before admitting another parallel phase; the key-width depth bound alone does not justify recursive parallel admission.

### Narrow direct keys

When the root digit covers all remaining variation in at most eight bits, direct keys use the `KeyCounts` summary that serial counting uses: one count per bucket and one encoded key with its window bits cleared. Each worker summarizes a disjoint shard and a reduction combines the summaries. Disjoint destination runs are then filled with keys decoded from the shared outside bits and each bucket's digit, serially or as parallel tasks; only `Send` is required. Sealed encodings are bit-exact bijections, so decoded keys preserve the exact multiset, including NaN payloads, signed zeros and `Reverse`. User-key records decline because equal keys do not identify their payloads. Narrowed 128-bit extractors delegate with the original bit offset. Histograms use O(PR) space.

## Insertion and scans

Every insertion pass, a budgeted probe or the finish of a tile or bucket, runs `order::insertion_loop`: find the first descending adjacent pair, move the later record left, resume after it. A probe returns its swap count and may stop at its budget. `finish_insertion` has no budget and returns `()`, so no unused count crosses a target-feature boundary. The loop receives an inversion scan alongside the extractor: user-key scans use the same mutable extractor, direct-key passes capture the backend selected before entering the loop. No insertion-only extractor carries a SIMD token.

| Pass | Implementation |
|---|---|
| Key function (`stable_sort_by_key`) | One record at a time, keeping the previous key, so the key function runs once per scanned record |
| Primitive direct keys, probe and finish | Vector scan, instruction set selected once per pass |
| Primitive direct keys, ascending confirmation | The same vector scan, dispatched once after checking the encoded predecessor |
| `Reverse<K>` direct keys | The scan of `K`, with the pair sides exchanged |

Slices shorter than 64 records and 128-bit and pointer-sized keys scan one pair at a time. Each direct key type supplies its lane count and an encoded-order pair mask; one scan owns block traversal, mask aggregation and the scalar tail. Integer comparison and float encoding use explicit `fearless_simd` operations rather than autovectorization. `Reverse<K>` keeps the lane count of `K` by swapping transparent shared views, also for nested `Reverse`.

Dispatch happens once per insertion pass and the loop is inlined into the dispatched function: per-scan dispatch repeats overhead on short runs, and a SIMD token outside its target-feature context loses the intended code generation. Every build requires `std` and uses `fearless_simd`; there is no backend switch. User-key tiles keep scalar callbacks rather than materializing a key array. The [SIMD constraints](evidence.md#rejected-or-on-hold) were measured outside the paired protocol and support no formal speedup claim.

## Record handling

Records move by bitwise copies between the input and `MaybeUninit<T>` scratch. Every record has exactly one live owner at any time; other physical copies are stale bytes that are never dropped, borrowed or returned.

A counting pass reads each key twice, for the histogram and for the scatter:

- **User key functions** (`stable_sort_by_key`). The histogram stores a two-byte bucket per record and the scatter reads it back. Bucket ranges are disjoint, so each output slot is written exactly once whatever the key function returns. This keeps `String`, `Box<T>` and `&mut T` records safe on one path.
- **Library-encoded keys** (`stable_sort`). The records are their own keys, encoded deterministically by Tessera, so the scatter recomputes them and stores nothing.

`Extract::DIRECT` owns this distinction, not a mode passed through the executors. It is present only when every record is its own sealed key; its unsafe contract requires deterministic, panic-free extraction, and narrowed extractors inherit it with a translated offset. Tile allocation, histogram, scatter and window counting consume this one fact.

The research prototype skipped stored buckets for `Copy` records by filling scratch with valid records first. The port stores them for every record type; the [measured cost](evidence.md#port-parity-baseline) is within 3% of the prototype forced onto cached IDs.

## Panic safety

Key functions are user code and may panic. On a panic:

- the block partition guard retains bucket cursors and input-block progress; on unwind its destructor derives the unread suffix, appends it to bucket 0 without calling the failed classifier, and completes the chains through the source ledger;
- a tile keeps a pointer to the complete record set of the last finished pass and copies it back if needed;
- ordinary unwinding propagates the panic after the guards restore ownership.

The slice then holds every record once, in an unspecified order. Every build uses this recovery; there is no abort-only variant.

## Stack usage

Each tile-local MSD level keeps a cursor table of up to 32 KiB on the stack, at most ⌈key bits / 6⌉ levels (22 for 128-bit keys). Node recursion is bounded by the key width too: equality, run and digit splits remove the top varying bit from every unfinished child, so a node nests at most once per key bit, with its frame and boundary vector. Actual frame sizes depend on the compiler.
