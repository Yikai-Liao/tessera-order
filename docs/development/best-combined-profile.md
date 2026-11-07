# Best combined: actual phase, traffic and worker-count study

## Findings

All **1,152 full-matrix sorts** and **48 bounded no-overlap sorts** passed complete reference verification. The 48 small-screen sorts also passed. The full run held exclusive FD9 affinity 0–7 from **07:57:01 to 07:59:31 UTC on 2026-11-07**; the bounded follow-up held it from **08:00:17 to 08:00:27**. Builds used the same lock in shared mode. All results use one unchanged release binary. No source in the shared repository was modified.

1. The frozen old-core combined algorithm still has a large real advantage over both current-main scalar and current-main portable-SIMD serial on 10M random row16/u64. At W8 its paired speedups against SIMD are **2.881× and 2.778×**, respectively, with all nine blocks agreeing and effects far beyond A/A and placement controls. These are same-input, same-record-layout, same-binary comparisons, not historical cross-driver numbers.
2. Public worker-count scaling is useful but not linear: random C medians at W1/2/4/8 are **155.152/90.857/62.055/47.297 ms** for row16 and **89.915/61.024/35.079/29.916 ms** for u64. W1 falls back to serial; W2/4/8 change root shard count and sometimes B. These are not fixed-decomposition parallel speedups.
3. At W8 the remaining critical path is chiefly root permutation plus serial squeeze, not a long unfinished child tail. Row16 P permutation/squeeze/tail medians are **15.054/27.213/0.434 ms**; u64 **11.035/14.294/0.477 ms**. The copy-job execution portion dominates permutation planning. Squeeze overlaps almost all child work, so it is not an isolated sequential cost.
4. Low-cardinality d16 shows no demonstrated W4→W8 improvement. C medians change **35.918→37.136 ms** for row16 and **22.927→24.267 ms** for u64 amid broad controls. The single-digit route has no child sorting to overlap. More workers reduce some earlier stages while squeeze does not improve.
5. Exact root record-copy requests are about 3n cells; counted child operations add about 4.96n cells on random data. Root evacuation is **zero in every parallel profile** because the preceding permutation already makes the directory safe. Optimizing that final squeeze must not reintroduce enough snapshots/copy work to consume its available end-to-end margin.

## End-to-end table

Independent medians, milliseconds. C is the per-block mean of C1 and C2, then the median across blocks. Tight is the frozen strong old-core scalar; main and SIMD are commit 72c298494ef382246aa83d08f54193cc3896de07. SIMD means the portable-simd feature, default release target flags. N=10,000,000 in every row.

| Case | C | Tight | Main default | Main SIMD | A2/A1 range | Placement/C range |
|---|---:|---:|---:|---:|---|---|
| row16 d16 W1 | 75.398 | 60.595 | 68.864 | 68.224 | 0.931–1.090 | 0.892–1.076 |
| row16 d16 W2 | 47.279 | 57.730 | 65.131 | 66.282 | 0.776–1.209 | 0.927–1.478 |
| row16 d16 W4 | 35.918 | 57.254 | 64.255 | 64.475 | 0.884–1.478 | 0.877–1.128 |
| row16 d16 W8 | 37.136 | 58.281 | 64.899 | 64.824 | 0.743–1.894 | 0.664–1.510 |
| row16 random W1 | 155.152 | 149.892 | 150.085 | 139.914 | 0.776–1.037 | 0.779–1.103 |
| row16 random W2 | 90.857 | 144.721 | 147.093 | 136.052 | 0.954–1.331 | 0.725–1.163 |
| row16 random W4 | 62.055 | 140.607 | 140.685 | 129.634 | 0.854–1.397 | 0.891–1.197 |
| row16 random W8 | 47.297 | 139.279 | 139.627 | 129.313 | 0.938–1.279 | 0.881–1.379 |
| u64 d16 W1 | 38.043 | 33.771 | 38.123 | 36.541 | 0.946–1.170 | 0.913–1.578 |
| u64 d16 W2 | 27.661 | 31.883 | 36.188 | 36.603 | 0.822–1.090 | 0.843–1.019 |
| u64 d16 W4 | 22.927 | 29.804 | 33.247 | 33.079 | 0.582–1.956 | 0.679–1.191 |
| u64 d16 W8 | 24.266 | 32.218 | 35.293 | 34.588 | 0.766–1.360 | 0.550–1.126 |
| u64 random W1 | 89.915 | 90.161 | 91.741 | 85.220 | 0.914–1.039 | 0.987–1.048 |
| u64 random W2 | 61.024 | 90.796 | 93.684 | 85.149 | 0.663–1.396 | 0.780–1.034 |
| u64 random W4 | 35.079 | 89.493 | 91.921 | 86.342 | 0.921–1.344 | 0.840–1.315 |
| u64 random W8 | 29.916 | 92.270 | 92.846 | 85.073 | 0.790–1.233 | 0.823–1.234 |

