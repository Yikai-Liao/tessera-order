<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Selected native API scaling: W1, W2 and W4

## Result: no accepted native/public win

The six selected full-random cases completed without reruns, exclusions, source changes or a new binary. None of the native/public parallel comparisons clears its within-W A/A plus placement envelope. W2 has a concerning worse native median in both record types, including nine slower Row16 blocks out of nine, but neither meets the formal slowdown gate under broad controls. W4 Row16 is consistently faster, yet remains inside its control envelope. This phase is complete; it does not establish adoption or complete the project.

The earlier W8 findings remain unchanged: u64 passes all-group controls; Row16 passes the compared parallel controls but narrowly misses all-group sensitivity; Wide128 is inconclusive; Owned32 has a concerning worse native median. This follow-on deliberately selects the two promising10M cases, not a replacement for the earlier mixed result.

## Scope and interpretation

Same frozen native combined implementation and public research API control, not a new improvement over research combined. u64 Direct10M and repr(C) Row16 {u64 key,u64 original identity}10M, full-width SplitMix64 seed1. Four independent primary/placement package identities,12labels: public parallel P1/P2/PL, native parallel N1/N2/NL, public serial SP1/SP2/SPL and native serial SN1/SN2/SNL. Serial controls retain their respective strongest matching cores and feature context.

Each of six processes has12warmup samples and nine randomized12-label measured blocks. Every measured sample accumulates at least50ms; every constituent call receives a fresh worker-local clone outside its timer and full stable-reference checking afterward. Exact totals:648measured+72warmup samples;715measured+72warmup constituent API calls=787calls. The preceding72full-size untimed smoke calls are separate. No sample or tail was discarded.

This is an API-policy curve, not fixed-geometry pure thread scaling. W1 parallel APIs may fall back to serial. Worker-count settings can change scheduling, routing and decomposition. Each W has separately randomized blocks and its own controls; across-W observations are not paired. All comparison ratios below are medians of nine within-W block-paired A/A pair-mean ratios, not ratios of separately summarized medians.

Acceptance requires at least8/9 blocks agreeing and a ratio outside the symmetric compared-group envelope max(r,1/r), using every A/A and placement ratio. All-group sensitivity additionally includes the other two groups. All observed ISA detection/dispatch was Avx512, with the same default/std/rayon/portable-simd feature context. No quota/frequency or bandwidth inference.

## Within-W comparisons

N=native parallel; P=public parallel; SP=public serial; SN=native serial. Counts are faster blocks out of9. Both serial cores remain visible; no weaker baseline is selected after seeing results.

| Case/W | N/P ratio (faster blocks) | N/SP ratio (faster blocks) | N/SN ratio (faster blocks) | N/P compared envelope | All-group envelope |
|---|---:|---:|---:|---:|---:|
| u64/w1 | 1.014460 (3/9) | 0.988997 (7/9) | 0.962390 (6/9) | 1.282333 | 1.433801 |
| row16/w1 | 0.998813 (5/9) | 0.966330 (8/9) | 0.977867 (5/9) | 1.144194 | 1.149565 |
| u64/w2 | 1.109979 (2/9) | 0.691357 (9/9) | 0.668413 (9/9) | 1.658612 | 1.658612 |
| row16/w2 | 1.108315 (0/9) | 0.662104 (9/9) | 0.643402 (9/9) | 1.390502 | 1.413110 |
| u64/w4 | 0.969869 (8/9) | 0.425974 (9/9) | 0.415151 (9/9) | 2.709403 | 2.709403 |
| row16/w4 | 0.860899 (9/9) | 0.442233 (9/9) | 0.434456 (9/9) | 1.237603 | 1.419986 |

No N/P faster or slower gate passes, including the broader all-group sensitivity. Native beats both serial cores under their respective compared gates for Row16W2 andW4; neither native/serial comparison passes for u64W1/W2/W4 or Row16W1. Native/public serial-core shifts remain inside their controls.

u64W2 is slower in7/9blocks and Row16W2 in9/9. u64W4’s native placement maximum2.709403 is retained, widening its control envelope; it is not removed to manufacture a win.

## Descriptive absolute pair medians

Milliseconds, median of nine within-block A/A pair means. These six absolute public/native pairs describe latency; dividing columns is not the reported paired-ratio statistic. Across-W changes are descriptive independently randomized API observations, not paired speedup estimates.

| Case/W | Public parallel | Native parallel | Public serial | Native serial | Measured calls |
|---|---:|---:|---:|---:|---:|
| u64/w1 | 85.8091 | 83.1864 | 86.1448 | 88.6649 | 108 |
| row16/w1 | 123.0641 | 120.2234 | 123.1726 | 123.3851 | 108 |
| u64/w2 | 52.7908 | 61.4016 | 87.3627 | 88.5475 | 114 |
| row16/w2 | 72.7885 | 80.0354 | 118.9090 | 121.9209 | 108 |
| u64/w4 | 35.5559 | 34.4845 | 82.9505 | 82.6152 | 161 |
| row16/w4 | 59.8712 | 52.3065 | 117.2693 | 117.5698 | 116 |

