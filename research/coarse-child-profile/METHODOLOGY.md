<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Methodology and interpretation

## Fixed workload

The diagnostic used the previously frozen, ordinary yielding-consumer implementation. It did not incorporate the rejected draining-consumer scheduler or newer public-source changes. The workload was fixed at eight workers, 10 million records, SplitMix64 seed 1, and root radix 16. Record types were direct u64 and a 16-byte key/original-index record. The allocation policy was warmed allocation with repeated fixed input; each sort received a fresh clone prepared outside its timer.

Logical children 14 and 15 were selected before collecting new data because they covered the last-finishing children in the earlier measured traces. Publication, start, and end observations still covered all children. If another child finished last, that coverage miss was reported rather than replacing the focus or repeating the run.

## Phase accounting

A child block partition calls tile callbacks during materialization, so timing the whole partition does not isolate movement. The diagnostic separately recorded:

1. Whole-child service and initial key scan.
2. Inclusive block partition, setup, scatter, and inclusive materialization.
3. Each focused callback's key scan and complete tile sort.

For each child in each run, exclusive partition cost was its inclusive cost minus its own callback intervals. Exclusive materialization used the same per-run subtraction. Independent medians were never subtracted, and overlapping inclusive phases were never added.

The selected children each had one block partition, 256 tile callbacks and tile-MSD calls, zero LSD passes, and complete matching record counts. Unexpected routes, missing scan boundaries, event overflow, incorrect geometry, or reference failures rejected the run.

## Controls and perturbation

Each process used fifteen labels: native and public parallel A/A and placement controls; native and public serial A/A and placement controls; and the same diagnostic binary in disabled, boundary-only, and focused-leaf modes. True controls retained their original frozen libraries. Control samples accumulated at least 50 ms of constituent sort timers; diagnostic modes used one verified call per block.

Six timing processes gave three blocks per type. Mode order was balanced before execution: each diagnostic mode occupied each relative position once per type. All label orders and randomization seeds were fixed before compilation. There were no replacement runs, discarded outliers, or extra cases.

Diagnostic modes used bounded worker-local event buffers, not global per-leaf mutex logging or per-record timers. Across eight workers, their requested heap/TLS payload was 656,128 bytes, plus 64 bytes of static configuration. This excludes runtime/allocator overhead and is not a measured memory peak or a proof of an optimized algorithm's space bound.

The focused mode added three CPU/wall timestamp pairs per callback, or 1,536 pairs per call. Clock samples are sequential. Signed wall-minus-CPU residuals were retained. Aggregate callback discrepancies ranged from approximately −36.7 to +4.4 microseconds. Timer/check overhead was not subtracted through a calibration model.

## Environment checks

Every process and all eight workers reported an allowed CPU mask of 0–7. The binary's Fearless SIMD probe reported both detected and dispatch-selected Avx512. This was a same-configuration dispatch probe, not an inference from processor flags or proof of every sort-site invocation.

The compiler was rustc 1.99.1, commit 58cf8f9c3, dated 2026-10-13. The diagnostic library enabled default, std, rayon, and portable-simd features. Build settings were edition 2024, optimization level 3, 16 codegen units, disabled debug assertions and overflow checks, unwind panics, the default x86_64 target, and no incremental compilation. The only sorting-specific environment override was the root-width setting of 4 bits.

## Limits on conclusions

The two large mode-comparison anomalies exceed contemporaneous controls and remain unexplained. One long interval was localized to an unfocused child and another included a prolonged publication window. Localization is not a causal explanation. There were no hardware-counter or scheduling measurements to distinguish cache, traffic, CPU, or scheduler mechanisms.

Observed worker child-service availability does not establish OS idleness or the feasibility of sharing a tile's scratch state. A different scheduler would change readiness, contention, and possibly the critical child. The percentages in the summary describe only instrumented selected children; they do not represent the entire sort or an uninstrumented speedup opportunity.