## Paired comparison audit

The duplicated baseline in this driver is C. The guide statistic for each serial variant is therefore serial / mean(C1,C2). A candidate win is marked only if the median exceeds the conservative two-sided A/A and placement envelope max(max_ratio, 1/min_ratio) and at least 8/9 blocks have serial/C > 1. All ratios are computed within a block, never reconstructed from independent median times. This is a targeted diagnostic study, not a complete core/release suite.

| Case | Tight/C | Main/C | SIMD/C | C faster blocks (tight/main/SIMD) | Clears controls (tight/main/SIMD) |
|---|---:|---:|---:|---|---|
| row16 d16 W1 | 0.813 | 0.919 | 0.907 | 0/9; 1/9; 0/9 | no / no / no |
| row16 d16 W2 | 1.252 | 1.441 | 1.488 | 9/9; 9/9; 9/9 | no / no / yes |
| row16 d16 W4 | 1.599 | 1.826 | 1.783 | 9/9; 9/9; 9/9 | yes / yes / yes |
| row16 d16 W8 | 1.543 | 1.809 | 1.728 | 9/9; 9/9; 9/9 | no / no / no |
| row16 random W1 | 0.955 | 0.977 | 0.889 | 1/9; 2/9; 0/9 | no / no / no |
| row16 random W2 | 1.593 | 1.592 | 1.504 | 9/9; 9/9; 9/9 | yes / yes / yes |
| row16 random W4 | 2.283 | 2.274 | 2.080 | 9/9; 9/9; 9/9 | yes / yes / yes |
| row16 random W8 | 2.969 | 3.036 | 2.881 | 9/9; 9/9; 9/9 | yes / yes / yes |
| u64 d16 W1 | 0.899 | 1.016 | 0.974 | 0/9; 5/9; 2/9 | no / no / no |
| u64 d16 W2 | 1.154 | 1.291 | 1.323 | 9/9; 9/9; 9/9 | no / yes / yes |
| u64 d16 W4 | 1.287 | 1.450 | 1.439 | 8/9; 8/9; 8/9 | no / no / no |
| u64 d16 W8 | 1.254 | 1.427 | 1.415 | 7/9; 8/9; 9/9 | no / no / no |
| u64 random W1 | 0.998 | 1.026 | 0.948 | 4/9; 7/9; 0/9 | no / no / no |
| u64 random W2 | 1.471 | 1.553 | 1.428 | 9/9; 9/9; 9/9 | no / yes / no |
| u64 random W4 | 2.569 | 2.663 | 2.428 | 9/9; 9/9; 9/9 | yes / yes / yes |
| u64 random W8 | 3.065 | 3.066 | 2.778 | 9/9; 9/9; 9/9 | yes / yes / yes |

## Instrumented phase table

All phase numbers come from P, not C. Independent medians are **not additive**. Public W1 uses serial fallback: all native-root phases are **N/A**, not zero. Random takes the general width-8 pipeline; d16 takes the width-4 native single-digit path, with no child intervals.

