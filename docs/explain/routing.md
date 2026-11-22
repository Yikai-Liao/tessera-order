# 3. Routing an input

Many real inputs are already sorted, nearly sorted, reversed, constant or dominated by one key. A radix sort that ignores this makes full passes for nothing. Tessera checks the cheap explanations first and pays for radix passes only on what is left.

| Input | Route | Cost |
|---|---|---|
| sorted | order probe, one confirming scan | ~1 read pass, no allocation |
| reversed | order probe, scan, stable reversal | ~2 passes, no allocation |
| all keys equal | varying-bit scan | ~1 read pass, no allocation |
| nearly sorted | budgeted insertion | linear in n plus inversions |
| a few records out of place, or one dominant key | sparse source order | sorts only the exceptions; each run moves once |
| one frequent key | equality partition | one partition; the equal band is done |
| already grouped by the top bit | split at that bit | one scan, no record moves |
| at most 8 varying bits, the record is its own key | counted window | one counting pass and one fill |
| everything else | top-digit block partition, then tiles | one streaming pass per level, then cache-resident tiles |

## Order probe

**Problem.** Sorted and reversed inputs are common, but proving order takes a full scan, which an unsorted input should not pay for.

**Idea.** Sample about 4,096 adjacent pairs, evenly spaced. If the sample contains both an ascending and a descending pair, the input is unsorted, and a random input is rejected after a few pairs. Only if every sampled pair agrees does a full scan check every pair. Inputs below 8,192 records skip the sample, which would hold fewer than 64 pairs, and scan directly.

The sample stride is at least 128 records, which bounds what an ordered input pays for sampling before its confirming scan. On large inputs, 4,096 pairs are dense enough to catch an unsorted tail before the confirmation has read nearly every key.

**Result.**

