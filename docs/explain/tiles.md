# 5. Finishing tiles

**The core idea:** once a node splits the input into buckets small enough to fit inside a **64 KiB tile**, the whole working set stays in L2 cache. Inside L2, a 64 KiB second buffer is practically free — so Tessera switches back to ordinary counting passes and picks the cheapest leaf for the remaining varying bits.

## Three leaves

Every tile holds at most **64 KiB of records** and at most **65,535 records** (so every slot index fits in a `u16`). Depending on length and remaining key variation, the tile finishes with one of three leaves:

| Leaf | Used when | How it works |
|---|---|---|
| <span id="insertion">**Insertion sort**</span> | Tile fits the short-leaf cutoff below | Stable insertion over encoded keys; zero radix setup. |
| <span id="tile-lsd">**Tile LSD**</span> | All varying bits fit in 1–2 flat passes | Branch-free counting passes from lowest digit to highest, alternating between tile and scratch. |
| <span id="tile-msd">**Tile MSD**</span> | More than 2 passes needed, or SIMD MSD override applies | One top-digit counting pass, then either a whole-tile finisher or nested tile leaves per bucket. |

## Digit planning

Instead of hardcoding 8-bit bytes, Tessera chooses digit widths dynamically from the tile's actual varying bits and cursor-table budget (one `u16` per bucket):

- **One pass:** if all remaining varying bits fit inside one digit, one pass finishes the tile.
- **Two balanced passes:** if they fit in two passes, Tessera chooses the split with the **fewest total cursor-table slots**. For 20 adjacent varying bits with a 14-bit table cap, it picks **10 + 10 bits** (2,048 table slots) rather than 14 + 6 (16,448 slots) — same two record moves, 8× smaller histogram! Non-adjacent varying bits need not be contiguous: two scattered varying bits take two 1-bit passes.
- **SIMD MSD override:** when direct-key SIMD finishing is available and one top digit already gives at least one bucket per record using fewer table slots than two-pass LSD, Tessera overrides two-pass LSD with that single MSD pass.
- **Otherwise, partition and inspect:** run one top-digit MSD pass and let each child bucket re-plan from the bits that actually vary inside it. Small buckets drop straight to insertion without scanning; constant buckets finish immediately. (This favors skewed/concentrated tiles and can occasionally lose to a longer LSD plan; [engine evidence](../design/evidence.md#landed-with-open-losses) tracks those cases.)

Click any preset or toggle individual bits below to compare Tessera's digit plan against naive fixed-width windows:

<DigitPlanner />

## Inside a pass

Tile passes use the classic [counting pass](radix-sort.md#the-counting-pass) with three practical refinements:

- **Direct-key counted fill:** for a single-digit `sort` plan, equal keys are bit-identical — count occurrences and write each run directly from counts and shared constant bits, moving nothing.
- **One key evaluation per pass:** for `sort_by_key`, the histogram stores each record's `u16` bucket ID in scratch so scatter never calls the user's key function a second time (which also makes inconsistent key functions memory-safe; see [6. Ownership and panics](safety.md)).
- **Zero-cost tracking of live buffer:** a guard remembers whether the live records sit in the tile or in scratch, copying them back if a plan ends in scratch or unwinds on panic.

After an MSD pass, boundaries are read directly from the finished cursor table (recomputing them measured 48% slower at ~1 bucket per record). When the buckets are tiny, Tessera finishes the whole tile in one sweep instead of recursing bucket by bucket:

- **Every bucket ≤ 1 record:** restoring from scratch already finishes the tile.
- **Tiny buckets on direct SIMD keys:** a branchless odd-even vector sorting network repairs the restored tile in place.
- **All buckets insertion-sized:** one contiguous insertion sweep across the tile finishes it without walking bucket boundaries.

## Parallel child allowance

A complete child of a parallel root may use a larger tile — up to **1 MiB of records** (still capped at 65,535 records) — when the entire child fits. Larger children keep the ordinary 64 KiB tile budget for their descendants. This is an empirical threshold, not a cache-size claim.

## Exact rules & thresholds

| Rule | Value / formula |
|---|---|
| Tile capacity | `≤ 64 KiB` of records and `≤ 65,535` records (`u16` positions). |
| Short-leaf insertion cutoff | For inline record width $w$ bytes: $\max(16, \lfloor 48 \times 16 / (16 + w) \rfloor)$ records (models comparison + loop overhead as 16 equivalent bytes per inversion; keeps ≥ 16 records to amortize radix setup; see [short insertion leaves](../design/evidence.md#short-insertion-leaves)). |
| Max digit width (cursor table) | Clamped to `6–14` bits, bounded by cursor table bytes ≤ tile bytes (14 bits = 32 KiB) and ≤ 4 buckets per record. |
| Direct-key SIMD MSD repair | Requires a real SIMD backend with ≥ 8 vector lanes, tile length `≥ max(64, 2 * lanes)`, and max bucket size `≤ lanes + 1` (one vector of overlap). Tiles with `≥ 8 * lanes` records run four independent repair streams, then stitch seams and tail; shorter tiles run one overlapping sweep. Counting pass restores records before finishing so ownership/panic guards stay shared. |
| Varying-bit progress | Child scans mask against parent's remaining candidate bits, so a partitioned digit can never reappear even under inconsistent key closures. |

Next: [6. Ownership and panics](safety.md) explains how raw bitwise record movement stays safe for arbitrary Rust types even if a key closure panics.