| Case | B / shards | P total ms | Scatter | Permutation | Squeeze | Child tail | Squeeze/child overlap |
|---|---|---:|---:|---:|---:|---:|---:|
| row16 d16 W1 | serial fallback | 75.187 | N/A | N/A | N/A | N/A | N/A |
| row16 d16 W2 | 442 / 2 | 44.778 | 13.262 | 11.453 | 13.189 | N/A | N/A |
| row16 d16 W4 | 313 / 4 | 36.505 | 8.102 | 8.829 | 13.647 | N/A | N/A |
| row16 d16 W8 | 221 / 8 | 33.788 | 5.938 | 6.626 | 16.014 | N/A | N/A |
| row16 random W1 | serial fallback | 153.220 | N/A | N/A | N/A | N/A | N/A |
| row16 random W2 | 111 / 2 | 93.784 | 14.501 | 19.233 | 19.630 | 38.772 | 19.440 |
| row16 random W4 | 79 / 4 | 56.929 | 9.956 | 17.048 | 26.570 | 9.331 | 26.411 |
| row16 random W8 | 64 / 8 | 52.336 | 6.272 | 15.054 | 27.213 | 0.434 | 26.815 |
| u64 d16 W1 | serial fallback | 38.168 | N/A | N/A | N/A | N/A | N/A |
| u64 d16 W2 | 625 / 2 | 27.659 | 10.421 | 7.506 | 6.525 | N/A | N/A |
| u64 d16 W4 | 442 / 4 | 20.293 | 5.398 | 4.433 | 7.440 | N/A | N/A |
| u64 d16 W8 | 313 / 8 | 23.648 | 6.216 | 4.008 | 8.410 | N/A | N/A |
| u64 random W1 | serial fallback | 90.000 | N/A | N/A | N/A | N/A | N/A |
| u64 random W2 | 157 / 2 | 53.197 | 8.582 | 9.137 | 10.078 | 25.012 | 9.934 |
| u64 random W4 | 128 / 4 | 34.754 | 6.650 | 7.382 | 14.882 | 6.421 | 14.671 |
| u64 random W8 | 128 / 8 | 30.903 | 3.747 | 11.035 | 14.294 | 0.476 | 14.108 |

## Permutation internals

Milliseconds; independent medians. Snapshot/setup includes address-table construction and boundary staging. Finish includes directory rewrite and cleanup. Copy jobs transfer full physical blocks, including raw padding.

| Case | Plan | Snapshot/setup | Copy-job execute | Finish |
|---|---:|---:|---:|---:|
| row16 d16 W2 | 0.136 | 0.026 | 11.270 | 0.022 |
| row16 d16 W4 | 0.187 | 0.038 | 8.576 | 0.028 |
| row16 d16 W8 | 0.301 | 0.146 | 6.144 | 0.040 |
| row16 random W2 | 0.701 | 0.193 | 18.286 | 0.063 |
| row16 random W4 | 1.109 | 0.329 | 15.442 | 0.096 |
| row16 random W8 | 1.691 | 0.397 | 12.566 | 0.109 |
| u64 d16 W2 | 0.102 | 0.019 | 7.246 | 0.019 |
| u64 d16 W4 | 0.146 | 0.053 | 4.215 | 0.025 |
| u64 d16 W8 | 0.230 | 0.101 | 3.671 | 0.031 |
| u64 random W2 | 0.440 | 0.158 | 8.415 | 0.049 |
| u64 random W4 | 0.646 | 0.155 | 6.377 | 0.063 |
| u64 random W8 | 0.624 | 0.211 | 10.118 | 0.062 |

## Logical record-copy accounting

Decimal MB below count one-way copied bytes; read-plus-write convention is twice this amount. **This is not hardware DRAM traffic or measured bandwidth.** Key scans, metadata, write allocation, cache effects and compiler elision are not measured. Full-block permutation includes uninitialized padding. Counted nonroot traffic is a lower bound because insertion swaps/shifts and adaptive partitions/reversals are excluded. Values are identical across blocks for these deterministic inputs.

