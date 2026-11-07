<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Independent historical causal controls

These three checkpoints are **not adopted** and must not be stacked:

- [Fixed root P4](fixed-root-p4.README.md)
- [Fixed 50µs wait](sleep50-wait.README.md)
- [Deferred parallel consumers](deferred-overlap.README.md)

Each independently starts from historical b1d36ce plus combined-main-5b13.patch. None is a drop-in for public 0e50926. Read the aggregate findings, exact validation scope, space/progress notes and per-control lineage before use. Only source/test diffs, concise documents and evidence hashes are included. Raw timing data and tooling remain external. No new run or implementation is requested by this archive.
