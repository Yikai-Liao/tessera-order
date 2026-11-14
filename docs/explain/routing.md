# 3. Routing an input

Before any radix pass, Tessera checks whether the input needs one. Many real inputs are already sorted, nearly sorted, reversed, constant, or dominated by one key, and a radix sort that ignores this does full passes for nothing. This page follows `sort_with_tile_bytes` and `Sorter::sort_node` in [`engine.rs`](../../rust/tessera-order/src/engine.rs) in order.

## Order probe

`probe_order` samples adjacent pairs at a stride of `max(1, floor(n / 4,096))`. At stride 1 (fewer than 8,192 records) it goes straight to the full scan, `confirm_order`, since sampling every pair first would repeat its work. That scan reads each key once. Its first three keys can already show both a rising and a falling pair, which returns unsorted at once. Otherwise it skips leading equal keys, fixes the direction at the first unequal key, and checks only that direction from there. It returns ascending (sorted or constant keys), strictly descending, descending with ties, or unsorted.

At larger strides, the sample classifies the input:

| Sampled pairs | Class | What happens |
|---|---|---|
| some ascending and some descending | **unsorted** | finish insertion-sized tiles; otherwise try budgeted insertion |
| all equal | **flat** | decide after the varying-bit scan (below) |
| all `≤`, or all `≥` | candidate order | a full scan, `confirm_order`, checks every pair |

A confirmed **ascending** input returns immediately. A strictly descending input is reversed directly. Any leading equal keys skipped while finding the direction rule out strict descent. For other confirmed descending inputs, the engine reverses the slice and then each run of equal keys (`reverse_stably`), because a plain reversal would put equal keys in reverse input order.

The sample rejects most unsorted inputs within a few pairs, so the full scan runs only on inputs that look ordered. Its ascending continuation is the vector scan budgeted insertion uses for direct primitive keys, with a scalar fallback for other keys and short slices; its descending continuation also records whether any adjacent pair ties.

## Budgeted insertion

