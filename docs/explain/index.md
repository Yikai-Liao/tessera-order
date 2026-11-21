# How Tessera works

Tessera is a radix sort that stays stable without a second, input-sized buffer and skips work the data does not need. Both demos below run the engine itself, compiled to WebAssembly.

## The textbook: LSD needs an input-sized buffer

<CountingPassDemo />

A textbook LSD radix sort repeats this counting pass once per digit: count each digit value, turn the counts into start positions, then copy every record into its bucket in a second array as large as the input. For 10 million 8-byte records that buffer is 76 MiB. [Radix sort in one pass](radix-sort.md) builds LSD, MSD and Tessera from this pass.

## Tessera: a stable split without a second buffer

<BlockPartitionDemo />

Tessera splits in place. Here the input splits on the tens digit into four buckets. Each bucket fills a chain of fixed-size blocks, and once an input block has been read it becomes free for any bucket's output. A short ledger then copies each chain into its final range. Equal keys keep their input order (the small number under each key), and the only extra memory is a few spare blocks: about 6 MiB instead of 76 MiB for those 10 million records. [The block partition](block-partition.md#step-by-step) explains every row and control.

## The whole engine

<PipelineMap />

1. **Skip what the data does not need.** An order probe returns ascending inputs at once and reverses descending ones stably. Unsorted inputs get an insertion pass with a swap budget, which finishes nearly sorted ones. One scan finds the key bits that vary; if none do, the input is done.
2. **Split large inputs (nodes).** A node takes the first partition that applies: one window covering every varying bit when they span at most 8 bits (counted directly when the records are their own keys), an equality partition when one key is frequent, a split at its top varying bit when it is already grouped by that bit, or otherwise a block partition on its top digit. Each split removes at least one key bit, so recursion never goes deeper than the key is wide.
3. **Route the children again.** A child larger than a tile repeats the order check and the varying-bit scan before it is split, so an ordered or constant region exposed anywhere costs one scan.
4. **Finish cache-sized pieces (tiles) in a scratch buffer**, where a second buffer is cheap: insertion sort, a tile-local MSD split, or LSD counting passes.

Argsort, selection and cached-key sorting reuse the same steps and skip the ones their answer does not need.

## Reading order

| Page | You will learn |
|---|---|
| [1. Radix sort in one pass](radix-sort.md) | Counting passes, LSD versus MSD, and how Tessera combines them |
| [2. Keys as unsigned integers](keys.md) | How signed integers, floats and `Reverse` become radix keys, and why constant bits are free |
| [3. Routing an input](routing.md) | How sorted, nearly sorted, repeated and partly ordered inputs avoid radix passes |
| [4. The block partition](block-partition.md) | The O(√n) stable split, step by step, its parallel form and the block size |
| [5. Finishing tiles](tiles.md) | Choosing digits and the leaf that finishes each cache-sized piece |
| [6. Ownership and panics](safety.md) | Why any record type is safe, and what happens when a key function panics |
| [7. Beyond sorting](operations.md) | How argsort, selection and cached-key sorting skip work a full sort does |

## Memory

Extra memory is O(√n) block scratch plus a fixed-size tile buffer and small tables. [Ordering semantics](../../conformance/semantics.md#space) states the contract; [block scratch](block-partition.md#choosing-the-block-size) and [tile scratch](tiles.md) explain the parts.

These pages own the routing rules, thresholds and their reasons; the [engine reference](../design/engine.md) maps them to code.
