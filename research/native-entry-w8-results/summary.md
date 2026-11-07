<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Existing native-root implementation: four-case W8 result

## Conclusion: mixed, phase completed

The existing best generic native implementation beats the frozen public parallel research API convincingly for full-random u64. Row16 passes the compared parallel controls but narrowly misses the broader all-group sensitivity test. Wide128 is inconclusive. Owned32 has a concerning worse native median and seven slower blocks out of nine, without meeting the formal slowdown gate because controls are broad. This phase is completed; the project is not complete. These results do not support broad adoption, and there were no rescue reruns or exclusions. This is adoption research on the already-known combined implementation, not a new gain over that implementation.

## Protocol and exact counts

One frozen binary, four distinct original/placement package identities,12 labels: public parallel P1/P2/PL; native parallel N1/N2/NL; public serial SP1/SP2/SPL; native serial SN1/SN2/SNL. The two entries in each A/A pair share a callsite. All variants use the same feature context; each parallel core is compared with its own strongest matching serial and the other serial core.

Each case has one randomized12-label warmup block and nine randomized12-label measured blocks. Each measured sample accumulates at least50ms of sort time, retaining every constituent call. Every call clones the original on an actual Rayon worker outside its timer, then checks full stable-reference equality outside the timer. Reference equality includes payloads and original identities, not merely keys/hashes. Whole API sort time includes scratch allocation. Controller/watchdog affinity CPU8, sorting-worker migratable affinity CPU0–7, W8; these are masks, not individual per-core thread pinning. The20ms watchdog runs on CPU8. Warm clones/reference access and allocator reuse make this a warm workload. No measured quota/frequency claim. Actual detected/selected ISA: Avx512.

SplitMix64 seed1, full-width random u64 keys throughout. u64 Direct10M; Row16 repr(C) u64 key+u64 original identity10M; Wide128262144 uses u64 key+u64 identity+112 deterministic payload bytes, not a u128-key record; Owned32262144 uses u64 key+String containing16 hexadecimal identity digits followed by :tessera-owned-payload. No casts across layouts. All private root override knobs cleared.

| Case | Measured samples | Warmup samples | Measured sort calls | Warmup sort calls |
|---|---:|---:|---:|---:|
| u64 | 108 | 12 | 165 | 12 |
| row16 | 108 | 12 | 128 | 12 |
| wide128 | 108 | 12 | 433 | 12 |
| owned32 | 108 | 12 | 2084 | 12 |

Total:432 measured samples+48warmup samples=480samples,2810 measured sort calls+48warmup calls=2858sort calls. Four input metadata records are separate from samples. All strict parsing and per-call verification passed. These counts exclude earlier untimed correctness self-tests.

## Statistical interpretation

Every comparison ratio is the median of nine same-block A/A-pair-mean ratios, never a ratio of independently calculated medians. Faster/slower acceptance requires at least8/9 blocks agreeing and the paired median outside the compared groups’ conservative A/A+placement envelope. Envelope=max(r,1/r) across every observed control ratio, including extremes. All-group sensitivity adds both other groups’ controls. A serial-control outlier is not discarded, nor assumed to be an algorithm regression.

| Case | Native/public parallel median ratio | Faster blocks | Compared envelope | Compared gate | All-group envelope | All-group sensitivity |
|---|---:|---:|---:|---|---:|---|
| u64 | 0.698241 | 9/9 | 1.207270 | faster | 1.208129 | faster |
| row16 | 0.690637 | 9/9 | 1.317210 | faster | 1.453313 | inconclusive |
| wide128 | 0.717658 | 8/9 | 2.528747 | inconclusive | 2.528747 | inconclusive |
| owned32 | 1.390650 | 2/9 | 2.664531 | inconclusive | 2.664531 | inconclusive |

Row16’s broad-sensitivity faster threshold is approximately0.688083, just below its observed0.690637. Its largest broad control deviation is a native-serial A/A ratio1.453313. Wide128’s public A/A minimum0.395453 expands its envelope to2.528747. Owned32’s native A/A maximum2.664531 and public placement maximum2.061304 are retained; its native/public paired median1.390650 and7/9 slower blocks remain concerning, not an accepted formal slowdown.

