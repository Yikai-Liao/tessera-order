# 0008: Give insertion finishing a unit-returning entry point

Date: 2026-11-07. Status: accepted.

## Context

The engine uses insertion both to estimate work within a swap budget and to
finish a partitioned tile. A probe needs its swap count and may stop early.
A finishing pass needs every record sorted; its count is unused.

Returning both results through the same runtime-dispatched entry point can
hide the finishing pass's constant budget and unused result from the
compiler. Its target-feature function then retains swap accounting that the
caller discards. Splitting the call also changes scalar inlining: a default
scan that becomes an out-of-line helper adds a call per inversion search.

## Decision

Keep one insertion loop and expose two extractor operations:

- `insertion_sort` accepts a budget and returns `Option<usize>` for probes.
- `finish_insertion` returns `()` and supplies `usize::MAX` to that same loop.

The finishing wrapper is inlined inside the selected target-feature context,
so its unused swap count can disappear before that function returns. Both
operations have the same meaning in the default and portable SIMD backends.
Backend selection controls the scan implementation and dispatch timing.

Keep the direct scan chain inlined, as the SIMD scan already is. This exposes
the caller's slice bounds and prevents a separate call for every scan. User
key functions retain their existing scan and inlining policy.

A portable insertion pass shorter than the existing `SIMD_MIN` stays scalar
without runtime dispatch. Every suffix it subsequently scans is shorter too,
so it cannot reach a vector kernel. This reuses the scan's existing gate;
there is no new tuning parameter. Routing, digit planning and tile limits
stay unchanged.

## Consequences

The ordering, stability, panic and workspace contracts stay unchanged. No
record cache or scratch allocation is added. The compiler may instantiate
separate probe and finishing functions and duplicate inlined scan code, so
both feature configurations need performance checks, including long inputs.

Acceptance follows the [benchmark protocol](../development/benchmarking.md):
nine shuffled blocks, duplicate baseline timings, independently named source
copies, verified outputs, and separate API and prepared-workspace runs.
Raw data, executable snapshots and the diagnostic accounting ablation remain
outside the repository.

## Alternatives considered

A feature-dependent accounting policy couples a caller's need for a result
to the available instruction set. Separate sorting loops duplicate the
ordering and recovery logic. A shared loop with operation-specific return
values preserves one algorithm while exposing the information the compiler
needs at each call boundary.
