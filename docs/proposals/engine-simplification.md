# Proposal: engine simplification candidates

Status: candidate evaluation in progress. Date: 2026-11-11.

This lists the remaining opportunities to replace a specialized engine mechanism with a more general one. It is a search result, not a plan of record: each candidate is adopted only if it meets the [acceptance rule](#acceptance-rule), and the [engine reference](../design/engine.md) continues to describe what the code does today. Delete an entry when it lands or is rejected; record accepted debt in the [roadmap](../../ROADMAP.md#measured-engine-refactors).

The goal is fewer mechanisms in the core algorithm, not fewer lines: tests, comments and safety arguments are not reduction targets. Non-test engine source went from 3,591 to 3,358 lines over the five slices below, measured as the lines before each file's test module.

## Landed in this series

The slices below landed. Their performance acceptance, the follow-up candidate checkpoints and their controls are recorded in the [port record](../design/port.md). The root scan has one candidate-masked implementation and tile planning attempts at most two flat passes, otherwise partitions actual children; the port record owns their measurements.

| Slice | Mechanisms retired | Where the benefit remains |
|---|---|---|
| Covering digits | `NodePlan::Window`, the ends-free block entry, the visit-only parallel entry, optional record-end computation | A digit covering all variation is an ordinary digit route whose children return at once; direct keys still count instead of moving records |
| One half-key extractor | Cloneable `Narrow`, the shared-implementation macro and its duplicate tests | Entries take the extractor by value, so one owned `KeyHalf` is cloneable whenever its extractor is |
| One scan hook | Four forwarding methods on the sealed key trait, their macro, `Direct`'s scan override | Each key type supplies only its adjacent-pair scan; passes are written once. `Reverse<K>` now follows the primitive length rule |
| One tile pass loop | Duplicated scratch, guard and buffer-swap setup in the MSD and LSD leaves | An MSD partition is the one-digit case of the same loop |
| Uniform scratch segments | Rayon-only scratch field, second block lookup, three callers choosing between them | A serial pass is the one-segment case; the lookup is a shift because a segment holds `2R` blocks |

## Acceptance rule

A candidate lands when all of these hold:

1. One mechanism replaces at least two, and a representative change (sampling, tile capacity, partition selection, block layout or task size) touches fewer places afterwards.
2. The retired fast path's saving is kept by the general mechanism, or each measured loss is listed and accepted individually.
3. Structural and routing changes are separate commits, measured under the [benchmark protocol](../development/benchmarking.md#run-an-experiment).

For this series, the maintainer accepts individually reported local losses of at most 10% when the reduction in mechanisms warrants them. This is a ceiling, not a performance budget: preserve savings through the general mechanism before accepting a loss. Use three-block, narrowly selected screens rather than nine-block matrices; expand input coverage only to answer a concrete remaining question. Do not claim source causality when placement or A/A controls do not support it. Rejected candidates retain evidence in the [port record](../design/port.md#simplification-series-candidate-screens). Each PR needs independent review and correctness checks before it merges.

## Candidates

Ordered by expected value for the risk. "Cases" names the continuous shapes most likely to move.

### Root preparation

| ID | Retire | Replace with | Risk and cases |
|---|---|---|---|
| R2 | `Order::Flat` and the second confirmation after the variation scan | Confirm all sampled order candidates with the shared scan | [PR #105](https://github.com/Yikai-Liao/tessera-sort/pull/105) is open, not draft, and explicitly **do not merge**; constant-input losses exceed the ordinary limit, and a possible exception is not accepted |
| R3 | Three copies of "ascending returns, descending reverses" (probe, flat, child) | Implemented by one `finish_ordered` operation, retaining the flat-sample route while R2 is on hold | [Targeted evidence](../design/port.md#r3-shared-ordered-finishing); final performance acceptance pending |
| R4 | The `INSERTION_MAX` root shortcut beside the budgeted probe | One insertion attempt that owns its budget; a leaf-sized input is the unlimited-budget case | Short inputs carry the known `random/1000` losses; measure them first |
| R5 | `Descending { strict }` | Always reverse stably | One extra key scan on `reverse`; likely a rejected candidate, listed so the cost is measured once |

### Node planning

| ID | Retire | Replace with | Risk and cases |
|---|---|---|---|
| N1 | `plan_progress` and the equality fallback in the executor | Let the plan carry its progress guarantee: an equality child is planned with equality disabled, so the executor never re-plans | Preserves the inconsistent-key termination test; no routing change for consistent keys |
| N2 | `two_runs`, `NodePlan::Runs`, `split_ordered` | A block partition that detects non-decreasing bucket order while classifying and returns without moving records. This covers any number of already separated pieces | Loses the rotated `1*0*` form and adds a fallback rescan: `sorted_tail`, `bimodal`, `far_swaps`. Speculative |
| N4 | `partition_by` beside `partition_digit` | One block entry taking a classifier and bucket count; the engine builds the digit classifier | The scatter closure must stay owned by the loop; a `&mut` classifier measured 40% slower |

### Extraction

| ID | Retire | Replace with | Risk and cases |
|---|---|---|---|
| X1 | `try_count_window_parallel` beside `try_count_window`, and both forwards in `KeyHalf` | One counting operation that receives the shard count. Sealed keys are all `Send`, so the bound can move to the key trait | A flag-shaped parameter; acceptable only if the serial build keeps no Rayon call |
| X2 | Two expressions of "the record is its own key" (`CACHE_BUCKETS = false` and the counting override) | One extractor fact from which both the scatter re-read and count-and-fill follow | `KeyHalf` over `Direct` must keep both |
| X3 | The duplicated half-key dispatch in the serial and parallel entries | One root helper generic over the root strategy; or generalize `KeyHalf` to the narrowest unsigned type covering the varying span | The generalization is a routing change for 64-bit keys with narrow variation and needs its own measurements |
| X4 | `finish_insertion` beside `insertion_sort(usize::MAX)` | One entry, if discarding the swap count across the target-feature boundary no longer matters | Measured micro-choice; short tiles |

### Tiles

| ID | Retire | Replace with | Risk and cases |
|---|---|---|---|
| T1 | Two homes for cached bucket IDs (cursor-table suffix or scratch tail) | Always the scratch tail | One more cache line set per by-key pass on small tiles: `row_u64` at 1,000 and 65,536 |

### Parallel completion

| ID | Retire | Replace with | Risk and cases |
|---|---|---|---|
| P1 | One of the two root completions: bounded staging (`finish.rs`) or parallel permutation with materialized jobs (`compact.rs`) | Whichever serves every layout within the controls. Bounded staging could split an oversized bucket instead of declining | The largest mass, about 600 non-test lines, and the largest risk. Needs the eight-core whole-call evidence rerun and the memory bound rechecked |
| P2 | `Config`, `BucketFinish` and `BoundedFinish` describing one root partition | One description consumed by the block layer | Structural; follows P1 |
| P3 | `tile::Scratch`, `block::Buffers` and the job `Pool` as three reuse protocols | One scratch owner per worker | Structural; check allocation counts in the memory tests |

## PR sequence

Evaluate one candidate per PR, against the preceding merged head. Keep a structural change separate from any later routing experiment. Rejection is a completed evaluation, not permission to remove the saving another way without evidence.

1. Root preparation: R2 is published in [PR #105](https://github.com/Yikai-Liao/tessera-sort/pull/105) for discussion with an explicit merge hold after its constant-input screen exceeded the ordinary limit. Do not merge R2 without lifting that hold; whether losses are confined to all-equal inputs is unproven. Proceed independently with R3 while retaining `Flat`. Evaluate R4 separately.
2. Independent small boundaries: N1, X1, X2 and N4, each with its affected invariants and targeted inputs.
3. Tile and entry cleanup: X4, T1 and X3. Try X3's structural helper before considering narrower-key routing.
4. Speculative routing: R5 and N2; stop candidates whose known savings cannot be preserved within the acceptance limit.
5. Parallel completion: start P1 with the existing completion evidence and a bounded feasibility screen. P2 follows the chosen completion representation; P3 must preserve allocation reuse and the documented memory bound.

The [port record](../design/port.md#simplification-series-candidate-screens) owns evaluated candidates and their measurements. Delete candidates above only when accepted or rejected; keep remaining work visible here.
