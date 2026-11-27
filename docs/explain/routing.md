# 3. Routing an input

**The core idea:** real datasets are often sorted, nearly sorted, reversed, constant, or dominated by one value. Paying for full radix passes on easy inputs wastes memory bandwidth, so Tessera tests cheap structural explanations first and falls back to radix partitioning only on what remains.

| Input shape | Route taken | Cost |
|---|---|---|
| Already ascending | Order probe → confirming scan | ~1 read pass, zero allocation |
| Descending (strict or with ties) | Order probe → scan → stable reversal | ~2 passes, zero allocation |
| All keys equal | Varying-bit scan (`varying == 0`) | ~1 read pass, zero allocation |
| Nearly sorted | Budgeted insertion | Linear in $n$ plus inversions |
| Few misplaced keys or one dominant key | Sparse source order | Sorts only exceptions; moves each run once |
| ≤8 varying bits (direct keys) | Counted window | 1 count pass + 1 fill, zero moves |
| One frequent key (≥25% of sample) | Equality partition | 1 pass; equal band is finished |
| Already grouped by top varying bit | Top-bit split (with optional rotation) | 1 scan, zero partition pass |
| Everything else | Top-digit [block partition](block-partition.md), then [tiles](tiles.md) | 1 streaming pass per level, then L2 tiles |

## Order probe

Proving an input is sorted requires reading every key, which random data should never pay for. Tessera therefore samples evenly spaced adjacent pairs first: a random slice hits both an ascending and a descending pair almost immediately and bails out in nanoseconds. Only when every sampled pair agrees does a full scan confirm the order.

- **Ascending:** done immediately.
- **Strictly descending:** reverse the slice in place.
- **Descending with ties:** reverse the slice, then reverse each equal-key run back so equal keys preserve their original input order.
- **Flat sample (all sampled pairs equal):** deferred until after the varying-bit scan, which catches all-equal inputs in one pass and runs a full order check if any bit varies.

## Budgeted insertion

A timestamp log with a few late arrivals is not sorted, yet insertion sort finishes it in near-linear time. The danger is quadratic time on unsorted inputs — so Tessera runs insertion sort with a strict **swap budget** and abandons it the moment the budget runs out.

Why giving up mid-pass is always safe: every insertion shift only moves a smaller key left past strictly larger keys, so equal keys never cross. An abandoned pass leaves a different, still-valid stable permutation that the radix engine finishes normally.

## Varying bits and flat inputs

After the order and insertion checks, one scan finds the [varying bits](keys.md#varying-bits). If none vary, the slice is done. If the earlier order probe saw only equal pairs while some bit varies, a full order scan runs now so an ordered slice with sparse steps still takes the zero-allocation shortcut. Scratch memory is allocated only after these checks pass, so sorted, reversed, and constant inputs allocate nothing.

## Sparse source order

When almost every record is already in order (a few scattered inversions) or almost every record shares one dominant key, moving the entire slice through radix passes does unnecessary work. Instead, Tessera decomposes the input into long **monotone runs** that stay in source order plus a small list of **exceptions**, sorts only the exceptions by `(key, source_index)`, and merges them between the runs in one pass.

- **Dominant key:** when nearly all sampled keys equal the middle record's key, every other record becomes an exception while the dominant key's records stay in input order between the smaller and larger exceptions.
- **Few inversions (full sort only):** scan left to right; whenever a key drops below the previous kept key, pull both records into the exception list. Removing both handles the ambiguity of which one was misplaced while removing at most twice the minimum needed.
- **Bounded fallback:** exceptions are capped at $O(\sqrt{n})$ and every key is scanned before any record moves, so exceeding the cap falls back cleanly to normal partitioning with nothing to undo. Selection skips the inversion form because one radix partition already discards ~99% of buckets ([7. Beyond sorting](operations.md#select-one-rank)).

## Nodes and tiles

Everything that survives the shortcuts is handled recursively by size:

- A **tile** fits inside the 64 KiB L2 budget and finishes directly in tile scratch ([5. Finishing tiles](tiles.md)).
- A **node** is anything larger. Tessera applies the first partition rule below that fits, then routes each resulting child again. Every node split removes at least the top varying bit from every unfinished child, bounding recursion depth by the key width even if a user key function changes its answers.

### Counted window

When all varying bits span at most 8 bits and records are their own keys (`sort`), equal keys are bit-identical. Tessera counts each value in the window and writes each run directly from its count and the shared constant bits — zero record movement. Keyed records (`sort_by_key`) carry payloads, so they take one [block partition](block-partition.md) over the window instead.

### Repeated key

Skewed distributions (Zipf, nulls, default values) would otherwise keep funneling most records through the same bucket on every digit pass. When a bounded sample finds one key occupying at least a quarter of the node, an **equality partition** splits the node into keys below, equal to, and above that value, finishing the equal band in one pass. To guarantee depth stays bounded by key width even on adversarial inputs, the same pass also splits whichever outer band crosses the node's highest varying bit — giving four ordered buckets where every unfinished child has lost its top bit.

### Already in bucket order

If a quick scan finds the node already consists of two runs separated by its top varying bit (`0*1*`, or `1*0*` fixed by one rotation), those runs *are* the partition. The scan stops on the first violation, so random inputs pay only a few reads.

### Top digit

Otherwise, the node splits on its top digit using the [stable block partition](block-partition.md). Each tile-sized child finishes immediately while its records are still warm in cache; larger children repeat the order probe and varying-bit scan over their remaining bits so ordered or constant sub-regions are caught anywhere in the tree.

## Exact rules & thresholds

| Stage | Rule / constant |
|---|---|
| Order probe | Samples ~4,096 evenly spaced adjacent pairs with stride ≥ 128; inputs `< 8,192` records skip sampling and scan directly. |
| Budgeted insertion | Unsorted slices within the [insertion leaf cutoff](tiles.md#three-leaves) finish unconditionally; single-tile inputs get `n / 4` swaps; larger inputs insertion-sort a copied 1-tile key window and scale its swaps to `n` (capped at `n / 4`; a 0-swap window falls through to radix because inversions may lie outside the window). |
| Sparse source order | Dominant-key check samples 128 keys around the midpoint; exception buffer capped at $O(\sqrt{n})$. |
| Repeated key | Samples up to 128 keys on nodes of ≥ 4 tiles using golden-ratio stratum offsets (avoiding periodic aliasing); triggers equality partition when one value fills ≥ 25% of the sample. The sorted sample is reused for digit widening. |
| Top-digit width | Aims for half-tile children (`n / (tile_records / 2)`), clamped to `3–8` bits (wider splits measured slower at 10M due to TLB pressure). If the 128-key sample hits `< 25%` of buckets, widens by 2 bits at a time up to `12` bits (above 9 bits, widening continues without the occupancy check), stopping at non-varying bits. |
| Child routing | Children larger than one tile repeat order probe + candidate-masked varying-bit scan; tile-sized serial children go straight to their [tile leaf](tiles.md). Serial and parallel roots share one planner decision; parallel roots shard equality and digit partitions and run existing-run splits serially. |

Next: [4. The block partition](block-partition.md) shows how a node splits stably in $O(\sqrt{n})$ extra memory.
