# Parallel Direct count-window: initial end-to-end screen

## Result

Three of four predeclared cases show a favorable full-API scheduling signal beyond this screen’s A/A and placement controls, with 9/9 paired blocks favoring P over both O and S. u32/d256 is inconclusive under the full controls because of a large retained P2 tail. This is not production acceptance and is not evidence about full-random inputs, generic records, the standard full-width skew99 distribution, or the general parallel root algorithm. No additional timing or tuning followed this screen.

P/O compares the complete new algorithm with the original serial count route behind the parallel public API. P/S compares the full public API with identical candidate geometry and intrinsic kernel, changing only row/fill scheduling; common serial preparation, allocation, prefix and reconstruction are still included. P/O must not be attributed entirely to threading.

## Absolute elapsed times and paired ratios

Times are milliseconds per full public sort: median across nine blocks of the mean of each duplicate pair. strongserial has one contextual sample per block. Ratios are medians of within-block ratios, not ratios of the displayed medians. All allocations made by the public sort are timed.

| Case, n=10,000,000 | Original O ms | Parallel P ms | Same-kernel schedule S ms | Original strongserial ms | P/O | P/S | Screen result |
|---|---:|---:|---:|---:|---:|---:|---|
| u32 / d256 | 11.713561 | 6.930555 | 10.452773 | 11.549985 | 0.580658 | 0.667531 | Inconclusive: controls |
| u32 / d256_hot99 | 16.302205 | 6.964111 | 15.316222 | 16.985864 | 0.428950 | 0.450751 | Favorable screen signal |
| u64 / d256 | 16.945769 | 10.115534 | 15.757001 | 16.471352 | 0.602302 | 0.671487 | Favorable screen signal |
| u64 / d256_hot99 | 20.733495 | 10.853639 | 18.305285 | 21.887045 | 0.532798 | 0.610438 | Favorable screen signal |

| Case | P/O block range | P/S block range | S/O median | P/O and P/S favorable blocks | Placement P/O median | Placement P/S median |
|---|---:|---:|---:|---:|---:|---:|
| u32 / d256 | 0.449391–0.845558 | 0.508156–0.997878 | 0.886434 | 9/9 and 9/9 | 0.584452 | 0.590679 |
| u32 / d256_hot99 | 0.362586–0.598635 | 0.402919–0.642343 | 0.909930 | 9/9 and 9/9 | 0.371347 | 0.408650 |
| u64 / d256 | 0.546113–0.745089 | 0.587314–0.797850 | 0.914488 | 9/9 and 9/9 | 0.544367 | 0.640975 |
| u64 / d256_hot99 | 0.434977–0.579444 | 0.480645–0.659328 | 0.879613 | 9/9 and 9/9 | 0.475318 | 0.537353 |

Each independent P/O and P/S placement comparison also favored P in 9/9 blocks. S/O alone does not clear its applicable full control envelope in any case. Contextual P/strongserial medians are u32 / d256 0.619813, u32 / d256_hot99 0.434692, u64 / d256 0.610496, u64 / d256_hot99 0.461770. strongserial lacks a dedicated A/A and placement pair, so it is context rather than a separate acceptance test.

## Complete A/A and placement controls

A/A is replica2/replica1. Placement is the independent source-identical package copy divided by the mean of the original two replicas in the same block. Entries show median [minimum, maximum] across all nine blocks; no outlier is dropped.

| Case | O A/A | P A/A | S A/A | O placement | P placement | S placement |
|---|---:|---:|---:|---:|---:|---:|
| u32 / d256 | 0.962722 [0.798006, 1.057514] | 1.130080 [0.887922, 2.397396] | 0.970430 [0.679496, 1.233367] | 0.995423 [0.789476, 1.052912] | 0.956652 [0.502841, 1.114204] | 1.023050 [0.936671, 1.089797] |
| u32 / d256_hot99 | 1.008743 [0.956993, 1.058205] | 0.900712 [0.670351, 1.260703] | 0.984551 [0.920883, 1.066735] | 0.977451 [0.911757, 1.125892] | 0.870238 [0.775385, 1.462132] | 0.984122 [0.930413, 1.046694] |
| u64 / d256 | 1.033127 [0.822337, 1.178098] | 1.066526 [0.789625, 1.263292] | 0.953341 [0.887664, 1.072582] | 0.988307 [0.896873, 1.264725] | 0.926754 [0.821207, 1.147810] | 0.954922 [0.919290, 1.022194] |
| u64 / d256_hot99 | 0.980551 [0.922113, 1.150138] | 1.015548 [0.869756, 1.111325] | 0.996181 [0.964335, 1.060508] | 0.978427 [0.952232, 1.081048] | 1.028893 [0.751067, 1.100502] | 1.009347 [0.955708, 1.160900] |