## Both strongest serial controls

| Case | Native/public serial ratio (faster blocks) | Native/native serial ratio (faster blocks) | Native serial/public serial ratio (faster blocks) |
|---|---:|---:|---:|
| u64 | 0.316037 (9/9) | 0.314725 (9/9) | 1.030062 (1/9) |
| row16 | 0.388399 (9/9) | 0.373098 (9/9) | 1.003813 (4/9) |
| wide128 | 0.519838 (9/9) | 0.523054 (9/9) | 0.982684 (7/9) |
| owned32 | 1.020374 (4/9) | 1.086031 (3/9) | 0.998149 (5/9) |

Native beats both serials under their respective compared-control gates in u64, Row16 and Wide128; neither serial comparison passes for Owned32. Native/public serial-core shifts do not pass their own slowdown/speedup gates. These are matching but distinct serial cores, not a deliberately weakened comparator.

## All comparison gates and broad sensitivity

| Case | Ratio | Median | Faster/slower blocks | Compared envelope | Compared faster/slower gate | All-group faster gate |
|---|---|---:|---|---:|---|---|
| u64 | N/P | 0.698241 | 9/0 | 1.207270 | faster | pass |
| u64 | N/SN | 0.314725 | 9/0 | 1.183282 | faster | pass |
| u64 | N/SP | 0.316037 | 9/0 | 1.208129 | faster | pass |
| u64 | P/SN | 0.459482 | 9/0 | 1.207270 | faster | pass |
| u64 | P/SP | 0.437357 | 9/0 | 1.208129 | faster | pass |
| u64 | SN/SP | 1.030062 | 1/8 | 1.208129 | inconclusive | no pass |
| row16 | N/P | 0.690637 | 9/0 | 1.317210 | faster | no pass |
| row16 | N/SN | 0.373098 | 9/0 | 1.453313 | faster | pass |
| row16 | N/SP | 0.388399 | 9/0 | 1.317210 | faster | pass |
| row16 | P/SN | 0.543388 | 9/0 | 1.453313 | faster | pass |
| row16 | P/SP | 0.535638 | 9/0 | 1.107803 | faster | pass |
| row16 | SN/SP | 1.003813 | 4/5 | 1.453313 | inconclusive | no pass |
| wide128 | N/P | 0.717658 | 8/1 | 2.528747 | inconclusive | no pass |
| wide128 | N/SN | 0.523054 | 9/0 | 1.310240 | faster | no pass |
| wide128 | N/SP | 0.519838 | 9/0 | 1.511278 | faster | no pass |
| wide128 | P/SN | 0.688745 | 8/1 | 2.528747 | inconclusive | no pass |
| wide128 | P/SP | 0.676819 | 8/1 | 2.528747 | inconclusive | no pass |
| wide128 | SN/SP | 0.982684 | 7/2 | 1.511278 | inconclusive | no pass |
| owned32 | N/P | 1.390650 | 2/7 | 2.664531 | inconclusive | no pass |
| owned32 | N/SN | 1.086031 | 3/6 | 2.664531 | inconclusive | no pass |
| owned32 | N/SP | 1.020374 | 4/5 | 2.664531 | inconclusive | no pass |
| owned32 | P/SN | 0.696517 | 9/0 | 2.061304 | inconclusive | no pass |
| owned32 | P/SP | 0.726411 | 9/0 | 2.061304 | inconclusive | no pass |
| owned32 | SN/SP | 0.998149 | 5/4 | 1.259456 | inconclusive | no pass |

## Absolute pair medians, descriptive only

Milliseconds: median of each group’s nine within-block A/A-pair means. These values describe scale; dividing columns is not the paired comparison statistic.

| Case | Public parallel | Native parallel | Public serial | Native serial |
|---|---:|---:|---:|---:|
| u64 | 37.6227 | 26.4331 | 83.2501 | 84.7512 |
| row16 | 64.2091 | 44.8931 | 118.5420 | 120.7868 |
| wide128 | 12.8613 | 9.9354 | 18.8554 | 18.6735 |
| owned32 | 2.1307 | 3.1355 | 2.9232 | 2.9815 |

