<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Ordinary unchecked cursor direction screen

Verdict: do not adopt this control. Removing the actual cursor bounds check did not establish a stable performance gain. The u64 paired result is mixed and approximately neutral; Row16 points slower than the checked aligned root, while its checked A/A controls are too wide to support a precise regression magnitude. This three-block single-placement screen is directional only. No additional measurements were taken.

All 64 calls passed complete stable-reference equality and fingerprints (32 per record type, including eight warmup calls each). All 48 measured calls and every tail are retained. Input data remained unchanged. Both aligned controls had the prescribed actual P=8, radix=256, B=128 for u64 and B=64 for Row16, path restore and public children. Native and serial use the same frozen current-native package. Duplicate labels use identical function bodies.

Ratios below use the geometric mean of the two same-function labels within each block, then the geometric mean across the three measured block ratios. Lower U/C or U/N favors unchecked. N/S measures parallel-native time divided by same-package serial time. A/A is label2/label1 within each measured block. Warmups are retained but excluded from these summaries.

## Paired direction ratios

| Type | U/C, blocks | U/C aggregate | U/N, blocks | U/N aggregate | N/S, blocks | N/S aggregate |
|---|---|---:|---|---:|---|---:|
| row16 | 1.1109, 1.1564, 1.0537 | 1.1062 | 0.8996, 1.0736, 0.8324 | 0.9298 | 0.5040, 0.3891, 0.4189 | 0.4347 |
| u64 | 1.1059, 0.9351, 1.0092 | 1.0144 | 1.2787, 1.1535, 1.2400 | 1.2229 | 0.2581, 0.2876, 0.2855 | 0.2767 |

## Absolute measured times

| Type | Group | Median ms | Minimum ms | Maximum ms |
|---|---|---:|---:|---:|
| row16 | C | 43.965 | 36.584 | 51.411 |
| row16 | N | 51.190 | 46.009 | 58.299 |
| row16 | S | 122.119 | 112.267 | 124.903 |
| row16 | U | 51.590 | 41.242 | 52.696 |
| u64 | C | 30.107 | 25.471 | 31.757 |
| u64 | N | 22.977 | 20.877 | 28.524 |
| u64 | S | 87.462 | 82.393 | 91.629 |
| u64 | U | 29.463 | 27.560 | 32.291 |

N = strong current-native parallel, S = same-package serial, C = checked aligned, U = unchecked aligned. Medians summarize six individual measured calls per group and are not used to compute the paired ratios.

## A/A controls

| Type | Group | Three measured ratios | Range |
|---|---|---|---|
| row16 | C | 0.8540, 0.7645, 0.8186 | 0.7645–0.8540 |
| row16 | N | 0.9822, 0.9030, 1.0046 | 0.9030–1.0046 |
| row16 | S | 1.0428, 0.9927, 0.9685 | 0.9685–1.0428 |
| row16 | U | 0.9970, 0.9731, 1.0674 | 0.9731–1.0674 |
| u64 | C | 1.0880, 1.0427, 0.9730 | 0.9730–1.0880 |
| u64 | N | 1.0051, 0.7319, 1.2420 | 0.7319–1.2420 |
| u64 | S | 1.0595, 1.0602, 0.9617 | 0.9617–1.0602 |
| u64 | U | 1.0550, 0.9587, 0.9584 | 0.9584–1.0550 |

In particular, Row16 C2/C1 is below one in all three blocks despite sharing the exact same function. That control variation limits causal interpretation and is retained, not discarded or rescued with more calls.

## Scope and evidence

- Exact source change: one cursor indexing expression and its SAFETY comment. Baseline/leaf/native/source identities are unchanged.
- Optimized correctness: eight tests passed, including 7,776 all-radix full stable-reference sorts, 36 low-digit stable partitions and the five existing tests; driver self-test added 16 ordinary stable sorts.
- Scoped Miri attempt 3: four exact filters passed with exit 0 under strict provenance, symbolic alignment, default Stacked Borrows/leak checks. The scoped-thread movement checks do not claim Miri Rayon integration. Attempts 1 and 2 remain failed and unchanged. Version-label and cached extern-format repairs were separately reviewed and frozen.
- Assembly: exact ordinary binary removed the cursor length load/compare/branch for both types. Single-record traversal, 40-byte cursor stride, full test and payload copy remain. Register routing changed, stack reservation grew by 16 bytes, and Row16 payload loading moved. Performance cannot be attributed solely to removing one branch.
- One exclusive FD9 timing window completed and was released before analysis. No retries, extra variants, thresholds, distributions, repetitions or independent placement were added.
- Paired native-versus-serial speedups remain approximately 3.61x for u64 and 2.30x for Row16. This does not establish an overall or generic algorithm winner.

Source plan SHA256: 39e9c0a7fd4234b1b15c61a2dae30928987ae7475fd1d6d0b60c045336d6a0d0

Screen binary SHA256: a5927cbf9eee1c3a0b97c9a3f5bf80762d5394caaa59a96aea7134b2d86f2762

Timing result SHA256: 7c5feb02a0b633cae1dcbb4a8dae02899ec23526f228fefa5ee9ec6237a679ea

Successful Miri result SHA256: c9f27551cf03c8326f0b0b8e81e809a357a407c9f4ce93fa23e11407790377a5

Analysis SHA256: 7107f117212950b1031966cfcc1840b9ee3bb1f15de36a02a5f8275625cb3afb

Original report SHA256: a1ae945ccc6b43b6ec7d140ba7ca1754694ea57ea5d34d9e64f88831c4a25d73.

Publication omits private execution-resource details and raw observations; original evidence is unchanged. This is a rejected research control, not runtime adoption.
