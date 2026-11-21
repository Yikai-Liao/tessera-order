# 3. Routing an input

Before any radix pass, Tessera checks whether the input needs one. Many real inputs are already sorted, nearly sorted, reversed, constant, or dominated by one key, and a radix sort that ignores this does full passes for nothing. This page follows root preparation and `Sorter::sort_node` in [`engine/refine.rs`](../../rust/tessera-order/src/engine/refine.rs) in order.

## Order probe

`probe_order` goes straight to the full scan, `confirm_order`, for fewer than 8,192 records. Larger inputs sample adjacent pairs at a stride of `max(128, floor(n / 4,096))`. The three limits serve different purposes. The 128-record minimum stride bounds what an ordered input pays for its sample before the confirming scan. The 4,096-pair target preserves the original sampling density on large inputs, where a sample can catch an unsorted tail before confirmation reads nearly every key. Below 8,192 records, a sample would hold fewer than 64 pairs, so confirmation runs directly. The [shared-order assessment](../design/evidence.md#shared-order-confirmation-for-serial-operations) records the trade-off. The first three keys of the confirming scan can establish a direction or rule out both directions. If they all tie, the last key determines which direction to check. Equal endpoints require every intervening key to match. It returns ascending (sorted or constant keys), strictly descending, descending with ties, or unsorted.

For larger inputs, the sample classifies the input:

| Sampled pairs | Class | What happens |
|---|---|---|
| some ascending and some descending | **unsorted** | finish insertion-sized tiles; otherwise try budgeted insertion |
| all equal | **flat** | decide after the varying-bit scan (below) |
| all `≤`, or all `≥` | candidate order | a full scan, `confirm_order`, checks every pair |

A confirmed **ascending** input returns immediately. A strictly descending input is reversed directly. For other confirmed descending inputs, the engine reverses the slice and then each run of equal keys (`reverse_stably`), because a plain reversal would put equal keys in reverse input order.

The sample rejects most unsorted inputs within a few pairs, so the full scan runs only on inputs that look ordered. Its ascending continuation is the vector scan budgeted insertion uses for direct primitive keys, with a scalar fallback for other keys and short slices. The scalar continuation extracts four successive keys and combines their ordering results before deciding whether to continue, allowing independent comparisons to overlap; an inversion may therefore evaluate the remaining keys in its group before returning. The final short tail still checks one pair at a time. Its descending continuation also records whether any adjacent pair ties.

## Budgeted insertion

An unsorted input no longer than the [small-leaf threshold](tiles.md#three-leaves) goes straight to stable insertion: a budget and a varying-bit scan would only lead back to the same leaf. `insertion_probe` owns this check, the probe window and the budget.

A larger unsorted input may still be *nearly* sorted: a sorted log with a few late entries, for example. Insertion sort finishes such an input in linear time, but on a random input it is quadratic. Tessera therefore gives it a **swap budget** and abandons it when the budget runs out.

- If the input fits in one tile, insertion runs on the records with a budget of `floor(n / 4)` swaps.
- Otherwise, the keys of a probe window (one tile's worth of records) are copied into a buffer and insertion-sorted there first, with a budget of `floor(window / 4)`. The number of swaps on the window predicts the number on the whole input: the budget for the records is `floor(swaps × n / window)`. The product is computed in a wider integer type, so it cannot overflow before division on a 32-bit target; the resulting budget is at most `floor(n / 4)`. A window that needed no swaps proves nothing, because the inversions are elsewhere, so the radix path continues.

The quarter-record allowance limits wasted exchanges before a failed probe falls back to radix sorting. It is an empirical work budget, not a test for any distribution: inputs with more inversions may leave insertion earlier, while insertion-sized leaves still finish without a budget.

An abandoned insertion pass leaves the records in a different order, but every swap exchanged two adjacent out-of-order records, so equal keys never crossed. The input is still a *stable permutation* of itself, and the radix path sorts it correctly.

## Varying bits and flat inputs

Next, one scan computes the [varying bits](keys.md#varying-bits). If no bit varies, every key is equal and the sort is done. A **flat** probe, where every sampled pair was equal, is then confirmed with a full order scan. The sample could have missed a few differing keys between its pairs.

128-bit keys whose variation fits in the low or high 64 bits continue as 64-bit keys.

Only now is tile and block scratch allocated. Budgeted insertion on a larger unsorted input may already have allocated its probe-key buffer; confirmed ordered and constant inputs never allocate.

## Sparse source order

A serial finisher can describe a permutation as long runs of source positions and a few isolated records. After the insertion probe, a node wider than one digit samples the input. If most keys are alike, their occurrences form the ordered background. If most positions are already ordered and every rank is required, the finisher removes both ends of each inversion from a stack of source runs. Each disjoint pair costs at most two exceptions for one record any ordered subsequence must exclude. Selection skips this construction: radix selection discards most buckets after one pass, which on small records costs less than the complete order. Exceeding the square-root exception budget returns control to ordinary partitioning before any records move.

A rank can supply its own boundary. If every key to its right is at least its key *p*, collect the records at most *p* needed to fill the prefix through the rank. Any matches taken from the right have key *p*, so taking them in input order places the correct occurrence. The left-hand bound works in reverse. A short requested end also fits the exception budget. A batch of keyed positions keeps its best half whenever it fills. If the original end keys occur only in the end, the winners and displaced records can exchange slots directly.

Both constructions merge isolated source positions with runs from the background. Reserving a destination band for equal keys establishes their full order. Reserving one position establishes a rank. The mover parks isolated records, copies the longer runs in overlap-safe order, and restores the parked records. Rayon schedules the slice before its serial finisher tries this path.

## Nodes and tiles

From here on, the engine works on subproblems. Full sorting refines every unfinished band; single-rank selection uses the same plan but only refines the band containing the requested rank. A partition always completes stable movement for every band, including those that receive no further key analysis. The [engine reference](../design/engine.md#ordered-rank-requirements) owns projection and workspace invariants.

Requested subproblems continue in place:

- a **tile** is a slice that fits the cache-resident tile budget, the [size defined on the tiles page](tiles.md). It is finished in a scratch buffer of the same size;
- a **node** is anything larger. It is split by a stable partition into children, and each child is routed again.

A partition child larger than a tile, and every child of a parallel root, enters `sort_child`. It reuses the root’s order probe first: confirmed ascending children return, and descending ones take the same strict or stable reversal, without a varying-bit scan. Unfinished children scan varying bits within the remaining candidate mask; constant children are done. A flat sample with actual variation then receives the deferred full order confirmation, just as at the root. This keeps constant children’s vectorizable scan while saving a full pass over confirmed ordered children. A partition can expose an ordered child anywhere in the input, without an end-run detector or a length cap. Only unordered children continue to `sort_node`, and the analysis allocates nothing. Tile-sized children of a serial partition skip the order probe and go straight to their [tile leaf](tiles.md).

`plan_node` then chooses a node's partition, in this order. If its varying bits span at most 8 bits, one window sorts it completely: a counted window or a block partition of that window.

### Counted window

A record sorted by its own key is interchangeable with every record of the same key: equal encoded keys are equal bit patterns. For those inputs — `sort`, not `sort_by_key` — the engine counts the records of each value in the window and fills each run with the key decoded from that value and the bits all keys share outside the window. The runs come out in key order, with no block partition and no record moves.

Records sorted by a key function keep their payloads: two records with key 7 may hold different data and in a fixed order between them. Counting would drop that, so `sort_by_key` takes the [block partition](block-partition.md) below instead.

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

While the block partition completes, each requested child that fits a tile is finished **at once**, while its records are still in cache. Larger requested children are routed again after the partition. A requested tile is fully sorted even when only one rank is needed.

## When the result is a permutation

Argsort expands sparse runs into its existing index buffer. For dense input it counts the highest varying byte and emits each source index into its band in input order. Short bands use source keys directly. For denser bands, spare bits in the index word hold the next key prefix. Within a band the words order by key prefix and then by source position. Only a tied prefix with unresolved bits requires another source-key lookup. Cached-key sorting can apply the resulting cycles or the original sparse runs directly to its payloads.

## Summary

| Input | Route | Cost |
|---|---|---|
| sorted | probe + one confirming scan | ~1 read pass, no allocation |
| reversed | probe + scan + stable reversal | ~2 passes, no allocation |
| all keys equal | direct confirmation on short inputs; otherwise flat sample + varying-bit scan | ~1 read pass, no allocation |
| unsorted tile, insertion-sized | insertion finish | bounded quadratic leaf, no allocation |
| larger nearly sorted input | budgeted insertion | linear in n plus inversions |
| one dominant key | equality partition | one partition, the equal band is done |
| ordered or constant child exposed by a partition | full order confirmation, then return or stable reversal | at most linear work in that child |
| node already in two runs of its top varying bit | split at that bit | one scan, no partition pass or scratch |
| at most 8 varying bits, record is its own key | counted window | one counting pass and one fill |
| everything else | top-digit block partition, then tiles | one streaming pass per node level, then cache-resident tiles |
