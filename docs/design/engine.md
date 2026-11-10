# Design

This document describes the engine in `rust/tessera-sort`. It describes current routing and invariants; the [measurement record](port.md) covers the initial port and subsequent measured changes.

## Vocabulary

| Term | Meaning |
|---|---|
| Key | The unsigned integer a record sorts by. Signed integers, floats and `Reverse` keys are encoded into unsigned keys whose integer order matches the requested order. |
| Digit | A window of key bits handled by one pass, described by a shift and a width. |
| Subproblem | A record slice and a candidate mask of key bits still to be ordered. A completed digit leaves that mask even if a key function changes its answers. |
| Bucket | A logical, stable classification group. It may span physical blocks; after materialization it is a contiguous subproblem, not necessarily a task or a tile. |
| Block | A physical storage and ownership-transfer unit used by the stable partition. Its size and directory costs belong to the block mechanism. |
| Node | A subproblem larger than the tile capacity. Nodes are split by a stable block partition or existing ordered runs. |
| Tile | A subproblem that fits a resource-bounded record capacity. The byte budget is converted once to `clamp(floor(bytes / record size), 1, 65,535)` records; routing compares lengths rather than reinterpreting cursor limits or byte products. The default budget is 64 KiB. Tiles are finished in scratch memory. |
| Cursor table | One `u16` position per bucket. At 14 bits it occupies 32 KiB. |

## Knowledge boundaries

| Owner | Complete responsibility | Consumers need to know |
|---|---|---|
| `engine.rs` | Root preparation, candidate-bit progress, recursive node execution | Public sort contracts and complete partition/finishing operations |
| `engine/plan.rs` | Bounded sampling, node and tile decisions, digit work estimates | Records, candidate variation and effective tile capacity go in; an executable decision comes out |
| `engine/tile.rs` | Capacity normalization, scratch allocation, cached classification, counting passes and restoration | Finish a cursor-bounded slice; no raw pointers, cursor layout or separately armed guard cross the boundary |
| `block.rs` and `block/` | Physical block storage, stable partition, directory costs, compaction and recovery | Logical bucket boundaries and safe completion callbacks |
| `engine/parallel.rs` | Task batching and scheduling of the shared decisions | Exclusive bucket borrows and the planner's decision, not a second cost model |

A maintenance change to sampled occupancy or a leaf estimate stays in planning; it does not require changing record movement. A change to tile scratch layout stays with its counting and recovery operation. Tile cursor size, maximum record count, alignment padding and table capacity derive from its cursor type and the shared byte budget; planning consumes those facts rather than pricing an independent two-byte representation. Block representation costs stay derived beside the physical representation, not copied into the strategy model.

## Cost model

The planner separates exact work from advisory estimates and empirical budgets. These are not interchangeable scores: a bucket-table entry, a record pass and a parallel task have different costs. The current model keeps their established decisions explicit rather than introducing unmeasured conversion weights.

| Decision | Facts and constraints | Estimate or empirical policy |
|---|---|---|
| Flat digit plan | Varying positions and maximum width; exact number of passes and total bucket slots | Minimize passes, then slots; no fitted coefficient |
| Tile table width | Two bytes per cursor and actual tile bytes; 14-bit table cap | Six-bit minimum; at most eight buckets per record, or four for records wider than eight bytes |
| Tile MSD versus LSD | Exact minimum LSD pass count; sampled group populations and remaining variation | Compare record-weighted child passes: partition, confirmation or insertion, then the cheaper of flat work and recursively projected partitions; singleton populations assume uniform spread |
| Node digit width | Effective tile capacity and block fan-out limit | Target half-tile children; widen a concentrated digit using sampled occupancy |
| Equality partition | Repeated complete keys in the same bounded sample | A quarter-sample mode, sampled only when a node spans at least four tiles |
| Block size | Descriptor bytes, bucket count, record size and input length | Storage owns its balanced-memory formula, scale and amortization floors |

Samples choose work, never certify ownership, equality of actual records or recursion completion. Full child scans still restrict variation to the remaining candidate mask. Sample-constant children contribute confirmation work rather than recursive passes, for any number of groups; there is no separate low-fanout override. Unseen variation and nonuniform singleton populations can make the estimate wrong, affecting strategy but not completion. Node execution and parallel scheduling consume decisions without reading these estimates or reproducing their thresholds.

