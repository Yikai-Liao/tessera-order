# 5. Finishing tiles

A **tile** holds at most 64 KiB of records and at most 65,535 records, so that a `u16` can address any position in it. Its scratch buffer has the same size and stays in the L2 cache. Inside a tile a second buffer is cheap, so Tessera uses ordinary counting passes. What remains to decide is which digits to use, and whether to finish with MSD or LSD.

## Three leaves

| Leaf | Used when | How it works |
|---|---|---|
| <span id="insertion">**Insertion sort**</span> | the tile fits the short-leaf limit below | stable insertion on encoded keys |
| <span id="tile-lsd">**Tile LSD**</span> | the varying bits fit in one or two counting passes | one counting pass per digit, lowest digit first, alternating between the tile and scratch |
| <span id="tile-msd">**Tile MSD**</span> | otherwise | one counting pass on the top digit, then each bucket is finished on its own: by insertion if it fits the short-leaf limit, otherwise by another tile leaf |

For a record occupying $w$ bytes in the slice, the insertion limit in records is

$$
L(w) = \operatorname{clamp}\!\left(\left\lfloor \frac{2048}{w} \right\rfloor, 16, 48\right).
$$

Only the inline record width counts. For example, moving a string handle leaves its heap allocation in place. Zero-sized records finish before this decision.

A random permutation of $n$ distinct keys has $n(n-1)/4$ inversions on average. Inserting two records together can resolve two inversions with one shift, but movement per input record still grows roughly as $n w$. In one radix pass, classification and counting cost roughly the same per record at any payload width, and scattering moves each record once. Where repeated shifts compete with classification, doubling the width therefore roughly halves the length at which insertion pays. The 2 KiB coefficient calibrates that crossover in units of length times width. A sort may move the same bytes many times.

The ceiling of 48 limits quadratic comparison work even when shifts are cheap. The floor of 16 keeps small leaves on insertion, where setting up the radix pass and moving each wide record once can already cost more. The two count bounds and the byte coefficient are tuning parameters. The measured crossovers vary with key distribution and the machine. See the [short-leaf measurements](../design/evidence.md#short-insertion-leaves).

## Digit planning

A digit's width is limited by its cursor table, one `u16` per bucket. The widest allowed table

- is no larger than the tile itself in bytes (a 14-bit table is already 32 KiB);
- has at most 4 buckets per record, so empty buckets cannot dominate;
- is between 6 and 14 bits wide.

**One or two passes.** If all varying bits fit one digit, one pass finishes the tile. If they fit two, the planner picks the split with the smallest total table. For 20 adjacent varying bits and a 14-bit limit, it uses 10 + 10 bits rather than 14 + 6: both move each record twice, but the balanced tables have 2,048 slots instead of 16,448. Varying bits need not be adjacent: two far-apart varying bits take two one-bit passes.

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

An MSD pass reads its bucket boundaries from the finished cursor table instead of recomputing them; recomputing measured 48% slower at about one bucket per record. If every bucket is insertion-sized, one adjacent-inversion sweep over the whole tile finishes it without walking the boundaries.

## Parallel child allowance

A complete child of a parallel root may use a larger tile: up to 1 MiB of records, within the same 65,535-record limit. It applies only when the whole child fits; a larger child keeps the ordinary tile size for its descendants. The budget is empirical, not a cache-size claim.