- **Ascending:** done.
- **Strictly descending:** reverse the slice.
- **Descending with ties:** reverse the slice, then reverse each run of equal keys back, so equal keys keep their input order.
- **Every sampled pair equal (flat):** decided after the [varying-bit scan](#varying-bits-and-flat-inputs).

## Budgeted insertion

**Problem.** A sorted log with a few late entries is not sorted, but almost. Insertion sort finishes it in linear time; on a random input, it is quadratic.

**Idea.** Run insertion sort with a **swap budget** and give up when the budget runs out.

- A serial refinement of at most 16 records that fits in one tile goes directly to single-record insertion. Larger unordered roots within the [short-leaf limit](tiles.md#three-leaves) use pair insertion. It shifts a predecessor larger than both new keys by two slots in one move. Both paths finish without a budget.
- An input that fits in one [tile](tiles.md) gets one swap per four records.
- A larger input first insertion-sorts a copy of one tile's worth of keys. The swaps it needed, scaled to the whole input, become the budget, capped at one swap per four records. A window that needed no swaps proves nothing, because the inversions may be elsewhere, so the radix path continues.

The quarter-record allowance is a work budget, not a test for any distribution: it limits how much a failed attempt can waste.

**Why it is safe to give up.** Every swap exchanged two adjacent records that were out of order, so equal keys never crossed. An abandoned pass leaves a different but still stable arrangement, and the radix path sorts it correctly.

## Varying bits and flat inputs

One scan computes the [varying bits](keys.md#varying-bits). If no bit varies, every key is equal and the sort is done. If the order probe saw only equal pairs but some bit varies, a full order scan runs now: the sample may have missed a few differing keys between its pairs, and the input may still be sorted.

A 128-bit key whose variation fits in its low or high 64 bits is sorted as a 64-bit key.

Scratch is allocated only after these checks, so sorted, reversed and constant inputs never allocate.

## Sparse source order

**Problem.** Some inputs are in order except for a few records, or consist mostly of one key with a few others. Their sorted output is nearly the input itself, yet a radix partition moves every record once per digit.

**Idea.** Describe the output as long **runs** of records that stay in source order, plus a few **exceptions**. Sort only the exceptions, merge them between the runs, and move each run once.

It applies when one digit cannot finish the node, and in two forms:

- **One dominant key.** A sample of 128 keys nearly all equals the middle record's key. The records with other keys are the exceptions; the dominant key's records stay in source order between the smaller and the larger exceptions.
- **A few inversions** (full sort only). Scan left to right. Whenever a key is smaller than the last kept one, remove both records. For `1 2 3 9 4 5 6`, the drop from `9` to `4` removes both, leaving runs `1 2 3` and `5 6` and exceptions `4, 9`. The merge gives `1 2 3 4 5 6 9`. Removing both is necessary because the scan cannot tell which one is misplaced; but one of the two must be, so this removes at most twice the minimum.

**Why it is stable.** Exceptions are sorted by key and then by source position, and each is merged before the first run record that it precedes in that order.

**Budget.** The number of exceptions is capped at O(√n), within the engine's memory bound. Every key is read before any record moves, so exceeding the cap falls back to partitioning with nothing to undo.

Selection does not try the inversion form: radix selection discards most buckets after one pass, which is cheaper than building the complete order. Its own shortcuts are on [Beyond sorting](operations.md#select-one-rank).

## Nodes and tiles

From here on, the engine works on subproblems:

- a **tile** fits the [cache-resident tile budget](tiles.md) and is finished in a scratch buffer of the same size;
- a **node** is anything larger. A stable partition splits it into children, and each child is routed again.

A child larger than a tile repeats the order probe and the varying-bit scan within the bits that are left. A partition can therefore expose an ordered or constant region anywhere in the input, and that region costs one scan. Tile-sized children of a serial partition skip the probe and go straight to their [tile leaf](tiles.md).

A node takes the first partition that applies, in the order below. Every node partition removes at least the top varying bit from each unfinished child, so a node nests at most once per key bit, even if the key function changes its answers.

### Counted window

If the node's varying bits span at most 8 bits, one window finishes it.

When the records are the keys themselves (`sort`, not `sort_by_key`), two records with equal keys are identical bit patterns. The engine counts each value in the window and writes each run from its count: the key is rebuilt from the window value and the bits all keys share. Nothing is moved.

Records sorted by a key function carry payloads, so equal keys are not interchangeable; they take one [block partition](block-partition.md) of the window instead.

### Repeated key

**Problem.** A skewed distribution (Zipf, a default value, a null marker) puts most records in one bucket on every digit, and that bucket costs one pass per digit.

**Idea.** A node of at least four tiles, large enough to pay for it, samples up to 128 keys, one from each equal stratum. If one value fills at least a quarter of the sample, an **equality partition** splits the node into keys below, equal to, and above that value. The equal band is finished.

**Bounding the depth.** The same pass also splits the outer band that crosses the node's highest varying bit, so every remaining band has that bit constant. Without this, an adversarial input could place a new, smaller key on exactly the sampled records at every level: each partition would peel off only those records, giving linear depth and quadratic work. Removing a bit per level caps the depth at the key width.

The resulting four buckets stay in key order: all keys agree above that bit, so within the crossing band the bit separates smaller keys from larger ones.

The stratum offsets follow the golden ratio so that the sample does not alias with periodic data, and the sorted sample is reused to choose the top digit below.

### Already in bucket order

A node may already consist of two runs separated by its top varying bit: all keys with the bit clear, then all with it set, or the reverse. Those runs *are* the partition. The reverse form needs one rotation. The bit leaves the candidates and each run is routed again, with no sample and no partition pass. A scan detects this; it stops at the first record that breaks the pattern, so a failed check costs a few key reads.

### Top digit

Otherwise the node is split on the digit under its highest varying bit, with the [stable block partition](block-partition.md). The width is chosen in two steps:

1. Aim for children of half a tile: enough bits for `n / (tile records / 2)` children, clamped to 3–8 bits. Wider splits measured slower on 10 million records because of page and TLB pressure.
2. Check the key sample against the digit. If it hits fewer than a quarter of the digit's buckets, the keys are concentrated: widen by 2 bits and check again, up to 12 bits. Beyond 9 bits, 128 keys cannot fill a quarter of the buckets, so widening continues without the check. Widening stops at bits no key varies in, which would add empty buckets without separating any keys.

While the partition completes, each child that fits a tile is finished at once, while its records are still in cache. Larger children are routed again afterwards.

Serial and parallel roots make the same decision. A parallel root shards equality and digit partitions across threads and splits existing runs serially.