## Full A/A and placement ranges

Median[min,max] across nine measured blocks. Placement compares independent placement with the primary pair mean.

| Case | Group | A/A | Placement |
|---|---|---|---|
| u64 | N | 0.962804 [0.845107,1.101349] | 0.981085 [0.878273,1.133655] |
| u64 | P | 1.041959 [0.884812,1.207270] | 1.009777 [0.963203,1.088972] |
| u64 | SN | 1.028026 [0.884124,1.097520] | 0.969414 [0.908418,1.053393] |
| u64 | SP | 0.994231 [0.974538,1.181658] | 0.991546 [0.827726,1.073959] |
| row16 | N | 1.015695 [0.759181,1.092949] | 1.048550 [0.929641,1.200699] |
| row16 | P | 1.025918 [0.977592,1.107803] | 0.993735 [0.903405,1.040046] |
| row16 | SN | 0.958319 [0.864393,1.453313] | 0.997799 [0.846626,1.132389] |
| row16 | SP | 1.012900 [0.903124,1.075278] | 0.993498 [0.916587,1.048609] |
| wide128 | N | 1.006934 [0.767667,1.107319] | 1.047012 [0.763219,1.281663] |
| wide128 | P | 1.036319 [0.395453,1.535595] | 1.037004 [0.538803,1.297373] |
| wide128 | SN | 1.010418 [0.953610,1.125228] | 1.046503 [0.942123,1.123163] |
| wide128 | SP | 1.043043 [0.661691,1.201855] | 0.969351 [0.797679,1.033790] |
| owned32 | N | 1.046063 [0.475893,2.664531] | 0.899615 [0.547395,1.134719] |
| owned32 | P | 0.970264 [0.858576,1.122331] | 0.991440 [0.912428,2.061304] |
| owned32 | SN | 1.050029 [0.929268,1.093824] | 0.982480 [0.940720,1.099988] |
| owned32 | SP | 0.987021 [0.882047,1.259456] | 0.976523 [0.923414,1.133781] |

## Every label’s constituent-sort tails

Milliseconds; measured blocks only. p90 is nearest-rank ceil(0.9×count), without interpolation or trimming.

