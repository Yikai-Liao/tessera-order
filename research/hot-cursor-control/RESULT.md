<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Explicit hot-writer core direction screen

Verdict: close this core experiment without adoption. Realizing the intended 16-byte checked hot writers and explicit four/two inlined source schedule did not establish an H/C performance gain. u64 is mixed and approximately neutral by the declared paired estimator; Row16 is slower than checked C in all three blocks. This is a three-block, one-placement directional screen, not formal acceptance. No further samples, parameters or reruns were taken.

All 64 calls passed full stable-reference equality and payload fingerprints. All eight warmups per type, 48 measured calls and every tail remain in the raw JSONL files. The Row16 native 163.109216 ms sample is retained; its same-function pair is 51.019579 ms, making that block a wide A/A control, not evidence for a precise H/N advantage.

N = strong current-native parallel, S = same-package serial, C = original checked aligned root, H = checked split-writer explicit-group root. Duplicate labels call identical function bodies and nominal record types. Input is full-random 10,000,000 records, seed 1; actual worker pool W=8; one fixed controller/worker placement. Both aligned roots use P=8, R=256, shift56, natural path restore, Public children; B=128 for u64 and B=64 for Row16. Selected and detected ISA are AVX512. Input clone occurs on the actual worker before timing; Stats/API drops remain within timing and full output/payload checks occur afterward.

Ratios use sqrt(t1*t2) within each label pair/block, followed by geometric mean of the three block ratios. Lower H/C or H/N favors H. Absolute medians use six individual measured calls and are not the paired estimator. Warmups are excluded from summaries but preserved.

## Paired ratios

| Type | Ratio | Block 0 | Block 1 | Block 2 | Geometric aggregate |
|---|---|---:|---:|---:|---:|
| u64 | C/N | 1.238032 | 1.312618 | 1.246412 | 1.265253 |
| u64 | H/C | 0.973081 | 1.020005 | 1.043296 | 1.011703 |
| u64 | H/N | 1.204706 | 1.338877 | 1.300376 | 1.280060 |
| u64 | H/S | 0.314963 | 0.374449 | 0.389426 | 0.358118 |
| u64 | N/S | 0.261444 | 0.279674 | 0.299472 | 0.279766 |
| row16 | C/N | 0.750157 | 0.411606 | 0.852792 | 0.640952 |
| row16 | H/C | 1.073689 | 1.173806 | 1.097015 | 1.114027 |
| row16 | H/N | 0.805435 | 0.483146 | 0.935526 | 0.714038 |
| row16 | H/S | 0.303793 | 0.370108 | 0.347424 | 0.339304 |
| row16 | N/S | 0.377179 | 0.766038 | 0.371368 | 0.475190 |

## Absolute times

| Type | Group | Median ms | Min ms | Max ms |
|---|---|---:|---:|---:|
| u64 | N | 22.259928 | 20.318380 | 26.694066 |
| u64 | S | 79.418044 | 78.263292 | 86.906281 |
| u64 | C | 28.501471 | 24.130220 | 35.225507 |
| u64 | H | 26.472725 | 25.571507 | 36.264256 |
| row16 | N | 49.117818 | 42.256495 | 163.109216 |
| row16 | S | 119.450307 | 111.228045 | 139.466845 |
| row16 | C | 36.901221 | 35.555878 | 38.120899 |
| row16 | H | 40.071742 | 37.454367 | 48.060026 |

## Raw and symmetric A/A controls

Raw A/A is label2/label1 in each block, retaining direction. The separate symmetric envelope normalizes each individual call by its arithmetic pair mean: e = abs(t2-t1)/(t1+t2), envelope = [1-e, 1+e]. The listed envelope spans all three blocks using their maximum e. It is not the raw A2/A1 range and is not a statistical confidence interval. Per-block normalized envelopes are retained in analysis.json.