The screen uses the union of relevant A/A and placement ranges, symmetrized by reciprocals, and requires a median outside that envelope plus at least 8/9 favorable blocks. Lower control-envelope bounds for P/O and P/S are u32 / d256 0.417119, u32 / d256_hot99 0.670351, u64 / d256 0.789625, u64 / d256_hot99 0.751067.

u32/d256 block 8, P2, slot 7 contains individual durations [4.881840, 7.378774, 37.435552, 7.266325] ms. Its sample mean is 14.240623 ms versus P1 5.940037 ms in that block, producing A/A 2.397396. The cause is unproven: the evidence does not establish whether this was scheduling, host interference, allocation, or another source. The tail is retained and blocks acceptance of this case; it is not labeled harmless noise and was not rerun away.

## Memory and route evidence

All four cases and all four measured memory labels O/P/S/strongserial have observed peak extra requested heap 65,536 bytes and retained delta 0. Original O and strongserial make one allocation totaling 65,536 bytes; P and S make three totaling 83,976 bytes. The two new vectors request exactly 18,440 bytes: 8×256 usize histograms (16,384 bytes) plus 257 usize endpoints (2,056 bytes). These extra requests did not increase the observed full-sort peak. The allocation run did not trace allocation phases, so the equal peaks are not claimed as a measured lifecycle attribution. Source inspection shows a common temporary preparation allocation; this is explanatory source evidence only.

Input, stable reference, fresh clone, and established Rayon pool allocation are excluded from allocator counters. Pool/runtime state was primed before counting. Full output equality and retained 0 were asserted for every measured memory sort. Stack usage is unmeasured. The source budgets a conservative 4,096 bytes per leaf for algorithm task/frame state, not measured heap or a guarantee about Rayon runtime stacks. At this geometry the declared vectors plus allowance total 51,208 bytes; the source’s checked aggregate budget is 8×P×ceil(sqrt(n))×sizeof(record), with per-worker row state bounded independently of P. There is no n-record scratch.

Separate cfg(test)-only diagnostic copies invoked the actual public APIs on all four exact n=10M vectors. Original parallel API and original stable_sort each entered Direct::try_count_window once, at low 0, width 8. Candidate P and S both entered the new count route, with P=8, R=256, eight count chunks and eight fill chunks of 1,250,000 records each. S ran both passes on its calling worker; P witnesses involved 7 or 8 distinct pool workers. Timing packages contain no diagnostic hooks, trace atomics or counting allocator.

Observed diagnostic histogram base alignment was 16 mod 64. Adjacent-row cacheline sharing is possible and remains unmeasured; no alignment/layout parameter was tuned. Diagnostic-only added helper visibility warnings were retained in build logs; both diagnostic tests passed. Source-owner safety checks are separate from these diagnostics.

## Input and measurement contract

