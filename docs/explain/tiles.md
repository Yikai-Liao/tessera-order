# 5. Finishing tiles

A tile is a slice of at most 64 KiB of records, and at most 65,535 records, so that one `u16` can hold any position in it. Its scratch buffer is the same size and stays in the L2 cache. Inside a tile, a second buffer is cheap, so Tessera can use ordinary counting passes. The open questions are which digits to use and whether to finish with MSD or LSD. Strategy selection belongs to [`engine/plan.rs`](../../rust/tessera-sort/src/engine/plan.rs); [`engine/tile.rs`](../../rust/tessera-sort/src/engine/tile.rs) executes the selected leaf and owns its scratch and panic recovery.

## Three leaves

| Leaf | Used when | How it works |
|---|---|---|
| <span id="insertion">**Insertion sort**</span> | the tile has at most 48 records | stable insertion on encoded keys |
| <span id="tile-msd">**Tile MSD**</span> | the LSD plan needs more than two passes, and estimated partition-plus-child work is smaller | one counting pass on the top digit through scratch, then each bucket is finished on its own: insertion for buckets of at most 48 records, `sort_tile` again for larger ones |
| <span id="tile-lsd">**Tile LSD**</span> | otherwise | one counting pass per planned digit, from the lowest digit up, alternating between the tile and scratch |

Below 48 records, the work of clearing and scanning bucket tables costs more than insertion's shifts. The threshold is `INSERTION_MAX` in `lib.rs`.

## Digit planning

The width of a counting digit is limited by its cursor table, one `u16` per bucket. `tile_digit_bits` picks the widest table that

- is no larger than the tile itself in bytes (a 14-bit table is already 32 KiB);
- has at most 8 buckets per record, or 4 for records wider than 8 bytes, so clearing and scanning empty buckets cannot dominate. A wider record fits fewer records in the same table budget, so its buckets would be mostly empty;
- is between 6 and 14 bits wide.

The planner covers varying bit positions with nonoverlapping windows no wider than that limit. It minimizes the number of record passes first, then the total bucket slots among equally short plans. A bounded search over at most 128 positions finds the cheapest lower-bit prefix for every legal next window. Dense, shifted and sparse variation follow the same rule; constant gaps only occupy a window when that avoids a pass or reduces total table work.

For example, 32 adjacent varying bits use widths 11 + 11 + 10 instead of 14 + 14 + 4, whether those bits start at bit zero or above a constant low part. Both plans move each record three times, but balancing clears and scans 5,120 bucket slots instead of 32,784. Sparse inputs can choose different window boundaries under the same objective.

Leaf selection needs only the minimum pass count, which greedy width-bounded coverage computes cheaply. The engine optimizes table sizes only if it selects LSD; an MSD tile does not construct an unused flat plan. The plan capacity is derived from the widest key and narrowest allowed digit rather than an independently tuned array length.

Toggle bits and change the table width to explore the plan. The figure also models the finish choice for 8,192 `u64` keys with the selected table limit; the engine itself uses 14 bits at this size. Change the sampled key pattern to compare constant children with children that still have varying bits:

<DigitPlanner />

## MSD or LSD: the cost model

Both leaves move records with counting passes. The model compares their estimated record-pass work rather than predicting cycles:

- **LSD** touches every record once per planned digit.
- **MSD** partitions every record once, then charges each sampled group for the work remaining in that child. Group populations weight the sum, so a small expensive child does not stand in for the whole tile.

The planner reuses the bounded key sample: up to 128 keys, one per equal stratum of the slice, sorted. Adjacent keys with the same top digit form a group. A repeated group's fraction of the sample estimates its child length; differences among its complete keys estimate the remaining varying bits, restricted to the candidate mask. An estimated child of at most 48 records costs one insertion finish. A larger child costs one variation scan plus the cheaper of minimum flat passes and another sampled partition on its remaining bits. The recursive estimate reuses the group's sampled keys; it never samples the records again. Every estimated partition consumes a digit, bounding its depth by key width. If all sampled keys in the group are equal, the continuation costs only the confirming scan.

A singleton provides no evidence of constancy. Its estimate retains all remaining candidate bits and assumes a uniform child length of `ceil(records / digit buckets)`. This assumption can underestimate an unsampled concentration, just as repeated groups can miss rare variation. These errors affect routing only: actual child scans still decide completion and recursion still consumes candidate bits.

The same comparison covers two, three or many sample-constant groups; there is no separate low-fanout rule. The minimum estimate is one partition plus one child finish or confirmation, so plans of one or two flat passes skip sampling. Larger plans choose MSD only when its weighted estimate is strictly smaller. Table width separately bounds storage and empty-bucket work; the model does not convert table entries into record passes through fitted cycle weights.

Three examples, for 8-byte records (`u64`):

| Tile | Varying bits | LSD plan | Top-digit sample | Decision |
|---|---|---|---|---|
| 8,192 random keys | 64 | 14-bit tables, tiled evenly: 13 + 13 + 13 + 13 + 12, 5 passes | mostly singleton groups; a repeated group may retain lower-bit work | MSD: estimated child work is weighted by population rather than applied to the whole tile |
| 8,192 keys in 0..2²⁰ | 20 | 10 + 10, 2 passes | skipped | LSD |
| 1,000 `u64` keys with a dominant repeated key | broad span | six passes | a large sample-constant group and small remaining groups | MSD: the dominant group needs confirmation, not more sorting passes |

## Inside a pass

Tile passes are the classic [counting pass](radix-sort.md#the-counting-pass) with two details:

- **Bucket IDs are cached** when the key comes from a user function: the histogram stores each record's bucket as a `u16`, and the scatter reads it back instead of calling the key function again. The IDs go into the unused part of the cursor table when they fit, or after the records in the scratch allocation. [Ownership and panics](safety.md) explains why.
- **A guard tracks the live copy.** After each pass, `RestoreTile` remembers whether the complete record set is in the tile or in scratch. If the plan ends in scratch, or a key function panics, it copies the records back into the tile.

Before recursing, each child is scanned for varying bits only within the parent's remaining candidate mask. Already partitioned digits cannot reappear, even if a user key function changes its answers, so each recursive level removes key bits.

An MSD tile pass reads its bucket boundaries from the finished cursor table instead of recomputing them from keys. Recomputing measured 48% slower when there was about one bucket per record. If every bucket is small enough for insertion, one insertion sort over the whole tile does the same work without walking the boundaries.
