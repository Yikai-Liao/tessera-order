# Design

This document describes the Tessera engine as it exists in the research prototype (`sort-research-rs`, commit `6c0877f`). The port to this repository keeps the algorithm unchanged; differences introduced by the port will be listed here.

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

One scan computes the bitwise union and intersection of all keys. Bits that are equal in every key cannot change the order and are excluded from every later digit. A constant input returns at this point. Wide keys whose variation fits in one 64-bit half are sorted through a narrowed 64-bit view.

### 3. Node partitioning

A node is split by one of three partitions, all of them stable:

- **Repeated key.** The node samples 64 keys in runs of 8. If one value covers at least a quarter of the sample, an equality partition separates it from the rest, so heavily repeated keys stop costing passes.
- **Outlier run.** If a short run at either end of the node (at most 1,024 records) disagrees with all other records on the highest varying bit, the run is split off and finished by insertion sort, and the rest of the node keeps a narrower set of varying bits.
- **Top digit.** Otherwise the node is split on its top digit. The initial width is 8 bits and may grow to 12; a sample of 128 keys checks that the digit actually divides the range.

The digit partition uses the block LSD scheme of Clausecker and Schintke. Buckets are filled block by block, consumed input blocks are reused as output blocks, and permutation metadata records their logical order. Stability holds without a full-size output buffer, and the extra space is O(√n) for a fixed digit width and record size. The block size is

```
max(1, ceil(4 * sqrt(9n / (2 R s))), floor(1024 / s))
```

records, where `n` is the input length, `s` the record size and `R` the number of buckets. The factor 4 and the 1 KiB floor were tuned experimentally for 8-bit digits and are not taken from the paper.

Buckets that fit a tile are finished as tiles during compaction. Larger buckets become nodes and are partitioned again.

### 4. Tile finishing

A tile is finished by one of three leaves:

- **Insertion sort** for tiles and buckets of at most 48 records.
- **Tile-local MSD partition**, recursing into buckets with a cursor table on the stack.
- **LSD counting passes** over a scratch buffer, with digit widths between 6 and 14 bits.

The leaf and the digit widths are chosen by a cost model that counts passes and charges for bucket tables. An MSD level is charged as one counting pass plus one pass for the insertion finish that ends the descent.

## Record handling

Records are moved by bitwise copies between the input and `MaybeUninit<T>` scratch. Every record has exactly one live owner at any time; other physical copies are stale bytes that are never dropped, borrowed or returned.

- **Plain-data path** (`stable_sort_by_key`, `T: Copy`). Scatter recomputes each record's key instead of storing its bucket. The first pass of a call that needs more scratch classifies with stored bucket numbers so that every scratch slot holds a valid record; later passes and tiles of the same call reuse that initialized prefix. A key function that returns different keys on different calls can then duplicate or drop records, but never expose uninitialized memory.
- **Owned path** (`stable_sort_by_key_owned`, any `T`). The histogram stores a two-byte bucket number per record, and scatter reads it without calling the key function again. Bucket ranges are disjoint, so each output slot is written exactly once whatever the key function does. This makes `String`, `Box<T>` and `&mut T` records safe.

The research prototype selected the path with specialization on `Copy`. This repository selects it by entry point, because stable Rust cannot specialize.

## Panic safety

Key functions are user code and may panic. On a panic:

- the block partition rebuilds each block's offsets from the bucket fill counts and the number of complete input blocks, then moves every live record back into the input slice;
- a tile keeps a pointer to the complete record set from the last finished pass and copies it back if needed;
- the panic is then resumed.

The slice holds every record afterwards, in an unspecified order. No record is dropped twice or leaked.

## Stack usage

Each tile-local MSD level keeps a cursor table of up to 32 KiB on the stack. The depth is bounded by ⌈key bits / 6⌉, at most 22 levels for 128-bit keys. Actual frame sizes depend on the compiler.
