# 0010: Track default and Portable SIMD benchmarks independently

Date: 2026-11-07. Status: accepted.

## Context

Portable SIMD is optional and off by default. Measuring only that build
leaves the default build without regression coverage. The two builds have
different performance characteristics, so sharing a baseline or combining
their geometric means can hide a regression.

The report publisher identifies PR comments by language and commit. Two
independent publishers for the same Rust commit would overwrite each
other's report.

## Decision

Build the same Criterion suite with default features and with
`tessera-sort/portable-simd` added. Keep the compiler, flags, cases and
hardware specification the same. Preserve runtime SIMD dispatch without
forcing or disabling any instruction set. Default builds may still use
compiler auto-vectorization.

Retain Testbed `intel-v1` for Default and `intel-v1-portable-simd` for
Portable SIMD, both using hardware spec `intel-v1`. Each configuration has
its own baseline, thresholds, alerts and PR history. Run the executables
as two sequential remote jobs with independent timeouts under the existing
concurrency group. Continue to the second measurement if the first fails.

Publish a separate GitHub Check for each configuration. After measuring
both, publish one job summary and one PR comment with an overview and two
report sections. Compute changes and geometric means within each
configuration. Keep image paths separate by configuration and preserve
the existing same-head comment update and superseded-head rules.

## Consequences

Both supported configurations receive regression coverage, at the cost of
two remote runs per measured commit. Existing histories remain usable
because neither build's compiler flags nor case IDs change. Main needs
enough samples in both Testbeds before both comparisons can alert.

A failed measurement remains visible without hiding the other result.
Repository rules requiring the old `Benchmarks (rust)` check must switch
to the two configuration-specific checks. Continuous results remain
regression signals; performance claims still need the full A/A and
placement controls of the benchmark protocol.
