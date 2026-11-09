# Design

This document describes the engine in `rust/tessera-sort`. It describes current routing and invariants; the [measurement record](port.md) covers the initial port and subsequent measured changes.

## Vocabulary

| Term | Meaning |
|---|---|
| Key | The unsigned integer a record sorts by. Signed integers, floats and `Reverse` keys are encoded into unsigned keys whose integer order matches the requested order. |
| Digit | A window of key bits handled by one pass, described by a shift and a width. |
| Node | A slice larger than the tile budget. Nodes are split by a stable block partition. |
| Tile | A slice that fits the tile budget, 64 KiB of records by default and at most 65,535 records. Tiles are finished in scratch memory. |
| Cursor table | One `u16` position per bucket. At 14 bits it occupies 32 KiB. |

## Pipeline

### 1. Order probe

The engine chooses a stride of `max(1, floor(n / 4,096))`. At unit stride it classifies order in one adjacent-key scan: first reject a mixed direction among up to three keys, the shortest possible witness, then find the direction past any leading ties and check it while retaining the previous encoded key. It returns unsorted at the first conflict without a separate ascending scan; a constant slice returns as ascending. At larger strides it samples adjacent pairs to classify the input as ascending, descending, flat (every sampled pair equal) or unsorted. The 4,096-pair target determines the stride, not a hard sample limit. Unless unsortedness is found early, the loop visits `floor((n - 1) / stride)` pairs. An ascending or descending sample is confirmed by a full scan, which takes its direction from the first unequal key. A confirmed ascending input returns immediately. A confirmed descending input is reversed stably. Strictly descending keys need only the whole-slice reversal; otherwise each run of equal keys is reversed back. A flat sample is confirmed after the varying-bit scan below, since a constant input stops there.

An unsorted input that fits a tile and has at most 48 records goes directly to insertion finishing, using the existing leaf threshold without a budget or varying-bit scan. Confirmed ordered inputs still finish in the order probe. A larger unsorted input gets a budgeted stable insertion pass. When the probe window, one tile of records, is shorter than the input, the pass first runs on a buffered copy of the window's keys to estimate the number of swaps, and only then touches the records with a budget scaled to the whole input. The scaling uses a wide intermediate product so that a valid budget cannot overflow on 32-bit targets. Each call owns this encoded-key buffer and releases it before the radix path continues. If the budget runs out, the input is left as a stable permutation of itself and the radix path continues. See [Insertion and scans](#insertion-and-scans) for how the pass finds inversions.

### 2. Varying bits

One scan computes the bitwise union and intersection of all keys. Bits that are equal in every key cannot change the order and are excluded from every later digit. A constant input returns at this point. Wide keys whose variation fits in one 64-bit half are sorted by that half as a 64-bit key. Tile and block scratch are created only after these root fast paths; budgeted insertion may already have allocated probe keys. Confirmed ordered and constant inputs do not allocate.

### 3. Node partitioning

Partition children entering `sort_child` first reuse the root order probe. Confirmed ascending records need no further work; confirmed descending records use the same strict or stable reversal as the root. This includes children reached after digit and equality partitions. The root enters `sort_node` directly because its order has already been probed; it does not repeat the gate. Unsorted or flat samples continue to the existing tile and radix routes. Sampling and confirmation take at most linear work per routed slice, use no allocation, and do not move records unless descending order has been confirmed. Tile-sized buckets finished during block compaction still go directly to the tile leaf.

A node whose remaining variation spans at most eight bits is one window away from sorted. When the records are their own keys (`Direct`), it is finished by counting that window: one `usize` count and one remembered record per bucket, then each run is filled from its record. Every sealed encoding is a bijection on bit patterns, so equal keys mean equal records and a filled run is the same permutation the block partition would produce, without block tables or record moves. A user key function keeps payloads that its key does not identify, so `ByKey` declines and takes the partition. The eight-bit cap bounds the stack table to 256 counts and records. The window must cover every remaining key difference or filling would merge distinct records.

Otherwise the node is split by one of two stable partitions. Equality sampling is tried only on nodes of at least four tiles, which amortize their 64-key sample:

- **Repeated key.** The node samples 64 keys in runs of 8. If one value covers at least a quarter of the sample, an equality partition separates it from the rest, so heavily repeated keys stop costing passes. If changing keys leave the entire node below or above that sampled value, a digit partition consumes candidate bits instead of repeating an unchanged node.
- **Top digit.** Otherwise [`choose_split_digit`](../../rust/tessera-sort/src/engine.rs) targets children of half a tile. It starts with enough bits for `n / target` children, clamped to 3–8 bits and the available key bits. A sample of 128 keys checks whether at least a quarter of the digit's buckets are occupied; a concentrated digit widens by two bits at a time, up to 12 or the available key bits.

