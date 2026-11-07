<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Negative fixed screen

Decision: no accepted scheduler gain. Stop this candidate without tuning, reruns, expanded workloads, or production adoption.

For u64 Direct, old-combined versus draining whole-call medians were 24.5545 ms versus 26.7119 ms. The median within-block new/old ratio was 1.11053, with the candidate faster in 2 of 9 blocks. For Row16 ByKey, medians were 45.4185 ms versus 49.8355 ms; the paired ratio was 1.09287, faster in 0 of 9 blocks. Paired ratios are not ratios of the displayed medians.

Conservative control envelopes were 1.43863 and 1.45983. These large envelopes prevent treating the observed ratios as controlled slowdown magnitudes. They do not justify accepting a gain. No outliers or tails were removed.

The predeclared same-binary, same-core old/new gate was primary. Current public and serial baselines were also included, but advantages over those baselines were already present in the old combined port and cannot be credited to the new scheduler.

The fixed screen used n=10,000,000, W8, seed 1, affinity CPUs 0–7, two cases, 16 treatment labels, nine measured blocks, fresh original-input clones per repetition, and at least 50 ms accumulated sorting per measured sample. A/A repeats, treatment placement, and serial controls were retained. Input allocation/reference comparison/destruction were outside sort timers. Every one of 457 calls passed complete reference equality. Repeated fixed input and warmed allocation do not represent cold-cache conditions. Unique generated keys do not exercise duplicate-key stability; separate safety tests do.

The exclusive run on 2026-11-07 UTC acquired its shared-file descriptor lock exclusively at 18:43:22 and released it at 18:44:18. The pre-run freeze remained unchanged. The screen's executable/source/method identities and report hash are listed in `PROVENANCE.json`; raw evidence and timing harness are intentionally outside this packet.

No broader distribution, W4/W1, owned-record performance, heap multiplier, internal route observation, task-count trace, cache, bandwidth, PMU, or scheduling-causality claim follows from this screen. R256 and 128 KiB batching were source/config expectations, not observed internal route witnesses. This negative result does not settle the independently different public versus combined root/core comparison.