| Case | Label | Count | Minimum | Median | p90 | Maximum |
|---|---|---:|---:|---:|---:|---:|
| u64 | N1 | 18 | 24.6645 | 26.7818 | 29.5969 | 29.6062 |
| u64 | N2 | 19 | 21.3549 | 26.0502 | 30.5081 | 35.0950 |
| u64 | NL | 20 | 19.3452 | 25.7314 | 28.1564 | 33.3064 |
| u64 | P1 | 18 | 31.0781 | 36.3365 | 39.8050 | 46.4823 |
| u64 | P2 | 18 | 32.2551 | 38.3210 | 39.7202 | 40.8028 |
| u64 | PL | 18 | 32.7982 | 37.3558 | 40.2984 | 45.7257 |
| u64 | SN1 | 9 | 81.6163 | 84.9358 | 93.3531 | 93.3531 |
| u64 | SN2 | 9 | 80.3841 | 84.7899 | 101.3028 | 101.3028 |
| u64 | SNL | 9 | 80.9381 | 84.6430 | 88.3345 | 88.3345 |
| u64 | SP1 | 9 | 78.8356 | 83.8013 | 89.0464 | 89.0464 |
| u64 | SP2 | 9 | 80.1464 | 82.2548 | 105.2224 | 105.2224 |
| u64 | SPL | 9 | 78.8502 | 82.8880 | 89.4072 | 89.4072 |
| row16 | N1 | 16 | 34.9821 | 45.4411 | 52.2547 | 55.0658 |
| row16 | N2 | 16 | 31.0148 | 44.1918 | 52.1813 | 52.4106 |
| row16 | NL | 15 | 38.9173 | 47.3228 | 52.1106 | 57.1578 |
| row16 | P1 | 9 | 53.4324 | 63.6637 | 65.7391 | 65.7391 |
| row16 | P2 | 9 | 52.7583 | 64.2660 | 70.1272 | 70.1272 |
| row16 | PL | 9 | 52.0497 | 64.8959 | 66.3407 | 66.3407 |
| row16 | SN1 | 9 | 113.2742 | 121.2789 | 133.6187 | 133.6187 |
| row16 | SN2 | 9 | 113.0320 | 115.8871 | 172.5001 | 172.5001 |
| row16 | SNL | 9 | 113.5072 | 121.4166 | 133.6256 | 133.6256 |
| row16 | SP1 | 9 | 112.5879 | 118.7933 | 126.0231 | 126.0231 |
| row16 | SP2 | 9 | 112.5078 | 119.1221 | 124.2542 | 124.2542 |
| row16 | SPL | 9 | 111.9141 | 118.1484 | 124.3334 | 124.3334 |
| wide128 | N1 | 51 | 7.7293 | 8.9801 | 11.3698 | 28.2004 |
| wide128 | N2 | 52 | 7.0761 | 9.1401 | 11.4932 | 14.5971 |
| wide128 | NL | 50 | 7.9931 | 9.5406 | 11.7498 | 15.6067 |
| wide128 | P1 | 42 | 7.9104 | 12.2553 | 14.5871 | 86.8987 |
| wide128 | P2 | 38 | 9.4269 | 13.6629 | 15.2089 | 19.3660 |
| wide128 | PL | 39 | 9.6544 | 13.8281 | 15.4770 | 18.6013 |
| wide128 | SN1 | 27 | 16.6662 | 18.3536 | 19.5204 | 22.7553 |
| wide128 | SN2 | 27 | 17.3671 | 18.5732 | 19.9119 | 22.6330 |
| wide128 | SNL | 27 | 17.1508 | 19.2021 | 20.7792 | 24.1682 |
| wide128 | SP1 | 26 | 16.6567 | 18.6109 | 21.1934 | 32.1429 |
| wide128 | SP2 | 27 | 15.2241 | 18.8832 | 20.7421 | 23.2612 |
| wide128 | SPL | 27 | 15.8559 | 18.1878 | 19.4765 | 19.7491 |
| owned32 | N1 | 166 | 1.5035 | 2.1843 | 4.6416 | 8.7664 |
| owned32 | N2 | 167 | 1.5956 | 2.0720 | 4.7385 | 15.6673 |
| owned32 | NL | 179 | 1.5276 | 2.0539 | 4.2155 | 14.3552 |
| owned32 | P1 | 213 | 1.5720 | 1.9817 | 2.3251 | 10.2308 |
| owned32 | P2 | 220 | 1.6153 | 1.9966 | 2.4269 | 4.8252 |
| owned32 | PL | 202 | 1.5623 | 1.9539 | 2.4872 | 28.0366 |
| owned32 | SN1 | 159 | 2.6268 | 2.8217 | 3.1324 | 6.3447 |
| owned32 | SN2 | 153 | 2.6262 | 2.9536 | 3.3066 | 5.2378 |
| owned32 | SNL | 156 | 2.6543 | 2.8842 | 3.2218 | 4.9161 |
| owned32 | SP1 | 159 | 2.5897 | 2.8133 | 3.2510 | 7.4031 |
| owned32 | SP2 | 157 | 2.6064 | 2.8181 | 3.3936 | 6.0054 |
| owned32 | SPL | 153 | 2.5861 | 2.8218 | 3.4980 | 7.9775 |

## Every label’s accumulated-sample mean tails

Milliseconds; measured blocks only. p90 is nearest-rank ceil(0.9×count), without interpolation or trimming.

