# 5. Finishing tiles

A tile is a slice of at most 64 KiB of records, and at most 65,535 records, so that one `u16` can hold any position in it. Its scratch buffer is the same size and stays in the L2 cache. Inside a tile, a second buffer is cheap, so Tessera can use ordinary counting passes. The open questions are which digits to use and whether to finish with MSD or LSD. This page follows `Sorter::sort_tile` in [`engine.rs`](../../rust/tessera-sort/src/engine.rs).

## Three leaves

| Leaf | Used when | How it works |
|---|---|---|
| <span id="insertion">**Insertion sort**</span> | the tile has at most 48 records | stable insertion on encoded keys |
| <span id="tile-msd">**Tile MSD**</span> | the cost model expects an MSD descent to beat the LSD plan | one counting pass on the top digit through scratch, then each bucket is finished on its own: insertion for buckets of at most 48 records, `sort_tile` again for larger ones |
| <span id="tile-lsd">**Tile LSD**</span> | otherwise | one counting pass per planned digit, from the lowest digit up, alternating between the tile and scratch |

Below 48 records, the work of clearing and scanning bucket tables costs more than insertion's shifts. The threshold is `INSERTION_MAX` in `lib.rs`.

## Digit planning

The width of a counting digit is limited by its cursor table, one `u16` per bucket. `tile_digit_bits` picks the widest table that

- is no larger than the tile itself in bytes (a 14-bit table is already 32 KiB);
- has at most 8 buckets per record, or 4 for records wider than 8 bytes, so clearing and scanning empty buckets cannot dominate. A wider record fits fewer records in the same table budget, so its buckets would be mostly empty;
- is between 6 and 14 bits wide.

`plan_digits` then covers the [varying bits](keys.md#varying-bits) with windows of at most that width, highest window first. Two rules shape the plan:

1. **Windows skip constant bits.** A window starts at the highest remaining varying bit and is cut back to its lowest varying bit, so a pass never ends on bits that cannot change. Gaps between varying bits are jumped over.
2. **Contiguous runs are tiled evenly.** If every bit below the top varying bit varies, the run is split into equal windows over the same number of passes. 32 bits with 14-bit tables become 11 + 11 + 10 instead of 14 + 14 + 4. Both plans move each record three times, but the even one clears and scans 5,120 bucket slots instead of 32,784.

Toggle bits and change the table width to see both rules:

<DigitPlanner />

## MSD or LSD: the cost model

Both leaves move records with counting passes, so the model counts passes over the tile:

- **LSD** costs one pass per planned digit: `passes`.
- **MSD** costs one pass per level it descends before the largest bucket falls to 48 records or fewer, plus one for the insertion finish at the bottom: `depth + 1`.

To estimate the depth, the engine samples 128 keys on the top digit and counts distinct digit values. If every sampled key landed in a different bucket, the sample is too small to measure the spread, and the full bucket count is assumed. Otherwise the number of distinct values is the expected fan-out. The descent divides the tile size by the fan-out until it reaches 48.

MSD wins when `depth + 1 < passes`. Plans of one or two passes skip the sample: one MSD level plus its finish already costs two passes.

Charging an MSD level as more than one pass kept three-pass plans on LSD, which was 11–38% slower for 32-bit keys at 1K–64K records (`MSD_FINISH_PASSES` in `engine.rs`).

Two examples, for 8-byte records (`u64`):

| Tile | Varying bits | LSD plan | Top-digit sample | Decision |
|---|---|---|---|---|
| 8,192 random keys | 64 | 14-bit tables, tiled evenly: 13 + 13 + 13 + 13 + 12, 5 passes | 127 or 128 distinct 14-bit values: 1 or 2 levels | MSD: at most 2 + 1 = 3 < 5 |
| 8,192 keys in 0..2²⁰ | 20 | 10 + 10, 2 passes | skipped | LSD |

## Inside a pass

Tile passes are the classic [counting pass](radix-sort.md#the-counting-pass) with two details:

- **Bucket IDs are cached** when the key comes from a user function: the histogram stores each record's bucket as a `u16`, and the scatter reads it back instead of calling the key function again. The IDs go into the unused part of the cursor table when they fit, or after the records in the scratch allocation. [Ownership and panics](safety.md) explains why.
- **A guard tracks the live copy.** After each pass, `RestoreTile` remembers whether the complete record set is in the tile or in scratch. If the plan ends in scratch, or a key function panics, it copies the records back into the tile.

An MSD tile pass reads its bucket boundaries from the finished cursor table instead of recomputing them from keys. Recomputing measured 48% slower when there was about one bucket per record. If every bucket is small enough for insertion, one insertion sort over the whole tile does the same work without walking the boundaries.
