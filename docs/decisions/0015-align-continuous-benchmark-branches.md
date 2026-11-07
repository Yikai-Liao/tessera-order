# 0015: Align branches in continuous benchmark builds

Date: 2026-11-07. Status: accepted. Amends the shared build flags and history
transition of [0011](0011-benchmark-feature-configurations.md).

## Context

Adding a benchmark changes the executable's layout, even when the existing
sorting instructions remain the same. On Intel CPUs affected by the JCC
erratum, branches crossing or ending on a 32-byte boundary can lose access
to the decoded instruction cache after the microcode mitigation. The
[port experiments](../design/port.md#rebuild-and-branch-alignment-check)
identified large placement effects that branch alignment removed on the
tested ordered inputs, while other placement effects remained.

## Decision

Add `-C llvm-args=-x86-branches-within-32B-boundaries` to both continuous
benchmark builds, Default and Portable SIMD. Keep static glibc linking and
runtime SIMD dispatch. Apply the flag in the benchmark workflow, without
changing the crate's build configuration or the release report's default
profile.

Keep Testbeds `intel-v1` and `intel-v1-portable-simd`, their hardware spec,
benchmark IDs and existing histories. Do not add a configuration or reset
history for this flag change.

## Consequences

The first aligned measurements compare against unaligned baselines, so
alerts at the transition may reflect the build flag alone. The previous
sample rule from [0013](0013-compare-with-the-previous-main-sample.md) uses
an aligned baseline once an aligned `main` result exists at a PR's base
commit. Earlier-base PRs still inherit their earlier baseline.

Branch alignment changes code size and absolute timing. It targets a known
Intel boundary hazard, not every source of layout sensitivity, and does not
prove the cause of any particular benchmark alert. Performance claims still
require the benchmark protocol's A/A and placement controls. The LLVM
option must remain supported by the compiler used to build the benchmarks.
