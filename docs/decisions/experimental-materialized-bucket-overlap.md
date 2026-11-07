# 0017: Overlap retired bucket batches with root compaction

Date: 2026-11-07. Status: experimental; controlled acceptance pending.

## Context

Finishing all root compaction before starting children leaves a serial phase on
the critical path. Compaction already knows when a bucket will no longer be
read or written. Adjacent buckets can be batched for scheduling, but separate
exclusive borrows must not be widened merely because their addresses adjoin.

## Decision

Queue retired bucket batches on a Rayon scope as they reach 256 KiB. Preserve
each bucket's own pointer and length and sort it through the existing child
kernels. Reuse workspace objects through a pool whose mutex never covers user
code. Preserve the sequential API, adaptive fallback, and std/no_std defaults.
This supersedes only the all-materialized scheduling and pool-size assumptions
in [decision 0016](0016-optional-bounded-rayon.md); it does not adopt the archived native-root research patch.

## Consequences

Root compaction and children may overlap; root scatter and compaction remain
serial. The exact pointer/length pairs allow targeted Miri coverage without
running Rayon itself. Native scheduling and panic tests remain necessary.

For W leased/retained workspaces, heap scratch is O(√n + √(Wn) + W + R), with R
root buckets. Ordinary extractors do not lease more than one workspace per
worker, but reentrant Rayon calls from user keys can suspend several jobs per
worker. The dynamic pool does not enforce a strict worker-count workspace cap;
W is conservatively at most the number of root jobs. Report actual heap use and
this distinction rather than claiming an unconditional one-per-worker bound.

The inherited hardware measurements describe their original source and machine.
They are not substituted for same-platform comparisons of the corrected public
implementation, strong serial entry point and frozen research candidates.