- Four predeclared cases only: Direct u32/u64, n=10M, d256 or explicitly named d256_hot99. Full varying-bit mask 255 is asserted.
- SplitMix64 seed 1. d256 draws next() & 255. d256_hot99 makes exactly 1% values from 1+next()%255 and 99% zero, then Fisher-Yates shuffles with the continued generator. Both distributions swap existing 255, 0, 1 into positions 0, 1, 2 without changing the multiset, ensuring a non-flat full-width initial probe. This is not the repository’s standard skew99 generator.
- Nine randomized blocks; ten labels O1/O2/Oplacement, P1/P2/Pplacement, S1/S2/Splacement, strongserial. Duplicate labels use exactly the same function pointer. Each placement is an independently named package with identical Rust source. Cargo feature isolation was verified.
- P and S use the same feature-enabled candidate package and literal public function pointer. S enters the panic-safe research TLS scope on the actual W8 worker around the entire sample loop, outside every timer. The public one-worker fallback is not S and is not measured here.
- Each sample accumulates at least 50 ms and at least two fresh sorts. Cloning occurs on the actual pool worker before each timer. Full stable-reference equality is checked after every timer. The real timer validator caught an intentionally wrong mutating variant, fresh-input checks passed, and raw JSON duration/count/unit round-trips passed.
- There are 360 verified samples containing 1,681 individual timed sorts. Every individual duration and every validation output is retained privately. Warmups are excluded from statistics.
- Process affinity CPUs 0–7, W8, no individual worker pins. Detected and actual dispatch-selected Fearless levels are both AVX512. portable-simd is enabled. CPU: AMD EPYC 9V74 80-Core Processor. Rust 1.99.1, LLVM 23.1.1, default release with debug information; no target-cpu/LTO/alignment flags. All TESSERA_* and Rust flag overrides were logged then cleared.
- Every build, diagnostic and allocator run held shared FD9; timing held exclusive FD9 on the preexisting device 27 / inode 1578019 lock. Final validation acquired 14:00:03Z and released 14:00:10Z. Timing acquired 14:00:10Z and released 14:01:25Z on 2026-11-07. Analysis began after RELEASE.

| Case | Bytewise little-endian FNV-1a input fingerprint | Individual duration range ms |
|---|---|---:|
| u32 / d256 | 2e2841855d838d04 | 4.287013–37.435552 |
| u32 / d256_hot99 | f6fe43a8cddc8ae4 | 4.674297–27.949220 |
| u64 / d256 | fa9d13862cbe1a44 | 8.070219–29.181677 |
| u64 / d256_hot99 | 558a5e14eed7a934 | 7.936357–30.342478 |

## Source and validation identity

- Shared runtime change: e8e590616f0d3cfb5e8a30b84e4b3e43042036a4, frozen equally in original and candidate. Later main changes are deliberately excluded.
- Original source identity: 955fa26515aa9feeac20b41d2885dd62dd4349d4d7040bfe9b3025e7306e6bfc.
- Candidate source identity: add21ec01b2982c32dbc430e98444a866743cd0de03801f7d9968213e1b82d71.
- Identity algorithm: SHA256 of UTF-8 canonical JSON of the complete file-hash map, sorted keys with comma/colon separators. Both full upstream maps and all four timing-source placements were rechecked after the owner’s safety PASS and before running sorts.
- Owner safety runner exit 0, RELEASE 13:59:24Z: baseline 61 unit+23 integration and candidate 71 unit+23 integration all-feature release tests; baseline 56 / candidate 66 default-backend unit tests; both Clippy -D warnings and no_std 13 tests. One existing 10M root diagnostic was ignored by those broad suites, separately disclosed. Exact-input count-route diagnostics described above are not ignored.

Timing and allocation binary SHA256:

- 3ec436f9c419cfab70f9e10b65c80a32cf0bf2bd6c5a5a3b8ef610ad013e583f  target/release/tessera-parallel-count-window-screen
- a09bf1cbcf1df5400847224af06cb0b3ad451149a80ca7276c56ecc3bd85ac99  target/release/full-memory

## Limits and next step

This initial feature-enabled research screen supports continuing the narrow Direct unsigned candidate, particularly the hot-zero and u64 cases. It does not establish production-feature-off code generation, cross-run repeatability, other lengths or worker counts, shifts/widths 1–7, other distributions, generic-route nonregression, or a universal speedup. Both source placements share one build and one host session. A subsequent production-feature confirmation and broader low-cardinality/nonregression matrix remain necessary before acceptance; none was pre-run or silently tuned here.

Raw observations, private driver, source copies, compiler/dependency/host capture, frozen source maps and diagnostic diffs remain in the private screen evidence. The source-only publication packet contains this report and identities/checksums, not the raw observations or harness.