| Case | Root scatter MB | Root permutation MB | Root squeeze nonself MB | Root tail MB | Counted child MB | Counted total MB including self copies |
|---|---:|---:|---:|---:|---:|---:|
| row16 d16 W1 | N/A | N/A | N/A | N/A | N/A | 479.952 |
| row16 d16 W2 | 160.000 | 160.549 | 155.005 | 0.003 | 0.000 | 480.552 |
| row16 d16 W4 | 160.000 | 160.737 | 157.511 | 0.004 | 0.000 | 480.741 |
| row16 d16 W8 | 160.000 | 161.033 | 158.757 | 0.003 | 0.000 | 481.036 |
| row16 random W1 | N/A | N/A | N/A | N/A | N/A | 1274.497 |
| row16 random W2 | 160.000 | 161.843 | 159.686 | 0.000 | 794.727 | 1276.571 |
| row16 random W4 | 160.000 | 162.622 | 159.844 | 0.000 | 794.727 | 1277.350 |
| row16 random W8 | 160.000 | 164.235 | 159.923 | 0.000 | 794.727 | 1278.963 |
| u64 d16 W1 | N/A | N/A | N/A | N/A | N/A | 239.972 |
| u64 d16 W2 | 80.000 | 80.415 | 77.502 | 0.000 | 0.000 | 240.415 |
| u64 d16 W4 | 80.000 | 80.525 | 78.755 | 0.002 | 0.000 | 240.527 |
| u64 d16 W8 | 80.000 | 80.729 | 79.378 | 0.002 | 0.000 | 240.731 |
| u64 random W1 | N/A | N/A | N/A | N/A | N/A | 636.643 |
| u64 random W2 | 80.000 | 81.305 | 79.843 | 0.000 | 396.817 | 638.122 |
| u64 random W4 | 80.000 | 82.123 | 79.922 | 0.000 | 396.817 | 638.940 |
| u64 random W8 | 80.000 | 84.232 | 79.961 | 0.000 | 396.817 | 641.050 |

The raw block-tail counter has two instrumentation hooks, one in Sorter::new and one in BlockStorage::new, around a **single** actual tail copy. The aggregator preserves that raw counter and divides it by two. This exact normalization is documented in PROTOCOL.md; the source/binary remained frozen throughout all runs. Native root counters are N/A on public-W1 serial fallback; its total row still includes serial movement.

## Tracer and placement controls

P and Poff invoke the same compiled instrumented sorting function with recording enabled/disabled. Poff/C also contains instrumentation branch and code-placement differences and must not be labeled tracer overhead. P/Poff is noisy; **no reliable universal tracing-overhead percentage is established**.

| Case | P/Poff median | P/Poff range | Poff/C median | Poff/C range |
|---|---:|---|---:|---|
| row16 d16 W1 | 0.983 | 0.906–1.043 | 0.995 | 0.978–1.044 |
| row16 d16 W2 | 1.015 | 0.887–1.138 | 0.989 | 0.806–1.074 |
| row16 d16 W4 | 1.006 | 0.891–1.102 | 0.977 | 0.771–1.133 |
| row16 d16 W8 | 0.949 | 0.712–1.143 | 0.967 | 0.808–1.419 |
| row16 random W1 | 0.987 | 0.964–1.070 | 0.996 | 0.878–1.034 |
| row16 random W2 | 1.013 | 0.966–1.315 | 0.996 | 0.732–1.485 |
| row16 random W4 | 1.032 | 0.889–1.359 | 0.883 | 0.781–1.079 |
| row16 random W8 | 1.057 | 0.856–1.134 | 1.035 | 0.971–1.248 |
| u64 d16 W1 | 1.010 | 0.952–1.182 | 1.014 | 0.931–1.088 |
| u64 d16 W2 | 0.991 | 0.866–1.196 | 1.008 | 0.790–1.251 |
| u64 d16 W4 | 1.008 | 0.574–1.946 | 0.998 | 0.731–1.362 |
| u64 d16 W8 | 1.056 | 0.666–2.117 | 0.920 | 0.547–1.261 |
| u64 random W1 | 1.003 | 0.919–1.069 | 1.020 | 0.980–1.060 |
| u64 random W2 | 1.012 | 0.635–1.537 | 1.012 | 0.797–1.354 |
| u64 random W4 | 1.004 | 0.765–1.305 | 0.972 | 0.780–1.364 |
| u64 random W8 | 1.157 | 0.067–1.430 | 0.937 | 0.823–12.663 |

