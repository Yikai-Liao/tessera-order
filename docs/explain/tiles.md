# 5. Finishing tiles

A tile is a slice of at most 64 KiB of records, and at most 65,535 records, so that one `u16` can hold any position in it. Its scratch buffer is the same size and stays in the L2 cache. Inside a tile a second buffer is cheap, so Tessera can use ordinary counting passes. The open question is how to finish: one counted split, or one flat plan of passes. This page follows `tile::finish` in [`tile.rs`](../../rust/tessera-sort/src/tile.rs) and the model in [`cost.rs`](../../rust/tessera-sort/src/cost.rs).

## Two routes and two leaves

| Route | Used when | How it works |
|---|---|---|
| <span id="insertion">**Insertion**</span> | the tile has at most 48 records, or every bucket of a counted split is 48 records or fewer | stable insertion on encoded keys, over the whole slice: bucket-ordered keys make the separate bucket passes cost the same and the dispatch between them cost nothing |
| <span id="tile-split">**Counted split**</span> | one pass leaves children the leaf finishes | one counting pass on a window at the top varying bit, then each bucket is finished on its own |
| <span id="tile-flat">**Flat plan**</span> | the flat passes are cheaper than a split | one counting pass per planned window, lowest first, alternating between the tile and scratch, and no recursion |

Below 48 records the work of clearing and scanning bucket tables costs more than insertion's shifts. The threshold is `INSERTION_MAX` in `lib.rs`.

## Digit planning

The width of a counting digit is limited by its cursor table, one `u16` per bucket. `table_bits` picks the widest table that

- is no larger than half the tile in bytes (a 14-bit table is already 32 KiB);
- has at most 8 buckets per record, or 4 for records wider than 8 bytes, so clearing and scanning empty buckets cannot dominate. A wider record fits fewer records in the same table budget, so its buckets would be mostly empty;
- is between 6 and 14 bits wide.

When every bit below the top one varies, `plan_flat` tiles the run evenly over the passes that width would need, so 32 bits in three passes become 11 + 11 + 10 rather than 14 + 14 + 4: both plans move each record three times, but the even one clears, counts and walks 3,074 bucket slots instead of 32,784. Gaps between varying bits keep the wide windows, which jump over them.

Toggle bits and change the table width to see the plan. The figure also models the route choice for 8,192 `u64` keys with the selected table limit; the engine itself uses 14 bits at this size. Change the sample's distinct-bucket count to explore the concentrated case:

<DigitPlanner />

## The model prices both

Both routes move records with counting passes, so the model prices them in passes over the tile:

- **Flat** costs one pass per planned window, plus the table each pass walks.
- **Split** costs one pass, then the children it prepares and the leaf it leaves behind. The children come from a sample: 128 evenly spaced keys name the buckets the window occupies, so a spread tile splits once into leaves and a concentrated one would descend over several levels.

The model takes the cheaper route and reports the digit it chose, so the engine executes exactly the plan that was priced. Ties go to the flat plan, which moves each record fewer times.

The choice is remembered when the shape alone settled it: a spread sample leaves nothing for the keys to answer, so the next tile of the same size and varying bits reuses the answer instead of sampling again.
