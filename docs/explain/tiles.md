# 5. Finishing tiles

A **tile** holds at most 64 KiB of records and at most 65,535 records, so that a `u16` can address any position in it. Its scratch buffer has the same size and stays in the L2 cache. Inside a tile a second buffer is cheap, so Tessera uses ordinary counting passes. What remains to decide is which digits to use, and whether to finish with MSD or LSD.

## Three leaves

| Leaf | Used when | How it works |
|---|---|---|
| <span id="insertion">**Insertion sort**</span> | the tile fits the short-leaf limit below | stable insertion on encoded keys |
| <span id="tile-lsd">**Tile LSD**</span> | the [digit planner](#digit-planning) chooses a one- or two-pass flat plan | one counting pass per digit, lowest digit first, alternating between the tile and scratch |
| <span id="tile-msd">**Tile MSD**</span> | otherwise (including the [SIMD MSD override](#digit-planning)) | one counting pass on the top digit, then either a whole-tile finisher (restore-only when every bucket has at most one record, [in-place SIMD](#inside-a-pass) for eligible direct keys within the overlap limit, or one whole-tile insertion sweep when every bucket is insertion-sized) or nested tile leaves per bucket |

For an inline record width of $w$ bytes, the insertion limit is $\max(16, \lfloor 48 \times 16 / (16 + w) \rfloor)$ records. A random permutation of $n$ distinct keys averages $n(n-1)/4$ inversions. Insertion holds each moving record once and shifts each predecessor right, so each inversion still pays a comparison plus movement proportional to $w$ (and counts toward the same logical swap budget) while avoiding repeated three-move swaps; insertion work per record grows roughly as $n(16+w)$. The model charges comparison and loop overhead 16 equivalent bytes and keeps at least 16 records on insertion to amortize radix setup. These are tuning estimates. See [short insertion leaves](../design/evidence.md#short-insertion-leaves).

## Digit planning

A digit's width is limited by its cursor table, one `u16` per bucket. The widest allowed table

- is no larger than the tile itself in bytes (a 14-bit table is already 32 KiB);
- has at most 4 buckets per record, so empty buckets cannot dominate;
- is between 6 and 14 bits wide.

**One or two passes.** If all varying bits fit one digit, one pass finishes the tile. If they fit two, the planner picks the split with the smallest total table. For 20 adjacent varying bits and a 14-bit limit, it uses 10 + 10 bits rather than 14 + 6: both move each record twice, but the balanced tables have 2,048 slots instead of 16,448. Varying bits need not be adjacent: two far-apart varying bits take two one-bit passes. When direct-key SIMD MSD finishing is available on the active CPU backend and one top digit's bucket capacity already provides at least one bucket per record with fewer table slots than the two-pass split, the planner overrides two-pass LSD with that single MSD pass (for example, 25 remaining bits on an 8,000-record `u32` tile choose one MSD pass with SIMD finishing support and two LSD passes without it).

**Otherwise, partition and look.** One MSD pass consumes the highest digit, and each bucket plans again from the bits that actually vary in it. There is no cost estimate: an insertion-sized bucket needs no scan, and a larger one scans its remaining bits and is done if they are constant. Concentrated distributions thus get recursive finishing for free. This can choose MSD where a longer LSD plan would have been faster; [engine evidence](../design/evidence.md#landed-with-open-losses) lists the measured cases.

The figure shows the chosen flat plan, or the first MSD partition without its children's passes.

<DigitPlanner />

| Tile | Varying bits | Decision |
|---|---|---|
| 8,192 random u64 keys | 64 | Partition the highest 14-bit digit, then finish each bucket |
| 8,192 u64 keys in 0..2²⁰ | 20 | Two LSD passes of 10 bits |
| 1,000 u64 keys with a dominant repeated key | broad span | Partition; the dominant key's bucket is constant after its scan |

## Inside a pass

Tile passes are the classic [counting pass](radix-sort.md#the-counting-pass), with three refinements:

- **No pass for keys that are their own records.** A single-digit plan on `sort` first tries the [counted window](routing.md#counted-window): count, then write each run from its count, without scratch or record moves.
- **The key function runs once per pass.** With `sort_by_key`, the histogram stores each record's bucket as a `u16` after the records in scratch, and the scatter reads it back instead of calling the key function again.
- **A guard knows where the records are.** After each pass the complete set is either in the tile or in scratch. If the plan ends in scratch, or the key function panics, the guard copies the records back. [Ownership and panics](safety.md) explains why this matters.

A bucket's remaining varying bits are scanned only within the parent's remaining bits. A digit that has been partitioned cannot reappear, even if the key function changes its answers, so each level removes key bits.

An MSD pass reads its bucket boundaries from the finished cursor table instead of recomputing them; recomputing measured 48% slower at about one bucket per record. When every bucket holds at most one record, restoring from scratch already finishes the tile. For direct primitive keys (including `Reverse` wrappers) on real SIMD backends with at least 8 vector lanes, when the tile has at least `max(64, 2 * lanes)` records and the largest bucket fits within one vector of overlap (`<= lanes + 1`), a branchless odd-even vector network finishes the restored tile in place instead of insertion. Tiles long enough for two vectors per stream (`>= 8 * lanes` records across four streams) repair four independent streams, then their seams and any remaining tail; shorter eligible tiles use one overlapping repair sweep. The counting pass restores records before either finisher starts, so both use the same record ownership and panic recovery as other tile leaves. Otherwise, if every bucket is insertion-sized, one adjacent-inversion sweep over the whole tile finishes it without walking the boundaries.

## Parallel child allowance

A complete child of a parallel root may use a larger tile: up to 1 MiB of records, within the same 65,535-record limit. It applies only when the whole child fits; a larger child keeps the ordinary tile size for its descendants. The budget is empirical, not a cache-size claim.
