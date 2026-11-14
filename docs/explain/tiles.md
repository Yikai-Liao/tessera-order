# 5. Finishing tiles

A tile is a slice of at most 64 KiB of records, and at most 65,535 records, so that one `u16` can hold any position in it. Its scratch buffer is the same size and stays in the L2 cache. Inside a tile a second buffer is cheap, so Tessera uses ordinary counting passes; the questions are which digits to use and whether to finish with MSD or LSD. [`tile/plan.rs`](../../rust/tessera-order/src/tile/plan.rs) selects the leaf; [`tile.rs`](../../rust/tessera-order/src/tile.rs) executes it and owns its scratch and panic recovery.

## Parallel child allowance

A complete child of a parallel root may use a larger tile: up to 1 MiB of records, within the same cursor limit. `finishing_capacity` owns this allowance. It applies only when the whole child fits; a larger child keeps the ordinary capacity for its descendants. The budget is empirical, not a cache-size claim. Child variation and order confirmation still run first.

## Three leaves

| Leaf | Used when | How it works |
|---|---|---|
| <span id="insertion">**Insertion sort**</span> | the tile has at most 48 records | stable insertion on encoded keys |
| <span id="tile-msd">**Tile MSD**</span> | more than two width-bounded passes would be needed to finish flat | one counting pass on the top digit through scratch, then each bucket is finished on its own: insertion for buckets of at most 48 records, another tile leaf for larger ones |
| <span id="tile-lsd">**Tile LSD**</span> | otherwise | one counting pass per planned digit, from the lowest digit up, alternating between the tile and scratch |

Below 48 records, the work of clearing and scanning bucket tables costs more than insertion's shifts. The threshold is `thresholds::INSERTION_MAX` in `lib.rs`.

## Unstable small leaves

The same small-leaf threshold also bounds comparison finishing for an unstable call. Both direct and callback keys use the standard library's `sort_unstable_by_key` on encoded keys. The standard sorter owns its algorithm and panic recovery; Tessera adds no key cache, index permutation, record guard or threshold. If a callback changes its answer, comparisons may no longer form a total order and the standard sorter may panic. The [key-extraction contract](../../conformance/semantics.md#key-extraction) permits that outcome while requiring every original record to remain in the slice.

Only terminal small finishing changes. Budgeted insertion still exploits nearly ordered inputs; LSD digit passes remain stable; and a tile whose MSD buckets are all insertion-sized still uses one whole-tile insertion pass to avoid walking its bucket table. Finishing every such bucket with a separate small sort added table-walk work and was rejected in the [evidence](../design/evidence.md#unstable-node-partition-feasibility). Figures on this page continue to model stable leaves.

## Digit planning

The width of a counting digit is limited by its cursor table, one `u16` per bucket. `tile_digit_bits` picks the widest table that

- is no larger than the tile itself in bytes (a 14-bit table is already 32 KiB);
- has at most 4 buckets per record, so clearing and scanning empty buckets cannot dominate. Records of at most 8 bytes already meet this through the byte limit; wider records fit fewer records in the same table budget, so the record limit decides for them;
- is between 6 and 14 bits wide.

The planner first tries to finish in one or two flat passes. One window suffices when all varying bits fit its width. Otherwise, it takes the widest top window and checks whether the remaining bits fit another. If they do, it searches the legal split points for the smallest total bucket table. At most 14 split points need consideration; no dynamic-programming table or sample of records is needed.

For example, 20 adjacent varying bits and a 14-bit limit use 10 + 10 bits, rather than 14 + 6. Both plans move each record twice, but balanced tables use 2,048 slots rather than 16,448. The rule also handles shifted runs and gaps: two far-apart varying bits need two one-bit passes.

When two windows cannot finish the tile, one MSD pass consumes the highest digit. Each child then chooses its own plan from its actual remaining variation. There is no predicted population, singleton assumption or recursive sample model. The figure shows the chosen flat plan or the first MSD partition; an MSD partition's child passes are not included in its table total.

<DigitPlanner />

## Actual children instead of predicted work

An MSD partition preserves the savings a cost estimate would predict. An insertion-sized child needs no variation scan. A larger child scans its remaining bits and is done if they are constant. Node partitions finish their tile-sized children by the same rule (`finish_child`). A child that still has many varying bits can partition again; each partition removes its digit from the candidate mask. Concentrated distributions therefore keep recursive finishing without a recursive cost estimate.

This simpler routing may choose MSD where a longer LSD plan would be faster. [Engine evidence](../design/evidence.md#landed-with-open-losses) lists the measured trade-offs; removing an estimate is not proof that its choices never helped.

| Tile | Varying bits | Decision |
|---|---|---|
| 8,192 random u64 keys | 64 | Partition the highest 14-bit digit, then finish actual children |
| 8,192 u64 keys in 0..2²⁰ | 20 | Two LSD passes of 10 bits |
| 1,000 u64 keys with a dominant repeated key | broad span | Partition; constant children finish after their variation scan |

## Inside a pass

Tile passes are the classic [counting pass](radix-sort.md#the-counting-pass) with two details:

- **Bucket IDs are cached** when the key comes from a user function: the histogram stores each record's bucket as a `u16`, and the scatter reads it back instead of calling the key function again. The IDs go after the records in the scratch allocation. [Ownership and panics](safety.md) explains why.
- **A guard tracks the live copy.** After each pass, `RestoreTile` remembers whether the complete record set is in the tile or in scratch. If the plan ends in scratch, or a key function panics, it copies the records back into the tile.

Before recursing, each child is scanned for varying bits only within the parent's remaining candidate mask. Already partitioned digits cannot reappear, even if a user key function changes its answers, so each recursive level removes key bits.

An MSD tile pass reads its bucket boundaries from the finished cursor table instead of recomputing them from keys. Recomputing measured 48% slower when there was about one bucket per record. If every bucket is small enough for insertion, one insertion sort over the whole tile does the same work without walking the boundaries.