| Type | Group | Raw block 0 | Raw block 1 | Raw block 2 | Raw range | Symmetric pair-mean envelope |
|---|---|---:|---:|---:|---|---|
| u64 | N | 0.888000 | 0.907827 | 1.233621 | 0.888000–1.233621 | 0.895407–1.104593 |
| u64 | S | 0.900548 | 1.001969 | 1.049421 | 0.900548–1.049421 | 0.947672–1.052328 |
| u64 | C | 0.818270 | 1.459809 | 0.842322 | 0.818270–1.459809 | 0.813071–1.186929 |
| u64 | H | 0.997265 | 0.739417 | 0.742725 | 0.739417–0.997265 | 0.850189–1.149811 |
| row16 | N | 1.036406 | 0.312794 | 0.991970 | 0.312794–1.036406 | 0.476532–1.523468 |
| row16 | S | 0.871571 | 1.119637 | 0.947869 | 0.871571–1.119637 | 0.931379–1.068621 |
| row16 | C | 1.047840 | 0.970180 | 1.035505 | 0.970180–1.047840 | 0.976639–1.023361 |
| row16 | H | 0.991489 | 1.189040 | 0.890440 | 0.890440–1.189040 | 0.913642–1.086358 |

u64 C and H controls are wide, so the approximately 1.012 H/C result cannot establish a small effect. Row16 H/C is 1.074, 1.174 and 1.097, while checked C controls are tighter; this points against the proposed optimization, without claiming a precise general regression. Row16 native controls are especially wide and limit interpretation of the apparent H/N ratio. A lower unpaired u64 H median does not override the predefined paired estimator. No cache cause is established.

## Exact geometry and safety

Geometry entries are [B, P, boundary blocks, scratch records, scatter records, restore records]. C and H geometry matched on every call.
- u64: [128, 8, 2037, 522880, 10000000, 9999872]
- row16: [64, 8, 2016, 260096, 10000000, 10000000]

The realized ordinary parallel routes passed independent ASM review: checked 16-byte writer indexing, four u64/two Row16 ordinary source writes with no per-record outlined calls, and before-write refill retained. This satisfies a code-generation mechanism gate, not a performance claim.

V3 optimized validation passed 10 tests, including 7,776 full stable-reference sorts, 36 partition cases and 1,485 checked/H exact stable-output, end, geometry and plan comparisons. Driver negative-oracle/self-test passed. All seven strict-provenance/default-Stacked-Borrows Miri filters passed, including grouped tails for all three payload types. Miri uses the actual mover with scoped threads, not the real Rayon dependency. Support remains sealed intrinsic-key Copy u64/Row16/Row128, not arbitrary key callbacks, Owned or generic panic recovery.

V1 outlined per-record calls and V2 retained a u64 lane loop; both mechanism failures and their artifacts are preserved. The V3 first build failed its old output cap while linking the driver; successful root/validation artifacts and all 24 partial files remain. A separately reviewed driver-only resource amendment succeeded with unchanged code/flags and fresh output/temp paths. The successful bundle explicitly retains original_build_passed=false. No failed result was rewritten.

One exclusive timing window completed and was released before analysis. No runtime adoption is justified by these findings.

## Identities

- Source plan: a56ad1441373b3af342675a64d0a3d317e4bb022dfc0b283d48c8f0178237c60
- Core: 9ad13ad1555a5ea9c3b58a7274d5bf684e5d2e7a848dedd67b4a09296a46ab2d
- Driver source: 5a5f1ed0a167e9f500220c88aeac1cecd0e92ef2cc5a5ef0e50d6f793ee36a4d
- Actual screen: bc55675a8560d0c02c942c37283317a9274c0832985d07105bab856aba8edb06
- Timing wrapper: 4f2ea79044648828c5b08044773d4d916c0f7986cc4a18e25319f55437114b36
- Timing result: b531887ef86eb0b1f3b0dec1ce95142022a728c06ab98ec5073b525774965ae4
- V3 native result: e7edfa67b98b0636691bd68a56a22c27d8634c7a64dd14e867ade38ce15e91a1
- V3 Miri result: 2d17ef0224298db9318468b9a53bb1a62d069ffe72e831f5516eb0a251a15823
- Analysis: 5812c95501111d98f13bc8fcafa8935ba9422279aa7c83806f164b9b14d3e781

Publication contains sanitized summaries and source only. Raw samples, harnesses, execution wrappers, binaries and full environment snapshots are excluded; original evidence remains unchanged.
