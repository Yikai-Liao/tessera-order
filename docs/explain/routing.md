# 3. Routing an input

Before any radix pass, Tessera checks whether the input needs one. Many real inputs are already sorted, nearly sorted, reversed, constant, or dominated by one key, and a radix sort that ignores this does full passes for nothing. This page follows `sort_with_tile_bytes` and `Sorter::sort_node` in [`engine.rs`](../../rust/tessera-sort/src/engine.rs) in order.

## Order probe

`probe_order` chooses a stride of `max(1, floor(n / 4,096))`. At stride 1 — fewer than 8,192 records — it classifies order in one adjacent-key scan: sampling every pair and then confirming would repeat the same work. It evaluates each visited record once. Before choosing a directional continuation, it checks up to three keys: three are the minimum that can demonstrate both rising and falling pairs. A direction conflict returns unsorted immediately. If these keys are equal, the scan finds the first unequal key; it then checks only the surviving direction, retaining the previous encoded key and rejecting the first conflict without dispatching a separate ascending scan. A completed scan returns ascending for sorted or constant keys, or distinguishes strict descent from descent with ties.

At larger strides, the adjacent-pair sample classifies the input below. The target of 4,096 determines the stride, not a hard sample limit. An ordered, non-flat candidate takes the stages modeled in [`orderProbe.ts`](../.vitepress/theme/sim/orderProbe.ts).

| Sampled pairs | Class | What happens |
|---|---|---|
| some ascending and some descending | **unsorted** | finish insertion-sized tiles; otherwise try budgeted insertion |
| all equal | **flat** | decide after the varying-bit scan (below) |
| all `≤`, or all `≥` | candidate order | a full scan, `confirm_order`, checks every pair |

A confirmed **ascending** input returns immediately. A strictly descending input is reversed directly. Any leading equal keys skipped while finding the direction rule out strict descent. For other confirmed descending inputs, the engine reverses the slice and then each run of equal keys (`reverse_stably`), because a plain reversal would put equal keys in reverse input order.

At larger strides, the sample rejects most unsorted inputs within a few pairs. The full scan runs only on inputs that look ordered. It finds the first unequal key, which fixes the direction, and then continues in that direction. For an ascending input it reuses the scan that budgeted insertion uses: the vector backend when the key has one, and one pair at a time otherwise. A descending input continues with a scan that also records whether any adjacent pair ties.

## Budgeted insertion

An unsorted input that fits a tile and has at most 48 records is already small enough for the existing insertion leaf. It goes directly to insertion finishing: trying a budget and scanning varying bits would only lead back to the same algorithm. The order probe still runs first, so ascending and descending inputs keep their existing return and reversal paths. The [executable stage model](../.vitepress/theme/sim/rootInsertion.ts) mirrors this choice.

A larger unsorted input may still be *nearly* sorted: a sorted log with a few late entries, for example. Insertion sort finishes such an input in linear time, but on a random input it is quadratic. Tessera therefore gives it a **swap budget** and abandons it when the budget runs out.