| Case | Label | Count | Minimum | Median | p90 | Maximum |
|---|---|---:|---:|---:|---:|---:|
| u64 | N1 | 9 | 25.1966 | 26.3476 | 29.2373 | 29.2373 |
| u64 | N2 | 9 | 24.2141 | 26.5930 | 28.6064 | 28.6064 |
| u64 | NL | 9 | 23.6933 | 25.7314 | 29.7470 | 29.7470 |
| u64 | P1 | 9 | 31.8867 | 36.3365 | 43.1437 | 43.1437 |
| u64 | P2 | 9 | 32.3788 | 38.1856 | 39.8105 | 39.8105 |
| u64 | PL | 9 | 34.3741 | 38.6570 | 39.8312 | 39.8312 |
| u64 | SN1 | 9 | 81.6163 | 84.9358 | 93.3531 | 93.3531 |
| u64 | SN2 | 9 | 80.3841 | 84.7899 | 101.3028 | 101.3028 |
| u64 | SNL | 9 | 80.9381 | 84.6430 | 88.3345 | 88.3345 |
| u64 | SP1 | 9 | 78.8356 | 83.8013 | 89.0464 | 89.0464 |
| u64 | SP2 | 9 | 80.1464 | 82.2548 | 105.2224 | 105.2224 |
| u64 | SPL | 9 | 78.8502 | 82.8880 | 89.4072 | 89.4072 |
| row16 | N1 | 9 | 38.4341 | 44.7557 | 55.0658 | 55.0658 |
| row16 | N2 | 9 | 31.3476 | 44.6159 | 52.1813 | 52.1813 |
| row16 | NL | 9 | 41.8935 | 48.3194 | 52.1106 | 52.1106 |
| row16 | P1 | 9 | 53.4324 | 63.6637 | 65.7391 | 65.7391 |
| row16 | P2 | 9 | 52.7583 | 64.2660 | 70.1272 | 70.1272 |
| row16 | PL | 9 | 52.0497 | 64.8959 | 66.3407 | 66.3407 |
| row16 | SN1 | 9 | 113.2742 | 121.2789 | 133.6187 | 133.6187 |
| row16 | SN2 | 9 | 113.0320 | 115.8871 | 172.5001 | 172.5001 |
| row16 | SNL | 9 | 113.5072 | 121.4166 | 133.6256 | 133.6256 |
| row16 | SP1 | 9 | 112.5879 | 118.7933 | 126.0231 | 126.0231 |
| row16 | SP2 | 9 | 112.5078 | 119.1221 | 124.2542 | 124.2542 |
| row16 | SPL | 9 | 111.9141 | 118.1484 | 124.3334 | 124.3334 |
| wide128 | N1 | 9 | 8.2842 | 9.7318 | 13.6670 | 13.6670 |
| wide128 | N2 | 9 | 8.4660 | 9.6740 | 10.4917 | 10.4917 |
| wide128 | NL | 9 | 9.1789 | 9.5825 | 11.2930 | 11.2930 |
| wide128 | P1 | 9 | 8.4600 | 12.4187 | 38.4810 | 38.4810 |
| wide128 | P2 | 9 | 12.2723 | 13.3039 | 15.2174 | 15.2174 |
| wide128 | PL | 9 | 11.9600 | 13.7848 | 14.6804 | 14.6804 |
| wide128 | SN1 | 9 | 17.7093 | 18.4856 | 19.3826 | 19.3826 |
| wide128 | SN2 | 9 | 17.9077 | 18.4272 | 19.9270 | 19.9270 |
| wide128 | SNL | 9 | 17.8372 | 19.5223 | 20.9190 | 20.9190 |
| wide128 | SP1 | 9 | 17.1268 | 18.4344 | 27.2446 | 27.2446 |
| wide128 | SP2 | 9 | 16.9934 | 19.2220 | 20.5840 | 20.5840 |
| wide128 | SPL | 9 | 17.1782 | 18.0563 | 19.1662 | 19.1662 |
| owned32 | N1 | 9 | 1.9227 | 2.6987 | 4.6049 | 4.6049 |
| owned32 | N2 | 9 | 1.8998 | 2.3924 | 6.4380 | 6.4380 |
| owned32 | NL | 9 | 1.9376 | 2.4233 | 4.1891 | 4.1891 |
| owned32 | P1 | 9 | 2.0079 | 2.1090 | 2.5486 | 2.5486 |
| owned32 | P2 | 9 | 1.9669 | 2.0509 | 2.3239 | 2.3239 |
| owned32 | PL | 9 | 1.9479 | 2.1539 | 4.2394 | 4.2394 |
| owned32 | SN1 | 9 | 2.7705 | 2.8630 | 3.1759 | 3.1759 |
| owned32 | SN2 | 9 | 2.7385 | 2.9932 | 3.1841 | 3.1841 |
| owned32 | SNL | 9 | 2.8313 | 2.9500 | 3.1758 | 3.1758 |
| owned32 | SP1 | 9 | 2.7200 | 2.8860 | 3.3549 | 3.3549 |
| owned32 | SP2 | 9 | 2.7400 | 2.8610 | 3.4258 | 3.4258 |
| owned32 | SPL | 9 | 2.7010 | 3.0041 | 3.4734 | 3.4734 |

