# How Tessera works

These pages explain the algorithm from first principles: what a radix sort does, why the textbook versions need either a second buffer or give up stability, and how Tessera avoids both. Each page has interactive figures. They run TypeScript models of the Rust source, and those models are tested against reference sorts. The text says everything the figures show, so you can read the Markdown files in [`docs/explain/`](.) without the site.

For the engine's exact decisions and constants, see the [engine design](../design/engine.md). These pages explain the reasons behind those decisions.

## The problem in one paragraph

A radix sort reads digits of the key instead of comparing keys, so it runs in time linear in n for a fixed key width. The stable textbook version, LSD radix sort, moves every record into a second buffer as large as the input on every pass. The in-place textbook version, American flag sort (an MSD radix sort), needs no buffer, but its swaps break stability. Tessera stays stable and linear and uses O(√n) extra memory: for 10 million 8-byte records, about 1.2 MiB instead of a 76 MiB buffer.

## The idea in three sentences

1. Split large inputs on their **top digit** with a stable **block partition** that recycles consumed input blocks as output blocks. It needs only 2R spare blocks and a small block table (R is the number of buckets).
2. Once a bucket fits a **64 KiB tile**, finish it in a cache-resident scratch buffer with the cheapest leaf: insertion sort, a tile-local MSD partition, or LSD counting passes.
3. Before any of that, **skip work the data does not need**: sorted or reversed inputs, nearly sorted inputs, key bits that never change, heavily repeated keys and short outlier runs.

## The pipeline

<PipelineMap />

The figure, as a list:

1. **Order probe.** Sample adjacent pairs. Ascending inputs return at once. Descending inputs are reversed stably.
2. **Budgeted insertion.** Unsorted inputs get an insertion pass with a swap budget. Nearly sorted inputs finish here.
3. **Varying bits.** One scan finds the bits that differ between keys. If none do, the input is done. Later digits cover only the varying bits.
4. **Node or tile.** An input larger than 64 KiB of records is a *node*, and anything smaller is a *tile*.
5. **Node partitions.** A node is split by a repeated-key partition, an outlier-run lift or a top-digit block partition. Each child is routed again.
6. **Tile leaves.** A tile is finished by insertion sort, a tile-local MSD partition or LSD counting passes.

## Reading order

| Page | You will learn |
|---|---|
| [1. Radix sort in one pass](radix-sort.md) | Counting passes, LSD versus MSD, and a side-by-side comparison with Block LSD and Tessera |
| [2. Keys as unsigned integers](keys.md) | How signed integers, floats and `Reverse` become radix keys, and why constant bits are free |
| [3. Routing an input](routing.md) | The order probe, budgeted insertion, nodes and tiles, repeated keys and outlier runs |
| [4. The block partition](block-partition.md) | The O(√n) stable partition, step by step, and how the block size is chosen |
| [5. Finishing tiles](tiles.md) | Digit planning, the cost model, and the three tile leaves |
| [6. Ownership and panics](safety.md) | Why any record type is safe, and what happens when a key function panics |

## Memory at a glance

| Part | Size | Where |
|---|---|---|
| Block partition | O(√n) records of scratch and block tables, for a fixed record size and digit width | heap |
| Tile scratch | at most 64 KiB of records, at least one record | heap |
| Cached bucket IDs | up to 2 bytes per tile record (`stable_sort_by_key` only) | heap |
| Probe keys | at most 65,535 encoded keys | heap |
| Cursor tables | up to 32 KiB per tile-local MSD level, at most 22 levels for 128-bit keys | stack |

[`conformance/semantics.md`](../../conformance/semantics.md#space) states these bounds as part of the contract.