- If the input fits in one tile, insertion runs on the records with a budget of n swaps.
- Otherwise, the keys of a probe window (one tile's worth of records, at most 65,535) are copied into a buffer and insertion-sorted there first. The number of swaps on the window predicts the number on the whole input: the budget for the records is `floor(swaps × n / window)`. The product is computed in a wider integer type, so it cannot overflow before division on a 32-bit target; the resulting budget is at most n. A window that needed no swaps proves nothing, because the inversions are elsewhere, so the radix path continues.

An abandoned insertion pass leaves the records in a different order, but every swap exchanged two adjacent out-of-order records, so equal keys never crossed. The input is still a *stable permutation* of itself, and the radix path sorts it correctly.

## Varying bits and flat inputs

Next, one scan computes the [varying bits](keys.md#varying-bits). If no bit varies, every key is equal and the sort is done. A **flat** probe, where every sampled pair was equal, is then confirmed with a full order scan. The sample could have missed a few differing keys between its pairs.

128-bit keys whose variation fits in the low or high 64 bits continue as 64-bit keys.

Only now do the free functions create tile/block scratch. Budgeted insertion on a larger unsorted input may already have allocated the encoded-key probe buffer. Confirmed ordered and constant inputs never allocate.

## Nodes and tiles

From here on, the engine works on slices of the input and sorts each one in place:

- a **tile** is a slice of at most 64 KiB of records. It is finished in a scratch buffer of the same size, which stays in the L2 cache ([tiles](tiles.md));
- a **node** is anything larger. It is split by a stable partition into children, and each child is routed again.

Partition children entering `sort_slice` reuse the full order confirmation described above: the order probe runs before the variation scan, so a child that is ordered or reversed returns without sampling adjacent pairs. An ascending or constant child returns, and a descending child uses the same strict or stable reversal. Thus a radix partition can expose an ordered child even when the full input was not ordered. This applies equally to children from either end or the interior of the input, without an end-run detector or a length cap. Only unordered children scan their remaining candidate bits and continue to the normal routes. The root goes directly to `sort_node`, since it was already probed. The extra check takes at most linear work in the slice length and allocates nothing; it does not run insertion or create a second key buffer.

A node then looks at its varying bits. If they span at most 8 bits, the node is one window away from sorted, and one of the routes below finishes it: every record is then in its final position.

### Counted window

A record sorted by its own key is interchangeable with every record of the same key: equal encoded keys are equal bit patterns. For those inputs — `stable_sort`, not `stable_sort_by_key` — the engine counts the records of each value in the window, remembers one record per value, and fills each run with it. The runs come out in key order, and no block table or record move is needed.

Records sorted by a key function keep their payloads: two records with key 7 may hold different data and in a fixed order between them. Counting would drop that, so `stable_sort_by_key` takes the [block partition](block-partition.md) below instead.

### Block partition of the window

A node that reaches this point gets a [block partition](block-partition.md) on the window, which also finishes the node. Wider nodes have two ways to split. Equality sampling is checked only on nodes of at least four tiles, so its 64-key sample is paid for, and it runs before the digit route below.

### Already in bucket order

A node that reaches its digit partition may already hold the two runs its top varying bit separates, and then it needs no pass. One scan reads that bit and reports whether the node is `0*1*` or `1*0*` in it; the second form is one rotation away from the first, and both runs keep their own order. Those runs *are* the partition, the bit leaves the candidate, and the recursion finishes each run in place — with no detector, no sample and no length cap. This covers sorted data with a separated block of keys at either end, several such blocks, or any other arrangement the order already groups, as long as the node was not already finished by the narrow window or by an equality partition. The scan stops at the first record that breaks the pattern, which unordered records reach within a few keys, so the check costs a handful of key reads when it fails and one pass when it succeeds.

### Repeated key

The node samples 64 keys in 8 runs of 8 consecutive keys. Runs rather than single keys avoid aliasing with periodic data. If one value fills at least a quarter of the sample, an **equality partition** splits the node into three bands: keys below, equal to, and above that value. The equal band is finished, since all of its keys are the same. The other two are routed again.

A heavily skewed distribution (Zipf, a default value, a null marker) would otherwise put most records in one bucket on every digit, and that bucket would cost one pass per digit.

A changing key function can make the sampled value disappear: every record then enters the same non-equal band, leaving the node unchanged. In that case the engine uses a top-digit partition instead. Its digit bits are removed from the next candidate mask even if the records again enter one bucket, so changing keys cannot repeat this node indefinitely.

### Top digit

Otherwise the node is split on a window under its highest varying bit, with the [stable block partition](block-partition.md). The cost model, not a rule, picks the width: it prices every width from one bit to eight bits — beyond that the write cursors, counts and output blocks of the block partition stop fitting in L1 — and takes the cheapest. Each price is one counting pass plus the bucket table, the children the window prepares and the leaf it leaves behind; a wide window leaves children that finish themselves, a narrow one pays fewer table slots.

Two facts the model measures rather than assumes. A sample of 128 evenly spaced keys names the buckets the window occupies; if it repeats buckets far more than a uniform spread would, the keys are concentrated and the sampled buckets are the ones the slice occupies, otherwise the slice spreads over as many buckets as it has records. And the same sample answers every candidate width, so pricing eight widths costs one pass over 128 keys.

Windows always start at the top varying bit, so each pass removes the highest remaining difference, and a split whose width the sample settled from the shape alone is remembered: the next node of the same size and varying bits reuses the answer.

While the block partition writes its result back into the input, each child that fits a tile is finished **at once**, while its records are still in cache. Larger children are routed again after the partition.

## Summary

| Input | Route | Cost |
|---|---|---|
| sorted | probe + one confirming scan | ~1 read pass, no allocation |
| reversed | probe + scan + stable reversal | ~2 passes, no allocation |
| all keys equal | confirmation at unit stride; otherwise flat sample + varying-bit scan | ~1 read pass, no allocation |
| unsorted tile, at most 48 records | insertion finish | bounded quadratic leaf, no allocation |
| larger nearly sorted input | budgeted insertion | linear in n plus inversions |
| one dominant key | equality partition | one partition, the equal band is done |
| ordered or constant child exposed by a partition | full order confirmation, then return or stable reversal | at most linear work in that child |
| node already in two runs of its top varying bit | split at that bit | one scan, no partition pass or scratch |
| at most 8 varying bits, record is its own key | counted window | one counting pass and one fill |
| everything else | top-digit block partition, then tiles | one streaming pass per node level, then cache-resident tiles |