## Full A/A and placement ranges

Each entry is median[min,max] over nine blocks. A/A=A2/A1; placement=independent placement divided by the primary pair mean. All controls are retained. Full raw calls and sample/constituent tail tables remain in the separately preserved evidence, not this publication packet.

| Case/W | Group | A/A | Placement |
|---|---|---|---|
| u64/w1 | N | 0.995394 [0.887785,1.282333] | 1.072850 [0.883308,1.245840] |
| u64/w1 | P | 1.000725 [0.899425,1.034045] | 0.985139 [0.806269,1.039232] |
| u64/w1 | SN | 1.037526 [0.875067,1.433801] | 0.973542 [0.895284,1.015279] |
| u64/w1 | SP | 0.945475 [0.803913,1.063808] | 0.980409 [0.864872,1.047246] |
| row16/w1 | N | 1.019169 [0.985422,1.070328] | 1.039029 [0.989095,1.144194] |
| row16/w1 | P | 0.973488 [0.927262,1.092907] | 1.001810 [0.952043,1.096256] |
| row16/w1 | SN | 0.986799 [0.921575,1.080734] | 0.989225 [0.968749,1.142402] |
| row16/w1 | SP | 0.984195 [0.869894,1.042594] | 0.980733 [0.896084,1.038290] |
| u64/w2 | N | 1.206455 [0.713441,1.658612] | 1.006089 [0.805873,1.205017] |
| u64/w2 | P | 1.022284 [0.714063,1.333129] | 1.042494 [0.764617,1.317348] |
| u64/w2 | SN | 0.967676 [0.923358,1.102158] | 0.971973 [0.921354,1.201266] |
| u64/w2 | SP | 0.977305 [0.929138,1.032636] | 0.994867 [0.907144,1.148966] |
| row16/w2 | N | 0.960311 [0.719165,1.305876] | 0.939275 [0.784009,1.055525] |
| row16/w2 | P | 1.007417 [0.982208,1.094244] | 1.079448 [0.969230,1.232001] |
| row16/w2 | SN | 0.983119 [0.707659,1.007865] | 0.995428 [0.845642,1.075708] |
| row16/w2 | SP | 0.980844 [0.732731,1.071529] | 1.010297 [0.885776,1.048294] |
| u64/w4 | N | 0.941646 [0.852666,1.181995] | 1.018515 [0.821989,2.709403] |
| u64/w4 | P | 1.030275 [0.903476,1.227877] | 0.983835 [0.914091,1.107456] |
| u64/w4 | SN | 0.998408 [0.939737,1.163023] | 1.008414 [0.905965,1.038643] |
| u64/w4 | SP | 1.034178 [0.967141,1.123198] | 1.001312 [0.971151,1.067429] |
| row16/w4 | N | 0.973904 [0.852179,1.237603] | 0.995133 [0.933263,1.055643] |
| row16/w4 | P | 0.989951 [0.813737,1.188582] | 1.000175 [0.869548,1.078305] |
| row16/w4 | SN | 1.019121 [0.704232,1.146026] | 0.986546 [0.878725,1.020916] |
| row16/w4 | SP | 1.016175 [0.925240,1.025256] | 0.971970 [0.902499,0.997006] |

## Identity and evidence links

All links below are content hashes identified by semantic labels; no machine paths are published. The accompanying identities manifest binds source, executable, input validation, complete raw streams, full analysis and tail report. Sources and original executable were verified unchanged before and after timing.

Public research API control commit:55b299b3716cd1af9e997f30b1a3c1813455d8be. Native is that control plus only the original combined patch SHA2562265e704ebb25e0dba1a08e8d4431d1473fce09a740ac5e67ee5fe0b2c4dda4b. No draining queue, R16 override or replacement algorithm.

Executable SHA256:69c008b6f10aa7f60761c4f899b542d8c547054232fdf42a3b03fd5765c7afc2. Toolchain:rustc1.99.1(58cf8f9c3), original release driver; no new build.

## Remaining qualifications

No distribution/fallback expansion, heap acceptance or adoption follows from this timing phase. The chosen u64 Direct extractor is sealed/pure, distinct from arbitrary user ByKey callbacks. Row16 uses a pure concrete closure here, not proof for every ByKey closure. A key invoking rayon::join can stack logical consumers on one physical worker; a per-consumer space bound does not prove a P-independent per-physical-thread bound. Nested sorts add separate invocation ledgers. Outstanding correctness and broader evidence gates remain separate; this report makes no aggregate-heap or few-times-serial-memory claim.
