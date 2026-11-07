# Historical preserved-generic screen: inconclusive, no adoption

This screen applies only to the historical b1d36ce + combined-main-5b13 lineage. It does not validate current main 0bc80b49, which includes the later df56 block-size policy change. Source G and its safety evidence are unchanged.

G/O median ratios are **0.980082** for u32 reverse 65,536, **0.934177** for u64 random 10M, and **0.940433** for Row16 random 10M. All are inconclusive under the declared all-repeat/placement controls. Directional agreement is respectively 7/9, 7/9 and 8/9. Acceptance requires a median outside the full reciprocal control envelope and at least 8/9 blocks agreeing. These results establish neither a causal improvement nor performance equivalence.

The older fallback regression is not reproduced as an outside-control finding in this new binary. Here N/O is 1.202614 with 9/9 slower, but the identical-original placement copy has placement/O = 1.195353 with 9/9 slower, and the full envelope is [0.671706973857675, 1.4887444062950663]. The older observation is retained; this is not a proved fix or an explanation of its cause.

Exactly **390 rows** are retained and verified: 39 warmup rows and 351 measurement rows across three cases. No sample is censored or discarded, and all reported control ranges and tails appear below. The aggregate ratios are elapsed_ns/sort_count, never total batch time. Raw per-sort data, driver, binaries and full source/protocol freeze remain external; [external-evidence.json](ordered-map-preserved.external-evidence.json) identifies their retained paths and checksums.

## Exact experiment

One frozen binary, 13 labels, fixed case order u32/reverse65,536, u64/random10,000,000, Row16/random10,000,000; W8, process affinity 0–7 inherited by workers, no individual-thread pinning. SplitMix64 seed 1. All 3 fixtures match the old driver key and complete-record fingerprints. One complete verified randomized warmup plus 9 randomized blocks per case. O/N/R/G/A/P pairs share a match arm and closure; the original placement package is independent. All packages enable common rayon+portable-simd, including strong serial A.

u32 samples accumulate individually timed fresh sorts to 25 ms, capped at 65,536 sorts or a soft 5 s wall time. The 10M cases use one API call per sample. The tables use elapsed_ns/sort_count, not total batch time. Worker clone/first-touch and complete reference/payload verification are outside timing; API scratch allocation is inside. All 390 warmup/measurement rows verified, no censoring or sample below 5 ms, and all tails retained. No rerun-to-win or cross-binary absolute-time comparison.

O = original combined 9586fc287e595da719ec8c958f3e428254459599b3ba2306ffe1e234058d76b8. N = old map-control 331452a13f64dd430ddf79c60ed7056856ab979649a644712c303debfb01620a. R = exact N except one ordered-wrapper true→false call. G = preserved-generic 6541fbba6b06bdd755fb4b3074d4a5cfcd86ec3399192bafa49bb53c0431e04a. P = public 0e50926e873b014de1d37a67a6da66da368010d0. A = original strongest serial public API, not a weakened-feature build.

Binary SHA256: 37cca0126753c7f928b751c263b2c01d56a7418c1294d6fc6c6614035427367b. 162 source/protocol files, dependency metadata/features, toolchain and exact source lineage are frozen. G's entire block.rs SHA256: d218329dcd1d4ab7f8f953e172c9ca8ba772a5b07fbb14d15ca8b8acd35f4078 is byte-identical to O, not merely text-normalized.

## u32 / reverse / n = 65,536

| Label | Independent median ms/sort |
|---|---:|
| O1 | 0.085633 |
| O2 | 0.085974 |
| N1 | 0.104171 |
| N2 | 0.104765 |
| R1 | 0.085843 |
| R2 | 0.085401 |
| G1 | 0.085083 |
| G2 | 0.085361 |
| placement | 0.103828 |
| A1 | 0.084575 |
| A2 | 0.084197 |
| P1 | 0.104794 |
| P2 | 0.103350 |