An unsorted input no longer than the [insertion leaf's threshold](tiles.md#three-leaves) goes straight to an unbudgeted insertion sort: a budget and a varying-bit scan would only lead back to the same leaf. `insertion_probe` owns this check, the probe window and the budget.

A larger unsorted input may still be *nearly* sorted: a sorted log with a few late entries, for example. Insertion sort finishes such an input in linear time, but on a random input it is quadratic. Tessera therefore gives it a **swap budget** and abandons it when the budget runs out.

- If the input fits in one tile, insertion runs on the records with a budget of n swaps.
- Otherwise, the keys of a probe window (one tile's worth of records) are copied into a buffer and insertion-sorted there first. The number of swaps on the window predicts the number on the whole input: the budget for the records is `floor(swaps × n / window)`. The product is computed in a wider integer type, so it cannot overflow before division on a 32-bit target; the resulting budget is at most n. A window that needed no swaps proves nothing, because the inversions are elsewhere, so the radix path continues.

An abandoned insertion pass leaves the records in a different order, but every swap exchanged two adjacent out-of-order records, so equal keys never crossed. The input is still a *stable permutation* of itself, and the radix path sorts it correctly.

## Varying bits and flat inputs

Next, one scan computes the [varying bits](keys.md#varying-bits). If no bit varies, every key is equal and the sort is done. A **flat** probe, where every sampled pair was equal, is then confirmed with a full order scan. The sample could have missed a few differing keys between its pairs.

128-bit keys whose variation fits in the low or high 64 bits continue as 64-bit keys.

Only now is tile and block scratch allocated. Budgeted insertion on a larger unsorted input may already have allocated its probe-key buffer; confirmed ordered and constant inputs never allocate.

## Nodes and tiles

From here on, the engine works on slices of the input and sorts each one in place:

- a **tile** is a slice that fits the cache-resident tile budget, the [size defined on the tiles page](tiles.md). It is finished in a scratch buffer of the same size;
- a **node** is anything larger. It is split by a stable partition into children, and each child is routed again.

A partition child larger than a tile, and every child of a parallel root, enters `sort_child`. It reuses the root’s order probe first: confirmed ascending children return, and descending ones take the same strict or stable reversal, without a varying-bit scan. Unfinished children scan varying bits within the remaining candidate mask; constant children are done. A flat sample with actual variation then receives the deferred full order confirmation, just as at the root. This keeps constant children’s vectorizable scan while saving a full pass over confirmed ordered children. A partition can expose an ordered child anywhere in the input, without an end-run detector or a length cap. Only unordered children continue to `sort_node`, and the analysis allocates nothing. Tile-sized children of a serial partition skip the order probe and go straight to their [tile leaf](tiles.md).

`plan_node` then chooses a node's partition, in this order. If its varying bits span at most 8 bits, one window sorts it completely: a counted window or a block partition of that window.

### Counted window

A record sorted by its own key is interchangeable with every record of the same key: equal encoded keys are equal bit patterns. For those inputs — `stable_sort`, not `stable_sort_by_key` — the engine counts the records of each value in the window and fills each run with the key decoded from that value and the bits all keys share outside the window. The runs come out in key order, with no block partition and no record moves.

Records sorted by a key function keep their payloads: two records with key 7 may hold different data and in a fixed order between them. Counting would drop that, so `stable_sort_by_key` takes the [block partition](block-partition.md) below instead.

### Block partition of the window

Otherwise the window takes a [block partition](block-partition.md), which also finishes the node.

### Repeated key

A wider node of at least four tiles, so that its bounded sample is paid for, samples up to 128 keys, one from each equal stratum. A golden-ratio offset inside each stratum avoids aliasing with periodic data, and locally ordered input still gives independent observations rather than runs of neighbours that share their top digit. Sorting this bounded sample lets both equality detection and digit occupancy use the same extracted keys. If one value fills at least a quarter of the sample, an **equality partition** splits the node into keys below, equal to, and above that value. The equal band is finished, since all of its keys are the same. The same pass also splits whichever outer band crosses the node's highest varying bit, so every remaining band holds that bit constant and is routed again without it. Each record's bucket is its rank (0, 1 or 2) plus that bit, which keeps the four buckets in key order: above the bit all keys agree, so a key below the value cannot have the bit set unless the value does, and a key above it cannot lack the bit if the value has it.

A heavily skewed distribution (Zipf, a default value, a null marker) would otherwise put most records in one bucket on every digit, and that bucket would cost one pass per digit.

Consuming that bit is what bounds the recursion. Without it, an outer band would keep every candidate bit, and an input that places a new, smaller key on exactly the sampled records would let each equality partition peel only those records: linear nesting depth and quadratic work, even with consistent keys. A changing key function could likewise make the sampled value disappear and leave the node unchanged. Every node partition (equality, runs or top digit) removes the top varying bit from each unfinished child, so a node nests at most once per key bit.

### Already in bucket order

A node may already hold the two runs its top varying bit separates. `two_runs` reads that bit and reports whether the node is `0*1*` or `1*0*`; the second form is one rotation away from the first, and both runs keep their own order. Those runs *are* the partition: the bit leaves the candidate and each run is routed again, with no sample and no partition pass. This covers sorted data with a separated block of keys at either end, or any other arrangement the order already groups. The scan stops at the first record that breaks the pattern, so it costs a few key reads when it fails and one pass when it succeeds.

### Top digit

Otherwise the node is split on the digit under its highest varying bit, with the [stable block partition](block-partition.md). `choose_split_digit` picks the width:

1. Aim for children of half a tile: with `target = tile records / 2`, start with enough bits for `n / target` children, clamped to 3–8 bits. Wider splits measured slower on 10 million records because of page and TLB pressure.
2. Project the key sample onto the digit. If it hits fewer than a quarter of the digit's buckets, widen the digit by 2 bits and check again, up to 12 bits. Above 9 bits, 128 keys cannot fill a quarter of the buckets, so widening continues without a check. Widening stops at bits no key varies in: they add empty buckets, and every bucket reserves block scratch, without separating a single key. The equality check's sample is reused; a node below four tiles samples here for the first time.

Serial and parallel roots share this decision. A parallel root shards equality and digit partitions across threads and executes an existing-run split serially, without resampling.

While the block partition completes, each child that fits a tile is finished **at once**, while its records are still in cache. Larger children are routed again after the partition.

## Summary

| Input | Route | Cost |
|---|---|---|
| sorted | probe + one confirming scan | ~1 read pass, no allocation |
| reversed | probe + scan + stable reversal | ~2 passes, no allocation |
| all keys equal | confirmation at unit stride; otherwise flat sample + varying-bit scan | ~1 read pass, no allocation |
| unsorted tile, insertion-sized | insertion finish | bounded quadratic leaf, no allocation |
| larger nearly sorted input | budgeted insertion | linear in n plus inversions |
| one dominant key | equality partition | one partition, the equal band is done |
| ordered or constant child exposed by a partition | full order confirmation, then return or stable reversal | at most linear work in that child |
| node already in two runs of its top varying bit | split at that bit | one scan, no partition pass or scratch |
| at most 8 varying bits, record is its own key | counted window | one counting pass and one fill |
| everything else | top-digit block partition, then tiles | one streaming pass per node level, then cache-resident tiles |
