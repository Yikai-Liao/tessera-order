# Comparison model proposal

Status: identities, routes and per-operation publication have landed; populating comparisons beyond full sorting is proposed. Current vocabulary and module boundaries belong to [benchmark tooling](benchmark-tooling.md), identities, plans and the rules for choosing references and cases to the [benchmark protocol](../development/benchmarking.md#choosing-references-and-cases), and published data formats to [continuous benchmarks](../development/continuous-benchmarks.md#main-long-term-tracking). This page owns the plan for the remaining operations until it lands; move each landed fact to those owners and delete it here.

## Problem

The [implementation comparison](../benchmarks/implementations.md) renders one section per operation of `comparison.json`, but only full sorting has a section:

- **Only sorting has a cohort and figures.** Tracking runs hold every operation's cases, but `comparison.ts` chooses only a sort cohort, and the figure templates assume the sort reference.
- **Implementation entries carry sort semantics.** `std-stable` is labelled `slice::sort`, although its argsort route sorts an index vector by key and its selection route sorts the whole input. A selection section would show `std-unstable` as "slice::sort_unstable" while it calls `select_nth_unstable`.

## Plan

- **Labels.** A displayed row name combines the implementation label and the route detail when they differ, such as "slice::sort_unstable · select_nth_unstable". Section titles derive from the operation's entry labels and execution ("argsort", "par_argsort · 8 threads"), always naming the worker count. Stability is a property of the route; unstable record results never enter a stable ranking.
- **Tracking.** Each section draws its cases from the latest run per column, so an aggregate never combines cases from different runs. `growth.json` gains a section per operation, because argsort's retained index output and in-place selection answer different memory questions.
- **Figures.** Each published operation has a ranking with memory, a pattern figure and a length figure, plus one figure per parameter axis (such as rank position for selection), from the existing row and curve templates with the operation's own reference. A new template needs a new question. The home page keeps the sort ranking. A summary of Tessera's speed per operation is deferred: each operation has its own reference, and the figure would have to prevent reading ratios across references as comparable.

The catalog names each operation's reference. The composition baselines answer whether a dedicated route beats composing the result from a sort:

| Operation | Reference | Composition baseline |
|---|---|---|
| `sort` | `slice::sort` | — |
| `argsort` | Index vector sorted with `sort_by_key` | Tessera sort of (key, index) rows |
| `select_nth` | `select_nth_unstable` | Tessera full sort (`tessera-full`) |
| `sort`, `argsort` at `tN` | Rayon's stable sort of the same records or indices | Serial Tessera |

The strongest public implementations for selection and argsort are still to be surveyed; name them in this table only after their adapters verify. The work follows the [CI scaling proposal](ci-scaling-proposal.md#keep-cross-implementation-comparisons-out-of-pr-timing) and changes no measured algorithm, input or sampling.
