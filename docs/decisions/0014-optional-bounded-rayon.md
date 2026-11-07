# 0014: Optional parallel completion of materialized buckets

Date: 2026-11-07. Status: experimental; scheduling and space accounting superseded
by [0015](0015-materialized-bucket-overlap.md).

## Context

Parallel sorting must not silently replace Tessera's sublinear block scratch
with an O(n) output buffer or weaken stable ordering and panic recovery.
The sequential API supports `FnMut`, non-`Send` records and `no_std`; these
contracts cannot require a parallel runtime by default.

## Decision

Add an optional `rayon` feature that implies `std` and separate parallel free
functions. Keep the existing sequential API and default features unchanged.
Require `T: Send` and `F: Fn(&T) -> K + Sync`, without imposing `T: Sync`,
`Clone`, or `Copy`.

Start tasks only after the root stable block partition has fully materialized
all records. Split mutable slices at bucket boundaries and allocate a bounded
number of independent workspaces. Reuse the sequential child kernels and their
panic guards. Retain sequential adaptive fast paths and test worker panic
recovery explicitly.

## Consequences

The implementation adds no unsafe record-moving code. Stable buckets isolate
equal-key runs, and Rayon joins restore a complete live input before a panic
escapes. Default and `no_std` builds retain no runtime dependencies.

The scratch bound becomes O(√n + √(Pn) + P) for P selected workspaces and fixed
record size/key width. It remains O(√n) only for fixed P. Rayon thread stacks
and runtime allocation are separate. A full-buffer parallel scatter is not
part of this design.

The serial root partition limits scaling, and moving tile completion out of
cache-hot compaction callbacks may cost time. Performance acceptance therefore
includes the original serial version, an identical-source placement control,
a split sequential control, explicit thread counts, and allocation measurements.
Raw experiment output remains outside the repository.
