# Parallel Direct count-window research archive

This archive pairs the [source lineage and replay instructions](source/README.md) with the [initial end-to-end screen](results/REPORT.md). Source and results have separate SHA256SUMS manifests. No patch is applied to the repository runtime.

The experiment is frozen on upstream e8e590616f0d3cfb5e8a30b84e4b3e43042036a4 plus the historical combined research control, not the newer main integrated into this branch. The replay normalizes only the private Cargo package name; Rust bytes match the measured candidate.

All 360 samples and 1,681 timed sorts passed full-reference checks. Three of four narrow Direct unsigned cases clear this screen's A/A and placement controls; u32/d256 is inconclusive because the retained tail widens its controls. This is initial feature-enabled research evidence, not production-feature-off, scaling, broader distribution or full-matrix acceptance. There is no default adoption.

Observed full-sort peak requested heap is 65,536 bytes for all labels, with zero retention; the candidate requests 18,440 additional bytes in total. Equal peaks do not establish allocation lifecycle causation. See the report for the full control ranges, retained tail, absolute timings, route witnesses and validation limits.

Publication independently checked both packet manifests, the source archive members, all three patches forward and backward, complete base-tree restoration, a second replay, normalized file hashes and exact frozen candidate identity. No build or benchmark was rerun. Raw samples, harnesses, source-copy trees and binaries remain outside Git.
