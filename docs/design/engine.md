# Design

This document describes the engine in `rust/tessera-sort`. It makes the same decisions as the research prototype (`sort-research-rs`, commit `6c0877f`); the [port record](port.md) lists how the port reaches them differently.

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

The engine samples the input to classify it as ascending, descending, flat or unsorted. An ascending input returns immediately. A descending input is reversed stably: runs of equal keys keep their internal order. An unsorted input gets a budgeted stable insertion pass. When the probe window is shorter than the input, the pass first runs on a copy of the window's keys to estimate the number of swaps, and only then touches the records. If the budget runs out, the input is left as a stable permutation of itself and the radix path continues.

### 2. Varying bits

One scan computes the bitwise union and intersection of all keys. Bits that are equal in every key cannot change the order and are excluded from every later digit. A constant input returns at this point. Wide keys whose variation fits in one 64-bit half are sorted by that half as a 64-bit key.

### 3. Node partitioning

A node is split by one of three partitions, all of them stable:

- **Repeated key.** The node samples 64 keys in runs of 8. If one value covers at least a quarter of the sample, an equality partition separates it from the rest, so heavily repeated keys stop costing passes.
- **Outlier run.** If a short run at either end of the node (at most 1,024 records) disagrees with all other records on the highest varying bit, the run is split off and finished by insertion sort, and the rest of the node keeps a narrower set of varying bits.
- **Top digit.** Otherwise the node is split on its top digit. The initial width is 8 bits and may grow to 12; a sample of 128 keys checks that the digit actually divides the range.

The digit partition uses the block LSD scheme of Clausecker and Schintke. Buckets are filled block by block, consumed input blocks are reused as output blocks, and permutation metadata records their logical order. Stability holds without a full-size output buffer, and the extra space is O(√n) for a fixed digit width and record size. The block size in records is

```
max(1, ceil(sqrt(10n / (2 R s))), floor(1024 / s), 32)
```

where `n` is the input length, `s` the record size and `R` the number of buckets.

The root is the paper's trade-off evaluated for this implementation. A block size `b` costs `2 R b s` bytes of scratch — one output block per bucket, plus as many in reserve, because an input block may only be recycled once every record in it has been read — and `10 n / b` bytes of tables, because a block position owns two `u32` block-order entries and one `u16` bucket id. The two terms are equal at that root, so it is the minimum of the sum: substituting `b = c b*` multiplies the total by `(c + 1/c) / 2`, which grows in both directions. No scale of the root is worth its memory, and the formula takes the minimum.

The lower bounds are not from the paper. The compaction pass moves a block with one copy, so a block holds at least a kilobyte of records; and the bookkeeping of a block does not shrink with the record, so a block holds at least 32 records whatever their width. Their cost does not depend on `n`, so the O(√n) bound holds for a fixed record size and digit width.

Buckets that fit a tile are finished as tiles during compaction. Larger buckets become nodes and are partitioned again.

### 4. Tile finishing

A tile is finished by one of three leaves:

- **Insertion sort** for tiles and buckets of at most 48 records.
- **Tile-local MSD partition**, recursing into buckets with a cursor table on the stack.
- **LSD counting passes** over a scratch buffer, with digit widths between 6 and 14 bits.

The leaf and the digit widths are chosen by a cost model that counts passes and charges for bucket tables. An MSD level is charged as one counting pass plus one pass for the insertion finish that ends the descent.

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

## Stack usage

Each tile-local MSD level keeps a cursor table of up to 32 KiB on the stack. The depth is bounded by ⌈key bits / 6⌉, at most 22 levels for 128-bit keys. Actual frame sizes depend on the compiler.