| Ratio | Median | Min | Max | Below 1 / above 1 blocks |
|---|---:|---:|---:|---:|
| G/O | 0.980082 | 0.875542 | 1.061172 | 7 / 2 |
| N/O | 1.202614 | 1.171089 | 1.263068 | 0 / 9 |
| R/O | 0.983446 | 0.836948 | 1.046077 | 5 / 4 |
| N/R | 1.217310 | 1.119506 | 1.509135 | 0 / 9 |
| G/N | 0.812142 | 0.693186 | 0.882388 | 9 / 0 |
| G/R | 0.989562 | 0.946715 | 1.046112 | 5 / 4 |
| G/A | 0.991791 | 0.967179 | 1.077955 | 5 / 4 |
| O2/O1 | 1.006512 | 0.813106 | 1.402029 | 3 / 6 |
| G2/G1 | 0.992358 | 0.927295 | 1.026126 | 6 / 3 |
| N2/N1 | 1.012928 | 0.671707 | 1.075776 | 4 / 5 |
| R2/R1 | 1.000585 | 0.814010 | 1.049747 | 4 / 5 |
| placement/O | 1.195353 | 1.130301 | 1.328083 | 0 / 9 |
| A2/A1 | 0.995525 | 0.936400 | 1.070967 | 5 / 4 |
| P2/P1 | 0.985769 | 0.942856 | 1.029870 | 7 / 2 |

Full reciprocal control envelope: [0.671706973857675, 1.4887444062950663]. G/O decision: inconclusive. The map/refactor comparisons G/O, N/O, R/O, N/R, G/N, G/R also remain inside controls or lack the required agreement. G/A is separately reported parallel-versus-strong-serial scaling, not a map-elision effect.

## u64 / random / n = 10,000,000

| Label | Independent median ms/sort |
|---|---:|
| O1 | 32.689777 |
| O2 | 33.882520 |
| N1 | 29.108975 |
| N2 | 32.936993 |
| R1 | 34.862372 |
| R2 | 32.458472 |
| G1 | 29.830164 |
| G2 | 32.575317 |
| placement | 35.493714 |
| A1 | 92.204193 |
| A2 | 93.009841 |
| P1 | 53.030099 |
| P2 | 49.224588 |

| Ratio | Median | Min | Max | Below 1 / above 1 blocks |
|---|---:|---:|---:|---:|
| G/O | 0.934177 | 0.851656 | 1.522128 | 7 / 2 |
| N/O | 0.973109 | 0.818350 | 2.152104 | 6 / 3 |
| R/O | 0.992956 | 0.794761 | 1.194852 | 5 / 4 |
| N/R | 0.999650 | 0.832967 | 1.905766 | 5 / 4 |
| G/N | 0.998297 | 0.707274 | 1.115466 | 5 / 4 |
| G/R | 0.950826 | 0.851336 | 1.347900 | 6 / 3 |
| G/A | 0.336724 | 0.286601 | 0.498051 | 9 / 0 |
| O2/O1 | 1.016944 | 0.866981 | 1.343044 | 4 / 5 |
| G2/G1 | 1.005543 | 0.486558 | 1.450586 | 4 / 5 |
| N2/N1 | 1.062010 | 0.305593 | 1.533314 | 3 / 6 |
| R2/R1 | 0.931046 | 0.707412 | 1.111117 | 6 / 3 |
| placement/O | 1.094134 | 0.946539 | 1.666359 | 2 / 7 |
| A2/A1 | 0.995899 | 0.929559 | 1.167261 | 5 / 4 |
| P2/P1 | 0.951635 | 0.528525 | 1.332232 | 5 / 4 |

Full reciprocal control envelope: [0.3055933948242635, 3.272322036198022]. G/O decision: inconclusive. The map/refactor comparisons G/O, N/O, R/O, N/R, G/N, G/R also remain inside controls or lack the required agreement. G/A is separately reported parallel-versus-strong-serial scaling, not a map-elision effect.

## Row16 / random / n = 10,000,000

| Label | Independent median ms/sort |
|---|---:|
| O1 | 49.230658 |
| O2 | 49.891972 |
| N1 | 46.254785 |
| N2 | 48.791538 |
| R1 | 50.222283 |
| R2 | 47.452299 |
| G1 | 47.357769 |
| G2 | 47.301886 |
| placement | 49.459761 |
| A1 | 142.590465 |
| A2 | 144.788085 |
| P1 | 79.926223 |
| P2 | 80.271580 |