## Pipeline

### 1. Order probe

The engine chooses a stride of `max(1, floor(n / 4,096))`. At unit stride it classifies order in one adjacent-key scan: first reject a mixed direction among up to three keys, the shortest possible witness, then find the direction past any leading ties and check it while retaining the previous encoded key. It returns unsorted at the first conflict without a separate ascending scan; a constant slice returns as ascending. At larger strides it samples adjacent pairs to classify the input as ascending, descending, flat (every sampled pair equal) or unsorted. The 4,096-pair target determines the stride, not a hard sample limit. Unless unsortedness is found early, the loop visits `floor((n - 1) / stride)` pairs. An ascending or descending sample is confirmed by a full scan, which takes its direction from the first unequal key. A confirmed ascending input returns immediately. A confirmed descending input is reversed stably. Strictly descending keys need only the whole-slice reversal; otherwise each run of equal keys is reversed back. A flat sample is confirmed after the varying-bit scan below, since a constant input stops there.

An unsorted input that fits a tile and has at most 48 records goes directly to insertion finishing, using the existing leaf threshold without a budget or varying-bit scan. Confirmed ordered inputs still finish in the order probe. A larger unsorted input gets a budgeted stable insertion pass. When the probe window, one tile of records, is shorter than the input, the pass first runs on a buffered copy of the window's keys to estimate the number of swaps, and only then touches the records with a budget scaled to the whole input. The scaling uses a wide intermediate product so that a valid budget cannot overflow on 32-bit targets. Each call owns this encoded-key buffer and releases it before the radix path continues. If the budget runs out, the input is left as a stable permutation of itself and the radix path continues. See [Insertion and scans](#insertion-and-scans) for how the pass finds inversions.

### 2. Varying bits

One scan computes the bitwise union and intersection of all keys. Bits that are equal in every key cannot change the order and are excluded from every later digit. A constant input returns at this point. Wide keys whose variation fits in one 64-bit half are sorted by that half as a 64-bit key. The borrowed `KeyHalf` in `key.rs` and cloneable `Narrow` in `engine/parallel.rs` share one implementation of truncation and counting-window translation; each representation stays in its owning module. Other operations retain the narrowed extractor's defaults instead of forwarding the original-width backend. Tile and block scratch are created only after these root fast paths; budgeted insertion may already have allocated probe keys. Confirmed ordered and constant inputs do not allocate.

### 3. Node partitioning

Partition children entering `sort_child` first scan their remaining candidate bits, as every child did before, and a constant child ends there. The varying ones then reuse the root's full order confirmation, without its preceding sample: ascending children end, and descending children take the same strict or stable reversal as the root. This includes children reached after digit and equality partitions. The root enters `sort_node` directly because its order has already been probed; it does not repeat the gate. Only unordered children continue to the existing tile and radix routes. Both scans stop at their first conflict, so a child that continues pays them in key reads, and neither scan allocates or moves records unless descending order has been confirmed. Tile-sized buckets finished during block compaction still go directly to the tile leaf.

A node whose remaining variation spans at most eight bits is one window away from sorted. When the records are their own keys (`Direct`), it is finished by counting that window: one `usize` count and one remembered record per bucket, then each run is filled from its record. Every sealed encoding is a bijection on bit patterns, so equal keys mean equal records and a filled run is the same permutation the block partition would produce, without block tables or record moves. A user key function keeps payloads that its key does not identify, so `ByKey` declines and takes the partition. The eight-bit cap bounds the stack table to 256 counts and records. The window must cover every remaining key difference or filling would merge distinct records.

A node that reaches its digit partition may already sit in the two runs its top varying bit separates, and then it needs no pass at all. One scan reads that bit and reports whether the node is `0*1*` or `1*0*` in it; the second form is one rotation away from the first, and both runs keep their own order. Splitting there hands the bits below the split to the recursion, so the node avoids the block partition, its tables and its scratch. The check belongs to the digit route: a node that the narrow window finished, or that an equality partition split because its sample repeated, never reaches it. The scan stops at the first record that breaks the pattern, which unordered records reach within a few keys. Inputs that keep a separated block of keys above or below the body, in any number of pieces, therefore need neither a detector nor a length cap.

