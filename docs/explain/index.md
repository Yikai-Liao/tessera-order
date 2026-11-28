# How Tessera works

Tessera is a stable radix ordering engine that runs in O(√n) extra memory instead of an O(n) output buffer, and skips passes whenever the input is already ordered, constant, narrow, or dominated by a repeated key.

## Engine route & scale explorer

Select a 10-million-record workload below to see which stages run, which are skipped, and how much extra memory Tessera uses compared with a textbook LSD radix buffer. Click any box in the pipeline to open its chapter.

<PipelineMap />

### How the four stages work

1. **Skip what the data does not need.** An [order probe](routing.md#order-probe) returns ascending inputs at once and reverses descending ones stably without allocating. Unsorted inputs try [budgeted insertion](routing.md#budgeted-insertion), which finishes nearly sorted slices in linear time. One scan then finds the [varying key bits](keys.md#varying-bits); if none vary, the slice is done.
2. **Split large inputs (nodes) in $O(\sqrt{n})$ scratch.** A node takes the first partition that applies: a [counted window](routing.md#counted-window) for a narrow direct-key range, an [equality partition](routing.md#repeated-key) when sampling finds a dominant key, a zero-copy split when the slice is already grouped by its top varying bit, or a stable [block partition](block-partition.md) on its top digit. Each split removes at least one key bit, bounding recursion depth by the key width.
3. **Route large children again.** Every child larger than a tile repeats the order check and varying-bit scan over the remaining bits, so ordered or constant regions exposed inside a bucket finish in one scan.
4. **Finish cache-sized tiles.** Once a subproblem fits in a [tile](tiles.md), a scratch buffer sits in L2 cache and ordinary counting passes are cheap: insertion sort for short tiles, one- or two-pass flat LSD when remaining bits fit, or tile-local MSD otherwise.

Argsort, selection and cached-key sorting reuse the same pipeline and skip every stage their answer does not need; see [7. Beyond sorting](operations.md).

## Reading order

Each chapter includes interactive models or WebAssembly replays of the engine itself:

| Page | You will learn |
|---|---|
| [1. Radix sort in one pass](radix-sort.md) | Counting passes, LSD versus MSD, and how Tessera combines them (interactive counting-pass demo) |
| [2. Keys as unsigned integers](keys.md) | How signed integers, floats and `Reverse` become radix keys, and why constant bits are free |
| [3. Routing an input](routing.md) | How sorted, nearly sorted, repeated and partly ordered inputs avoid radix passes |
| [4. The block partition](block-partition.md) | The $O(\sqrt{n})$ stable split, WebAssembly step-by-step replay, parallel form, and block-size derivation |
| [5. Finishing tiles](tiles.md) | Choosing digits and the leaf that finishes each cache-sized piece |
| [6. Ownership and panics](safety.md) | Why any record type is safe, and what happens when a key function panics |
| [7. Beyond sorting](operations.md) | How argsort, selection and cached-key sorting skip work a full sort does |

## Memory

Extra memory is $O(\sqrt{n})$ block scratch plus a bounded tile buffer and small tables. [Ordering semantics](../../conformance/semantics.md#space) states the formal contract; [choosing the block size](block-partition.md#choosing-the-block-size) and [finishing tiles](tiles.md) explain each component.

These pages own the routing rules, thresholds and their reasons; the [engine reference](../design/engine.md) maps them to code.