The digit partition uses the block LSD scheme of Clausecker and Schintke. Buckets are filled block by block, consumed input blocks are reused as output blocks, and permutation metadata records their logical order. Stability holds without a full-size output buffer, and the extra space is O(√n) for a fixed digit width and record size. The block size in records is

```
max(1, 4 * ceil(sqrt(10n / (2 R s))), floor(1024 / s), 32)
```

where `n` is the input length, `s` the record size and `R` the number of buckets. The no-std build computes the equivalent size with integer `isqrt` and ceiling correction; tests check square boundaries and agreement with the measured std formula.

The root is the paper's trade-off evaluated for this implementation. A block size `b` costs `2 R b s` bytes of scratch — one output block per bucket, plus as many in reserve, because an input block may only be recycled once every record in it has been read — and `10 n / b` bytes of tables, because a block position owns two `u32` block-order entries and one `u16` bucket id. The two terms are equal at that root, so it is the minimum of the sum: substituting `b = c b*` multiplies the total by `(c + 1/c) / 2`, which grows in both directions. The engine takes four times that root: the minimum is where memory is smallest, not where the sort is fastest, and a larger block amortises the random read that compaction pays per block.

The lower bounds are not from the paper. The compaction pass moves a block with one copy, so a block holds at least a kilobyte of records; and the bookkeeping of a block does not shrink with the record, so a block holds at least 32 records whatever their width. Their cost does not depend on `n`, so the O(√n) bound holds for a fixed record size and digit width.

Buckets that fit a tile are finished as tiles during compaction. Larger buckets become nodes and are partitioned again.

### 4. Tile finishing

A tile is finished by one of three leaves:

- **Insertion sort** for tiles and buckets of at most 48 records.
- **Tile-local MSD partition**, recursing into buckets with a cursor table on the stack.
- **LSD counting passes** over a scratch buffer, with digit widths between 6 and 14 bits.

The leaf and the digit widths are chosen by a cost model that counts passes and charges for bucket tables. For plans longer than two passes, a sample of at most two distinct top-digit buckets selects MSD directly; otherwise an MSD level is charged as one counting pass plus one pass for the insertion finish that ends the descent. Child variation scans return only bits in the remaining candidate mask, so even an inconsistent key function cannot restore a processed digit and prevent recursion from making progress.

## Insertion and scans

Every insertion pass, whether a budgeted probe or the finish of a tile or bucket, runs one loop, `insertion_loop`: it scans for the first adjacent pair whose keys descend, moves the later record left, and resumes the scan after it. A probe returns its swap count and may stop at its budget. A finishing pass, `finish_insertion`, has no budget and returns `()`, so the unused count is discarded inside the inlined loop rather than returned across a target-feature boundary.

The scan depends on the extractor and the build:

| Pass | Default build | `portable-simd` |
|---|---|---|
| Key function (`stable_sort_by_key`) | One record at a time, keeping the previous key, so the key function runs once per scanned record | Same |
| Direct keys, budgeted probe | Blocked scan of 32 pairs per mask update (`scan::first_inversion`); `f64` uses the scalar scan | Vector scan, with the instruction set selected once per pass |
| Direct keys, insertion finish | Scalar scan: the short ordered runs a radix partition leaves do not amortize a block mask ([measurements](port.md#scalar-insertion-finishing)) | Vector scan, with the instruction set selected once per pass |
| Direct keys, ascending confirmation | The same scan as a budgeted probe, after checking the encoded predecessor | The vector scan, dispatched once |

With `portable-simd`, slices shorter than 64 records and 128-bit and pointer-sized keys scan one pair at a time, and `Reverse<K>` keys keep the default-build scans. The insertion loop is inlined into the dispatched function, so its scan runs with the selected target features.

Dispatch stays inside one insertion pass: per-scan dispatch repeats overhead on short runs, and moving a SIMD token outside its target-feature context loses the intended code generation. SIMD is optional and requires `std` (MSRV in the feature dependency); the default build keeps its lower MSRV and no-std support. User-key tiles retain scalar callbacks rather than materializing a second key array. The [exploratory SIMD measurements](port.md#exploratory-simd-constraints) preserve the evidence for these constraints but were not the full paired protocol and establish no formal speedup claim.

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

Without the `std` feature, a panic cannot be caught. A key-function panic during block partitioning, where the input slice does not own every record, aborts through an unwind guard; panics elsewhere unwind with every record in place. A no-std library may run in an unwinding host; a documented abort requirement alone would not protect the safe API. The guard aborts before restoration can use an incomplete ownership directory.

## Stack usage

Each tile-local MSD level keeps a cursor table of up to 32 KiB on the stack. The depth is bounded by ⌈key bits / 6⌉, at most 22 levels for 128-bit keys. Actual frame sizes depend on the compiler.
