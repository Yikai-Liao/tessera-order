<!-- SPDX-License-Identifier: BSD-2-Clause -->

# 0040: Partition low-fanout tiles with MSD

Date: 2026-11-08. Status: accepted.

## Context

The tile cost model estimates MSD depth by dividing the tile length by the sample's distinct-bucket count until the average child fits insertion. That estimate misses a heavily concentrated child that becomes constant after one split. On 1,000-record `skew99` inputs, it can tie a six-pass LSD plan and leave the tile on LSD.

## Decision

For plans longer than two LSD passes, select the existing stable MSD partition when the 128-key sample finds at most two distinct top-digit buckets. Keep the depth estimate for other samples and skip sampling for one- and two-pass plans. Do not change digit widths, thresholds, caching or the public sorting contract.

Intersect child variation scans with the remaining candidate mask. An inconsistent key function may report variation in an already partitioned digit; allowing that digit back into the plan can otherwise recurse indefinitely when every histogram puts the whole tile in one bucket. Masking the scan makes progress depend on the remaining key bits, not on consistent callbacks or shrinking buckets.

Keep the Rust change local to the existing leaf condition. The TypeScript model returns MSD before calculating an unused depth estimate for low fan-out. The interactive planner labels its table width as a hypothetical limit: the actual engine uses 14 bits for 8,192 eight-byte records, even when the figure's slider selects a narrower table.

## Validation

The initial default-release measurements used the comparison runner pinned to CPU 2, with A/A and separate code-placement controls. The two baseline placements measured 20.308 and 20.157 ns/record on `u64/skew99/1000`, against 6.243 and 6.199 for the candidate; a second candidate placement measured 6.207. For `row_u64/skew99/1000`, the corresponding baseline values were 22.442 and 21.077, against 6.518 and 6.458; the placement control measured 6.468. Random controls showed no comparable shift.

After adding the candidate-mask fix, a separate paired harness checked 17 scalar and keyed-row cases in Default and Portable SIMD builds on a Ryzen 7 8845H pinned to CPU 2. Each build ran two processes of nine shuffled blocks, with two baseline placements, two original-PR placements, two revised placements and a repeated original-PR A/A call. Inputs were restored outside the clock and every result was checked against a stable reference. Both revised placements retained the short-skew improvement: baseline/original-PR ratios were 3.23–3.37 in Default and 3.09–3.23 in Portable SIMD, and revised/original-PR ratios were 0.94–1.03. The per-case A/A bounds on those skew cases ranged from 0.969 to 1.031. These figures describe the tested inputs and placements, not an application-wide speedup.

The continuous suite alerted on Default `u64/random/48` (+9.4%) and `u64/equal/65536` (+5.0%). Neither reaches the changed leaf decision. Paired measurements nevertheless exposed a real placement effect: identical original-PR copies differed by 21–22% on `random/48`, outside A/A bounds of 0.962–1.038 and 0.986–1.014. This is not discarded as timing noise.

The actual Default CI executables for baseline `e0ecd94` and candidate `54926ca` reproduced the short-input gap locally. Their order probe, root routing and budgeted insertion have identical normalized instructions; the tile's insertion path is also unchanged. A local binary intervention moved the candidate's `sort_tile` body 160 bytes to the baseline address, preserving instructions and relocating references, without changing the source or the input. Across three processes, baseline latency was 237–243 ns/sort, candidate latency 282–287, and the relocated candidate 243–247. Relocating only the budgeted insertion function did not remove the gap. This isolates tile code placement on the local host; it does not establish the Intel runner's microarchitectural mechanism.

The equal-key case did not show a consistent revised-candidate loss across the paired processes. An attempted early insertion finish for tiny inputs was rejected after the expanded matrix showed regressions on other small inputs. No source-level layout workaround or routing change is retained to tune one linked executable. Raw samples, executables and binary-intervention scripts remain outside the repository.

Two Rust tests check that scanned variation stays within the candidate and that an adversarial changing-key tile terminates while preserving records. Removing the mask makes the latter fail at a finite callback limit rather than overflowing the test process's stack. The model tests cover low fan-out, the strict cost comparison, saturated sampling, short plans and terminal leaves.

## Consequences

Short, heavily skewed tiles can stop after a stable split rather than taking every LSD pass. Consistent keys keep the same ordering and stability guarantees. Inconsistent keys cannot restore processed digits, preserving the existing recursion bound. Small-input timing remains placement-sensitive; continuous alerts still require the paired and placement controls in the [benchmark protocol](../development/benchmarking.md).