## Code and build identities

Public research parallel control commit: 55b299b3716cd1af9e997f30b1a3c1813455d8be. Native derives from that control plus only the original combined patch SHA2562265e704ebb25e0dba1a08e8d4431d1473fce09a740ac5e67ee5fe0b2c4dda4b; no draining-queue patch, R16 cap, sleeping consumer or other prototype. Native is shard scatter+parallel block permutation+serial squeeze+original fixed std-yield consumers.

Original combined freeze identity: bf3b2ea730f98fca2dfaf75f3baba732f7f5d67eaea8bafdbae17c684fe11bae.

driver_sha256: b5982b9ad11501de709ff627f52629f413d28510e78cc84920a068d2b3ee75cb.

parser_sha256: 3b307c13c6c6cf5a275ac54194f57f95f0c3987f09ce2eecad11db7c34f02246.

analyzer_sha256: 4ed739491e9a412b49c9c6f5fdccb541b7d4fc6ad14f077b59d1cbac45ba10c6.

binary_sha256: 69c008b6f10aa7f60761c4f899b542d8c547054232fdf42a3b03fd5765c7afc2.

Original crate Rust-source tree digest identities:

public: 1da10e61b2b97346fbdbf5d0ad03310c66687ee59deb1693ade8bb61aa3eb331.

public_placement: 1da10e61b2b97346fbdbf5d0ad03310c66687ee59deb1693ade8bb61aa3eb331.

combined: 5d648aff62347ecfdded7066ecfbb2f810ebda7048b4a383c1d9ed560fc36c0b.

combined_placement: 5d648aff62347ecfdded7066ecfbb2f810ebda7048b4a383c1d9ed560fc36c0b.

SHA256 of sorted compact JSON mapping every original crate src Rust filename to its SHA256; filenames relative to src, UTF-8, no final newline.

rustc 1.99.1 (58cf8f9c3 2026-10-13); features default,std,rayon,portable-simd; opt-level3,codegen-units16, no custom target-cpu/LTO/root overrides. Sources and executable unchanged before/after execution. Concrete four-shape call mapping and distinct primary/placement code addresses were independently checked. Inlining differs and runtime/drop helpers can be shared; independent complete callgraphs and actual per-call root routing are not asserted.

## Open gates and space qualification

No W1/W2/W4 scaling, allocation/retained-memory pass or low-cardinality/skew/ordered/reverse/mostly-sorted/fallback performance matrix was run in this phase. Full-random inputs do not substitute for tied-payload stability coverage. Broader SIMD-runtime, no_std-abort and MSRV-runtime correctness gates remain incomplete.

Private consumer Workspace bounds apply to logical consumers, with requested consumer count S<=poolP and actual scatter shardsQ<=S. They do not prove a P-independent per-physical-thread bound: a key invoking rayon::join may suspend one consumer and run another on the same physical worker; nested sorts add independent invocation ledgers. No measured aggregate heap, retained heap, or few-times-serial space claim is made here.

The immediate conclusion is mixed evidence, not adoption. The completed first slice should remain preserved while remaining correctness, Owned32-tail interpretation, allocation and future matrix gates are evaluated separately.