One shared `NodePlan` selects a narrow window, equality pivot, existing runs or digit for serial and parallel execution. The decision is consumed once: a parallel root taking an equality or existing-run route executes that decision serially without resampling. Both orientations of existing runs use this path. Run recognition precedes digit-width sampling, since a recognized run needs no digit table. Otherwise the node is split by one of two stable partitions. Equality sampling is tried only on nodes of at least four tiles, which amortize the bounded key sample:

- **Repeated key.** The node samples up to 128 keys in runs of 8 and sorts those encoded keys. If one value covers at least a quarter of the sample, an equality partition separates it from the rest, so heavily repeated keys stop costing passes. If changing keys leave the entire node below or above that sampled value, a digit partition consumes candidate bits instead of repeating an unchanged node.
- **Top digit.** Otherwise [`choose_split_digit`](../../rust/tessera-sort/src/engine/plan.rs) targets children of half a tile. It starts with enough bits for `n / target` children, clamped to 3–8 bits and the available key bits. The same sorted key sample checks whether at least a quarter of the digit's buckets are occupied; a concentrated digit widens by two bits at a time, up to 12 or the available key bits. Every tested width projects this retained sample rather than extracting records again. Adjacent projected groups give occupancy without a radix-sized bitmap. Tile leaf selection uses this sampling mechanism too; plans of at most two passes still skip sampling.

The digit partition uses the block LSD scheme of Clausecker and Schintke. Buckets are filled block by block, consumed input blocks are reused as output blocks, and permutation metadata records their logical order. Stability holds without a full-size output buffer, and the extra space is O(√n) for a fixed digit width and record size. The block size in records is

```
max(1, 4 * ceil(sqrt(10n / (2 R s))), floor(1024 / s), 32)
```

where `n` is the input length, `s` the record size and `R` the number of buckets. An integer reference in tests checks square boundaries and agreement with the floating-point formula.

The root is the paper's trade-off evaluated for this implementation. A block size `b` costs `2 R b s` bytes of scratch — one output block per bucket, plus as many in reserve, because an input block may only be recycled once every record in it has been read — and `10 n / b` bytes of tables, because a block position owns two `u32` block-order entries and one `u16` bucket id. The two terms are equal at that root, so it is the minimum of the sum: substituting `b = c b*` multiplies the total by `(c + 1/c) / 2`, which grows in both directions. The engine takes four times that root: the minimum is where memory is smallest, not where the sort is fastest, and a larger block amortises the random read that compaction pays per block.

The lower bounds are not from the paper. The compaction pass moves a block with one copy, so a block holds at least a kilobyte of records; and the bookkeeping of a block does not shrink with the record, so a block holds at least 32 records whatever their width. Their cost does not depend on `n`, so the O(√n) bound holds for a fixed record size and digit width.

Buckets that fit a tile are finished as tiles during compaction. Larger buckets become nodes and are partitioned again.

### 4. Tile finishing

A tile is finished by one of three leaves:

- **Insertion sort** for tiles and buckets of at most 48 records.
- **Tile-local MSD partition**, recursing into buckets with a cursor table on the stack.
- **LSD counting passes** over a scratch buffer, with digit widths between 6 and 14 bits.

The LSD digit planner minimizes record passes, then total bucket slots, over varying bit positions. This one objective covers dense, shifted and sparse variation. Greedy coverage supplies the minimum pass count for leaf selection; only a selected LSD leaf runs the bounded table-cost optimization. A plan has at most `ceil(128 / MIN_TILE_DIGIT_BITS)` windows, derived from key width rather than an independent capacity constant. Table width bounds cursor storage and empty-bucket work. Leaf selection compares record-pass work: one MSD partition plus each sampled group's estimated continuation, weighted by its sample population, against the flat pass count. Repeated groups estimate child length from their fraction of the sample and retain variation below the digit. Estimated insertion-sized groups cost one finish; larger groups cost one variation scan plus the cheaper of minimum flat passes or another partition projected from the same sample on their remaining bits. Recursive estimates consume candidate digits without extracting more records, allocating scratch or constructing exact flat plans. Sample-constant groups therefore cost only their confirming scan. Singletons cannot demonstrate constancy: their estimate uses the parent's remaining bits and a uniform population of `ceil(records / digit buckets)`. Plans of at most two flat passes skip sampling because the MSD estimate charges at least a partition and a child finish or confirmation. This is a bounded strategy estimate, not a cycle prediction or a guarantee of the actual child cost. Child variation scans return only bits in the remaining candidate mask, so even an inconsistent key function cannot restore a processed digit and prevent recursion from making progress.