| Ratio | Median | Min | Max | Below 1 / above 1 blocks |
|---|---:|---:|---:|---:|
| G/O | 0.940433 | 0.877437 | 1.007921 | 8 / 1 |
| N/O | 0.971457 | 0.916467 | 1.012211 | 6 / 3 |
| R/O | 0.972542 | 0.914638 | 1.010472 | 6 / 3 |
| N/R | 0.994440 | 0.957998 | 1.104548 | 6 / 3 |
| G/N | 0.972253 | 0.926990 | 1.013157 | 8 / 1 |
| G/R | 0.970602 | 0.902209 | 1.062894 | 6 / 3 |
| G/A | 0.324045 | 0.303512 | 0.346648 | 9 / 0 |
| O2/O1 | 1.012548 | 0.864169 | 1.143746 | 2 / 7 |
| G2/G1 | 0.977001 | 0.906109 | 1.184891 | 5 / 4 |
| N2/N1 | 1.077055 | 0.952789 | 1.228003 | 2 / 7 |
| R2/R1 | 0.968987 | 0.812048 | 1.057178 | 5 / 4 |
| placement/O | 0.952341 | 0.777325 | 1.072110 | 5 / 4 |
| A2/A1 | 1.000157 | 0.970637 | 1.073503 | 4 / 5 |
| P2/P1 | 1.004321 | 0.830555 | 1.115411 | 4 / 5 |

Full reciprocal control envelope: [0.7773246334946566, 1.2864637976340088]. G/O decision: inconclusive. The map/refactor comparisons G/O, N/O, R/O, N/R, G/N, G/R also remain inside controls or lack the required agreement. G/A is separately reported parallel-versus-strong-serial scaling, not a map-elision effect.

## Safety, mechanism and limits

The unchanged G source passed the owner's 78 native tests, 12 focused strict-provenance Miri tests, lint/docs and scoped fallback/MSRV/no_std gates. See [VALIDATION.json](ordered-map-preserved.VALIDATION.json) for exact limits. Benchmark preflight separately covered all 11 distributions for three types, mutation/reset/corruption/stability checks, independent fixture fingerprints and exact warmups. Packaging rechecked all 390 row flags/counts, raw checksums and ratio-table values; it did not build or rerun any sort.

Read-only disassembly of the exact new binary, documented externally in evidence/asm/MECHANISM.md, shows the G u64/Row16 ordered bodies omit inverse construction and four u32 map stores while keeping per-block movement/callback cadence. This establishes the intended code change, not saved traffic, saved milliseconds, global layout identity or fallback equivalence.

A separate independent read-only audit was reported to the coordinator by message, with **no standalone audit artifact**. It concerns the **older u32 binary** SHA-256 9bde84502c268270ff58badd3b0ef506263891e6d17973377abba61b9c8d93b0, not the new narrow binary. The audit traced the actual O/N/R reverse input through the descending probe and inline reverse directly to return, with neither compact nor visit executed. It reported that the reverse SIMD hot loops were the same 77 bytes (SHA-256 fed17f807c56df8a7c94fd899d43d73c7548c989a79cf8dbacbd3e78ed4c38fa). Separately, after excluding padding and normalizing relocation/local branch targets, the non-executed generic compact bodies had the same 602-instruction sequence, 0xd8 stack frame and 20 static call sites, with no extra retained N/R wrapper call. These audit details are communicated findings, not a fresh audit reproduced by this packet.

Thus an executed extra compact/wrapper call is not an explanation of this reverse case. A specific code-layout, cache or instruction-cost cause has not been established. The new screen is a performance check, not confirmation of a known fix. No cross-binary absolute-time comparison is made.

All tails are retained, including u64 N1/N2 block 0 at 107,780,448 / 32,936,993 ns and G1/G2 block 0 at 66,950,538 / 32,575,317 ns. Their complete repeat ranges are in the tables above, with all other repeat and placement controls. No tail has been explained away or rerun to obtain a preferred result. G/A is reported separately as parallel-versus-strong-serial scaling, not as a map-elision effect.

## Historical pause remains in force

The old 264-cell matrix remains paused at 139 completed matrix cells plus two primary cells, with 125 planned matrix cells unstarted. All started cells and their samples were preserved. Completed-case route checks found that 25/139 matrix cells and both primaries exercised ordered entry; the rest are fallback-only. This three-case screen neither completes nor restarts that matrix. No separate heap measurement or production adoption is supported.

The benchmark owner recorded exclusive narrow timing from 11:24:49 to 11:25:30Z on the preserved FD9 device 27 / inode 1578019. This archival update did no build or timing, held no benchmark lock, and did not modify source freezes, shared checkout or GitHub. The [revision note](ordered-map-preserved.REVISION.md) preserves the prior source-only packet identity.
