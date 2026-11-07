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

Publish a separate GitHub Check for each configuration, named
`Benchmarks (rust, Default)` and `Benchmarks (rust, Portable SIMD)` after
the configuration names in the comment. After measuring both, publish one
job summary and one PR comment. In that comment, configurations are
columns and cases are rows: one overview table, one comparison chart and
one PR history chart, and one table of changed cases, each with a column
per configuration. A case is named once, and each alert states whether it
occurs in every configuration or only in some. Compute changes and
geometric means within each configuration; there is no mean across them.
Preserve the existing same-head comment update and superseded-head rules.

API variants of one executable, such as the stable sort with or without a
prepared workspace, and later an unstable or parallel sort, are not
configurations. They are the mode segment of the case ID, measured in the
same remote run, and the report groups them as rows. Only a change to how
the executable is built adds a configuration, and with it a column, a
Testbed, a remote job and a check. Separate sections or chart pairs per
configuration were rejected: they repeat every case name per build, push
the second build's charts below the first build's tables, and multiply
with every further build or API.

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
