# How Tessera works

Tessera sorts stably, in linear time, with O(√n) extra memory. Below, its core step runs in the engine itself, compiled to WebAssembly.

## Tessera: a stable split without a second buffer

<BlockPartitionDemo />

The input splits on the tens digit of two-digit keys into four buckets. Each bucket fills a chain of fixed-size blocks, and once an input block has been read it becomes free for any bucket's output. A short ledger then copies each chain into its final range. Equal keys keep their input order (the small number under each key), and the only extra memory is a few spare blocks. [The block partition](block-partition.md#step-by-step) explains every row, label and control.

## The textbook: LSD needs an input-sized buffer

<CountingPassDemo />

A textbook LSD radix sort repeats this counting pass once per digit: count each digit value, turn the counts into start positions, then copy every record into its bucket in a second array as large as the input. For 10 million 8-byte records that buffer is 76 MiB; Tessera needs about 6 MiB. [Radix sort in one pass](radix-sort.md) builds LSD, MSD and Tessera from this pass and compares them on one input.

## The idea in three sentences

1. Split large inputs on their **top digit** with a stable **block partition** that recycles consumed input blocks as output blocks. It needs only 2R spare blocks and a little metadata per block (R is the number of buckets).
2. Once a bucket fits a **cache-resident tile**, finish it in a cache-resident scratch buffer with the cheapest leaf: insertion sort, a tile-local MSD partition, or LSD counting passes.
3. Before any of that, **skip work the data does not need**: sorted, reversed or nearly sorted inputs, key bits that never change, and heavily repeated keys. Partition children get the same order check.

## The pipeline

<PipelineMap />

The figure, as a list (the rules, thresholds and reasons for each stage are on [routing](routing.md), [the block partition](block-partition.md) and [tiles](tiles.md)):

1. **Order probe.** Sample adjacent pairs. Ascending inputs return at once, descending inputs reverse stably.
2. **Budgeted insertion.** Unsorted inputs get an insertion pass with a swap budget, so nearly sorted inputs finish here.
3. **Varying bits.** One scan finds the key bits that differ. If none do, the input is done; later digits cover only those bits.
4. **Node or tile.** A slice larger than a tile is a *node*, anything smaller a *tile*. A child larger than a tile scans its remaining bits and confirms its order again before routing.
5. **Node partitions.** A node with a repeated key takes an equality partition, a node already in two runs of its top bit is split there without moving records, and the rest take a top-digit block partition. Tile-sized children finish as soon as the partition completes their band.
6. **Tile leaves.** A tile is finished by insertion sort, a tile-local MSD partition or LSD counting passes.

## Reading order

| Page | You will learn |
|---|---|
| [1. Radix sort in one pass](radix-sort.md) | Counting passes, LSD versus MSD, and a side-by-side comparison with Block LSD and Tessera |
| [2. Keys as unsigned integers](keys.md) | How signed integers, floats and `Reverse` become radix keys, and why constant bits are free |
| [3. Routing an input](routing.md) | The order probe, budgeted insertion, nodes and tiles, ordered children and repeated keys |
| [4. The block partition](block-partition.md) | The O(√n) stable partition, step by step, its parallel form, and how the block size is chosen |
| [5. Finishing tiles](tiles.md) | Digit planning, the cost model, and the three tile leaves |
| [6. Ownership and panics](safety.md) | Why any record type is safe, and what happens when a key function panics |

## Memory

Memory is O(√n) block scratch plus a fixed-size tile buffer and small tables, all on the heap except the tile-local cursor tables. [Ordering semantics](../../conformance/semantics.md#space) states the contract, and each part is explained where it arises: [block scratch](block-partition.md#choosing-the-block-size), [tile scratch and cursor tables](tiles.md).

These pages own the routing rules, thresholds and their reasons; the [engine reference](../design/engine.md) owns code boundaries and invariants.
