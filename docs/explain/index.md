# How Tessera works

Tessera is a stable radix ordering engine that runs in O(√n) extra memory instead of an O(n) output buffer, and skips passes whenever the input is already ordered, constant, narrow, or dominated by a repeated key.

## Engine route & scale explorer

Select a 10-million-record workload below to see which stages run, which are skipped, and how much extra memory Tessera uses compared with a textbook LSD radix buffer. Click any box in the pipeline to open its chapter.

<PipelineMap />

### How the four stages work

1. **Skip what the data does not need.** An [order probe](routing.md#order-probe) returns ascending inputs at once and reverses descending ones stably without allocating. Unsorted inputs try [budgeted insertion](routing.md#budgeted-insertion), which finishes nearly sorted slices in linear time. One scan then finds the [varying key bits](keys.md#varying-bits); if none vary, the slice is done.
2. **Split large inputs (nodes) in $O(\sqrt{n})$ scratch.** A node takes the first partition that applies: a [counted window](routing.md#counted-window) when varying bits span at most 8 bits on direct keys, an [equality partition](routing.md#repeated-key) when one key fills at least a quarter of a 128-key sample, a zero-copy split when the slice is already grouped by its top varying bit, or a stable [block partition](block-partition.md) on its top digit. Each split removes at least one key bit, bounding recursion depth by the key width.
3. **Route large children again.** Every child larger than a 64 KiB tile repeats the order check and varying-bit scan over the remaining bits, so ordered or constant regions exposed inside a bucket finish in one scan.
4. **Finish cache-sized tiles in 64 KiB scratch.** Once a subproblem fits in a [tile](tiles.md), a scratch buffer sits in L2 cache and ordinary counting passes are cheap: insertion sort for short tiles, one- or two-pass flat LSD when remaining bits fit, or tile-local MSD otherwise.

### Workload routes at 10 million records

| Workload | Route taken | What is skipped | Extra memory (vs LSD buffer) |
|---|---|---|---|
| **10M random `u64`** | Order probe → varying bits → 8-bit block partition → tile MSD / insertion | Early exits, equality partition | ~3.6 MiB (`b = 714` + 64 KiB tile) vs 76.3 MiB |
| **10M nearly sorted `u64`** | Order probe → budgeted insertion → done | Varying-bit scan and all radix passes | 64 KiB probe copy vs 76.3 MiB |
| **10M reversed `u64` + ties** | Order probe → in-place stable reversal → done | Insertion, varying-bit scan, all radix passes | 0 B vs 76.3 MiB |
| **10M Zipf / repeated key** | Sampled equality partition (`< / = / >`) → refine non-equal bands | Re-scattering the dominant key across remaining digits | ~446 KiB for the equality split (up to ~3.6 MiB on sub-bands) vs 76.3 MiB |
| **10M 20-bit narrow keys** | Varying-bit scan → 8-bit block partition → 12-bit tile LSD | 44 constant high key bits | ~3.6 MiB vs 76.3 MiB |
| **10M `argsort` (128B rows)** | Top-digit band split + key bits packed into 64-bit index words | Moving 1.2 GiB of source records | 76.3 MiB index output + ~3.6 MiB workspace vs 1.2 GiB full-record buffer |
| **10M median `select_nth`** | Root block partition → refine only the bucket containing rank 5,000,000 | Refining ~255 non-target buckets (>99% of the slice) | ~3.6 MiB (no output buffer) vs 76.3 MiB |

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

Extra memory is $O(\sqrt{n})$ block scratch plus a fixed-size 64 KiB tile buffer and small tables. [Ordering semantics](../../conformance/semantics.md#space) states the formal contract; [choosing the block size](block-partition.md#choosing-the-block-size) and [finishing tiles](tiles.md) explain each component.

These pages own the routing rules, thresholds and their reasons; the [engine reference](../design/engine.md) maps them to code.
