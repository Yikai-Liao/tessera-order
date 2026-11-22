# Comparison model proposal

Status: identities, routes and their stability, per-operation publication, a section per tracked operation, parameter figures, parallel section titles, a memory sweep per operation and composition baselines have landed; the parts below remain proposed. Current vocabulary and module boundaries belong to [benchmark tooling](benchmark-tooling.md), identities, plans and the rules for choosing references and cases to the [benchmark protocol](../development/benchmarking.md#choosing-references-and-cases), and published data formats and section rules to [continuous benchmarks](../development/continuous-benchmarks.md#data-and-site-publication). This page owns the plan for the remaining parts until they land; move each landed fact to those owners and delete it here.

## Problem

The [implementation comparison](../benchmarks/implementations.md) has a section per tracked operation, each against its own reference with rows named by route, parameter figures, a memory sweep and a composition baseline. Three gaps remain: parallel argsort has no section before Tessera has a parallel entry point, no figure summarizes Tessera across operations, and selection and argsort have no dedicated public opponents yet.

## Plan

- **Parallel argsort baseline.** Once Tessera has a parallel argsort entry point, its section shows Tessera's serial argsort as the composition baseline, as the parallel sort figure shows serial Tessera.
- **Summary.** A summary of Tessera's speed per operation stays deferred: each operation has its own reference, and the figure would have to prevent reading ratios across references as comparable. The home page keeps the sort ranking.

The strongest public implementations for selection and argsort are still to be surveyed. Name them only after their adapters verify. The work follows the [CI scaling proposal](ci-scaling-proposal.md#keep-cross-implementation-comparisons-out-of-pr-timing) and changes no measured algorithm, input or sampling.