## Parallel root completion

With the optional `rayon` feature, inputs below 1 MiB and one-thread pools use the serial engine. Larger roots use the same `prepare` operation as serial sorting for order confirmation, stable reversal, budgeted insertion and root variation, then scatter disjoint shards in the current pool. Preparation returns either completion or the varying-bit mask; the parallel layer does not maintain its own extractor wrapper or copy of that routing. The shard count is at most the pool width and approximately one per 256 KiB of input. Directory adoption concatenates each bucket's prefixes in original shard order and retains shard scratch as separate allocations.

The materialized route permutes full blocks in parallel and compacts them. A bucket is passed to a scoped sorting job only after compaction retires its range as both destination and copy source. Adjacent buckets remain distinct borrows even if their addresses touch. Jobs group at least 256 KiB of records when possible and reuse serial scratch. Both materialized jobs and bounded-staging callbacks enter the same child finisher as serial recursion: candidate variation, constant termination, order confirmation and stable reversal, then node or tile routing. Scheduling does not assemble a separate analysis pipeline. Serial tile-sized buckets still finish directly during compaction to retain cache-hot completion.

### Bounded staging

For at least 2²¹ records of at most 16 bytes, offer bounded finishing with limit `L = 16 * floor(sqrt(n))`. Disable it if `L * R < n`, and decline after scatter if any actual bucket exceeds L. Eligible layouts use four times the balanced root block size `ceil(sqrt(20n / (2PRs)))`, with the existing 1,024-byte and 32-record floors. The factor 20 accounts for local and adopted directories coexisting. The multiplier amortizes per-source ownership work; the [measurements](port.md#eight-core-parallel-completion) cover the whole call, not an isolated causal attribution of this parameter.

A bounded bucket gathers all its prefixes into private `MaybeUninit<T>` staging in stable directory order. Each prefix is protected by its physical-owner mutex until the copy transfers ownership. Its source block is then free, and the staging slice becomes the unique live owner. The shared child finisher owns the staged-bucket allowance: it extends the effective tile capacity to the bucket length but at most 65,535 records, only for that operation. The scheduler neither interprets cursor limits nor repeats byte-to-record conversion.

Before writing a final band, a worker protects every full input block it touches. A block holds either a logical source, no source, or finalized fragments. An unread source moves to a free block under both physical-owner locks, acquired in increasing ID order; its atomic logical location is published before the locks release. Gather validates the owner after loading that location and retries if relocation won the race. A finalized block holds no unread source, so adjacent final bands can write disjoint fragments without overlapping record access. The input tail has no partition source.

Every bucket's free cache is registered before jobs begin and survives until all jobs and recovery finish. Writers try their own cache, the reserve and peer caches; peer locks use nonblocking acquisition. They wait without physical-owner locks, and no cache or owner lock spans user code. If there are P shards and R buckets, adoption starts with at least PR free blocks. A bucket gathering k source prefixes frees k blocks and touches at most k + 1 destination blocks, so at least PR − R genuinely free blocks remain. Production bounded finishing has at least two shards, leaving at least R. A waiting bucket has no local candidate; at most R − 1 other buckets can each hold one unpublished or checked-out candidate. Usable blocks therefore remain discoverable or held only by callback-free work that can publish progress.

A key panic is caught per staged bucket after the serial sorter restores its record set. That bucket still writes its band, and other buckets may continue sorting. All jobs join before a caught key panic is resumed. If the outer execution itself unwinds, recovery completes unfinished bands without further key calls before resuming that panic. Completed bands are not written twice. Stale staging and source bytes are never treated as live records.

Staging is released after each completed band. Ordinary scheduling retains only active buffers, but reentrant key callbacks can suspend up to R staged buckets. Root scratch uses O(√(nP)) space and staging adds O(R√n); R is fixed by the twelve-bit root cap. Together with bounded histogram storage, the conservative public bound is O(P√n), not a promise of P simultaneous buffers.

### Narrow direct keys

When all remaining variation spans at most eight bits, direct keys use the same `KeyCounts` summary as serial counting: one count and one representative per bucket. Each worker summarizes a disjoint mutable shard, and a reduction combines the summaries. The summary pairs disjoint destination runs with their representatives; serial sorting fills them directly and parallel sorting schedules their fills. Only `Send` is required. Equal sealed encodings mean identical record bits, so run filling preserves the exact multiset, including NaN payloads. User-key records decline this shortcut because equal keys do not identify their payloads. Narrowed 128-bit extractors delegate with the original bit offset. Histograms use O(PR) space; no n-record cache is built.

## Insertion and scans

Every insertion pass, whether a budgeted probe or the finish of a tile or bucket, runs one loop, `insertion_loop`: it scans for the first adjacent pair whose keys descend, moves the later record left, and resumes the scan after it. A probe returns its swap count and may stop at its budget. A finishing pass, `finish_insertion`, has no budget and returns `()`, so the unused count is discarded inside the inlined loop rather than returned across a target-feature boundary.

The scan depends on the extractor, not on a backend feature:

| Pass | Implementation |
|---|---|
| Key function (`stable_sort_by_key`) | One record at a time, keeping the previous key, so the key function runs once per scanned record |
| Primitive direct keys, budgeted probe and insertion finish | Vector scan, with the instruction set selected once per pass |
| Primitive direct keys, ascending confirmation | The vector scan, dispatched once after checking the encoded predecessor |
| `Reverse<K>` | Blocked scans for confirmation and probes; scalar scans for insertion finishing |

Slices shorter than 64 records and 128-bit and pointer-sized keys scan one pair at a time. The insertion loop is inlined into the dispatched function, so its scan runs with the selected target features.

Dispatch stays inside one insertion pass: per-scan dispatch repeats overhead on short runs, and moving a SIMD token outside its target-feature context loses the intended code generation. Every build requires `std` and uses `fearless_simd`, with no backend feature switch. User-key tiles retain scalar callbacks rather than materializing a second key array. The [exploratory SIMD measurements](port.md#exploratory-simd-constraints) preserve historical evidence for these constraints but were not the full paired protocol and establish no formal speedup claim.

## Record handling

Records are moved by bitwise copies between the input and `MaybeUninit<T>` scratch. Every record has exactly one live owner at any time; other physical copies are stale bytes that are never dropped, borrowed or returned.

Each counting pass reads the keys twice: once for the histogram and once to scatter the records. How the second read happens depends on who computes the key.

- **User key functions** (`stable_sort_by_key`). The histogram stores a two-byte bucket number per record, and scatter reads it instead of calling the key function again. Bucket ranges are disjoint, so each output slot is written exactly once whatever the key function returns. This keeps `String`, `Box<T>` and `&mut T` records safe and every record type on one path.
- **Library-encoded keys** (`stable_sort`). The keys are the records and are encoded by Tessera, so they are deterministic. Scatter recomputes them and stores nothing.

The research prototype skipped the stored bucket numbers for `Copy` records. It filled scratch with valid records on the first pass, so a misbehaving key function could duplicate or drop records but never expose uninitialized memory. The cost of storing bucket numbers for every record type is measured in the [port record](port.md#performance).

## Panic safety

Key functions are user code and may panic. On a panic:

- the block partition rebuilds each block's offsets from the bucket fill counts and the number of complete input blocks, then moves every live record back into the input slice;
- a tile keeps a pointer to the complete record set from the last finished pass and copies it back if needed;
- the panic is then resumed.

The slice holds every record afterwards, in an unspecified order. No record is dropped twice or leaked.

Every build catches a key-function panic during block partitioning, restores the complete record set and resumes the panic. There is no alternate abort-only recovery implementation.

## Stack usage

Each tile-local MSD level keeps a cursor table of up to 32 KiB on the stack. The depth is bounded by ⌈key bits / 6⌉, at most 22 levels for 128-bit keys. Actual frame sizes depend on the compiler.