One u64 random W8 Poff sample, block0 slot5, took **426.844 ms**, versus **25.630–30.327 ms** for later Poff blocks. C2, placement, serial variants and the immediately following P/C1 calls in that block were ordinary durations. This anomalous sample is retained. There is no phase trace for a trace-disabled call, and no perf tool is installed, so its cause is **unresolved**, not asserted to be noise, cache/TLB behavior or tracer overhead. There was no explicit per-variant warmup; first-use effects are therefore also not isolated. The same C performance claims do not use Poff timings.

## Separate no-overlap diagnostic

The existing TESSERA_PIPELINE_BATCH_BYTES override was set to **1,073,741,824**, above the whole input. This defers all child batches until after root return. It also prevents spawned consumers, so **the producer drains all children serially**. This is not just overlap disabled with child parallelism preserved and is not an end-to-end candidate. Three randomized blocks per type, same binary, separate exclusive window.

| Type | Full-pipeline squeeze median ms (9 blocks) | No-overlap squeeze median ms (3 blocks) | Verified overlap ms | Child workers | No-overlap child tail ms |
|---|---:|---:|---:|---:|---:|
| row16 | 27.213 | 20.623 | 0 | 1 | 81.242 |
| u64 | 14.294 | 13.023 | 0 | 1 | 62.250 |

All 256 child intervals in each diagnostic invocation start after squeeze ends. This bounds how much the full-pipeline squeeze can be interpreted as an isolated serial stage; subtracting these independent windows is not a causal speedup estimate. A parallel squeeze must preserve the useful overlap and include its own snapshots, copy work, scheduling and downstream effects in an end-to-end test.

## Reproduction and evidence

- run.py, locked-run.sh, env.sh: exact case/lock/environment orchestration.
- harness/src/main.rs and harness/Cargo.lock: same-binary driver and resolved dependencies.
- sources/: frozen candidate, identical-source placement, diagnostic profile, tight serial and both current-main variants.
- frozen-source-manifest.json: source before diagnostic modifications; evidence/*-manifest.json: exact measured source and binary hashes.
- evidence/full-*.jsonl: all raw timings, complete common-origin events and counters.
- evidence/*-aggregate.json and .txt: complete per-block values, ranges and independent medians.
- aggregate.py/report.py: validated aggregation and this generated report.
- PROTOCOL.md: traffic semantics, inputs, route/fallback, memory exclusions and limitations.

No full-O(n) record workspace was added to the sorting algorithm. Harness inputs, clones and references are explicitly excluded from algorithm scratch. Heap allocation peaks and fixed stack use were not newly measured here. Fixed-decomposition scaling was not added, so do not reinterpret this API worker-count table as such. The virtualized host exposes CPU0–8, with experiments pinned to0–7; /proc/cpuinfo reports AMD EPYC9V74. lscpu cannot read /sys CPU metadata, and its failure is retained in manifests. perf was unavailable; cycles/cache/TLB/DRAM counters were not measured.

## Integrity identifiers

These hash manifests and raw artifacts remain outside the repository. Only this report and hashes are suitable for committing under AGENTS.md.

- Measured binary: `980f03e1e23ba0385fbf2450743b73874fb706c7bfa7382f6f91a753a75545ea`
- Compiled source hash manifest: `127359badba070375a464a25a33d1bff31ef506f9032eed6b8814d2720f7f2d3`
- Raw sample/event/counter hash manifest: `e577d0805a10bb1a3405131c3889c6ff4cc27d31df57f58dc96d89ae542a685b`
- Full run manifest: `df57fc3abaf7939051a569326a0b743597664893d30e5c6b3d4041782656d0d1`
- No-overlap run manifest: `7260eb31f03ffd24e2c492074ea643834be390f7b944fb77ca73bf817f90c0f1`
- Harness source: `69ef7f4d40a31a62fe58e6bae52da8a16e679adacf49fe540ac1b946ddc3e106`
- Full aggregate: `9457f50b8a6117585301908196dd58abd99d4e9a2a88489ada9c7d6c5c064ccb`
- No-overlap aggregate: `9d4609de6c81143902f6f081043f077464b295e94a98ca18c9e29545c20f4681`
