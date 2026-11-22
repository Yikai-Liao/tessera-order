# Comparison model proposal

Status: identities, routes and their stability, per-operation publication and a section per tracked operation have landed; the parts below remain proposed. Current vocabulary and module boundaries belong to [benchmark tooling](benchmark-tooling.md), identities, plans and the rules for choosing references and cases to the [benchmark protocol](../development/benchmarking.md#choosing-references-and-cases), and published data formats and section rules to [continuous benchmarks](../development/continuous-benchmarks.md#data-and-site-publication). This page owns the plan for the remaining parts until they land; move each landed fact to those owners and delete it here.

## Problem

The [implementation comparison](../benchmarks/implementations.md) has a section per tracked operation, each against its own reference with rows named by route, but it still answers less for other operations than for sorting:

- **One parameter value per section.** A selection section shows the rank with the most measured cases; how speed changes with rank position is not shown.
- **Memory covers only sorting.** `growth.json` sweeps full sorts; other sections plot the peaks their tracking runs recorded, when a type has several lengths.
- **No composition baseline.** Sections do not show Tessera's full sort beside its dedicated route, so they do not show the work a route saves.

## Plan

- **Parameter axes.** One figure per parameter axis, such as rank position for selection, from the existing row template with the operation's own reference. A new template needs a new question.
- **Growth.** `growth.json` gains a sweep per operation, because argsort's retained index output and in-place selection answer different memory questions.
- **Composition baselines.** Tracking measures the baselines below, and each section marks them as compositions rather than competitors.
- **Parallel sections.** Section titles for parallel operations derive from the operation's entry labels and always name the worker count ("par_argsort · 8 threads").
- **Summary.** A summary of Tessera's speed per operation stays deferred: each operation has its own reference, and the figure would have to prevent reading ratios across references as comparable. The home page keeps the sort ranking.

The catalog names each operation's reference. The composition baselines answer whether a dedicated route beats composing the result from a sort:

| Operation | Reference | Composition baseline |
|---|---|---|
| `sort` | `slice::sort` | — |
| `argsort` | Index vector sorted with `sort_by_key` | Tessera sort of (key, index) rows |
| `select_nth` | `select_nth_unstable` | Tessera full sort (`tessera-full`) |
| `sort`, `argsort` at `tN` | Rayon's stable sort of the same records or indices | Serial Tessera |

The strongest public implementations for selection and argsort are still to be surveyed; name them in this table only after their adapters verify. The work follows the [CI scaling proposal](ci-scaling-proposal.md#keep-cross-implementation-comparisons-out-of-pr-timing) and changes no measured algorithm, input or sampling.
