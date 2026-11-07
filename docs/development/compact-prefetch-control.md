# Combined four-block source-prefetch control: negative screen

## Decision

**Do not promote this prefetch control.** The bounded screen did not establish a useful end-to-end gain. None of its paired feature-on/off medians exceeded the conservative two-sided A/A and placement noise envelope in the favorable direction. Only one or two of three measured blocks favored prefetch in each case. At u64 W8, prefetch was slower in every block against the original combined C baseline; both off and on also show placement differences from C. No distance tuning or broader noise sweep followed.

All **84 measured sorts and 28 warmup sorts** passed full stable-reference comparison. The exclusive FD9 timing window was **2026-11-07 08:06:11–08:06:29 UTC**, on CPU affinity **0–7**. The original phase-study binary, source snapshot and raw data were not changed.

## Mechanism and scope

The source is the frozen `tessera-prefetch-control` prototype. Its optional `prefetch-compact` feature looks ahead four block-copy positions in the existing parallel cycle permutation and prefetches every 64-byte cache line of that valid source block on x86-64. It does not alter the record-copy order, copy count, partition decomposition, child algorithm or workspace. A shared inherited closure lint fix (`for_each(&run)` to `for_each(run)`) is present identically in both feature-on and feature-off copies.
Feature-on/off are separately named Cargo packages whose Rust source files are byte-identical. Only the selected features differ: `rayon` versus `prefetch-compact` (which includes `rayon`). The intrinsic is disabled on other architectures and under Miri. The supplied source evidence reports 41 release library tests, warnings-denied Clippy, and two strict-provenance default Stacked Borrows Miri moving-kernel tests passing. Those Miri tests do **not** execute the prefetch intrinsic. This screen adds actual end-to-end timings and full-output validation, not new hardware-intrinsic safety claims.

## Fairness and method

- Four cases only: 10,000,000 SplitMix64 seed-1 random u64 keys or repr(C) row16 records (u64 key plus original u64 index), with four or eight Rayon workers.
- Seven same-binary labels: original C1/C2, an identical-source placement control, feature off, feature on, current-main default serial, and current-main portable-SIMD serial. Main is commit `72c298494ef382246aa83d08f54193cc3896de07`.
- One full randomized warmup block followed by three measured randomized blocks per case. Every warmup and measured output is compared in full with Rust stable-sort references. Row payloads verify permutation and stable equal-key order.
- Default release flags, no target-cpu native/LTO overrides. Shared FD9 lock covers the offline -j1 build; exclusive FD9 covers the whole timing driver, including preparation and references.
- API scratch allocation is inside sort timers. Input generation, cloning, reference construction and verification are outside. Each timed sample is one complete sort.
- This is a directional three-block screen, not nine-block acceptance. No phase timings or hardware counters were collected for this control. End-to-end results cannot attribute a change specifically to cache misses, TLB behavior, DRAM bandwidth or the permutation stage.

## Same-binary absolute medians

Milliseconds. C is the median of the per-block mean(C1,C2). Independent median durations must not be used to reconstruct paired ratios.

| Case | C | Feature off | Feature on | Main default | Main SIMD |
|---|---:|---:|---:|---:|---:|
| row16 W4 | 61.338 | 65.546 | 61.253 | 167.023 | 151.683 |
| row16 W8 | 57.370 | 51.202 | 53.116 | 157.717 | 137.639 |
| u64 W4 | 37.001 | 38.513 | 35.576 | 90.552 | 81.933 |
| u64 W8 | 34.421 | 39.260 | 40.668 | 103.313 | 97.655 |

## Paired ratios and controls

The conservative two-sided control factor is max(max_ratio, 1/min_ratio) over C2/C1 and placement/C. The favorable bound for on/off is its reciprocal. Even clearing that screen bound would not constitute nine-block acceptance.

| Case | On/off median | On/off range | On faster than off | On/C median | Off/C median | A2/A1 range | Placement/C range | Control factor |
|---|---:|---|---|---:|---:|---|---|---:|
| row16 W4 | 0.927 | 0.856–1.309 | 2/3 | 1.038 | 0.944 | 0.931–1.269 | 0.908–1.215 | 1.269 |
| row16 W8 | 1.004 | 0.508–1.045 | 1/3 | 0.926 | 0.989 | 1.055–1.177 | 0.897–1.066 | 1.177 |
| u64 W4 | 0.924 | 0.693–1.153 | 2/3 | 0.961 | 1.041 | 0.861–1.008 | 0.912–1.010 | 1.161 |
| u64 W8 | 1.139 | 0.978–1.744 | 1/3 | 1.226 | 1.141 | 0.861–1.060 | 0.951–1.186 | 1.186 |

## Adverse samples and limits

The row16 W8 feature-off sample of **104.661 ms** and u64 W8 feature-on sample of **79.320 ms** are retained in the raw data and ratios. Their causes are unresolved; neither is silently excluded or assumed to be noise. At u64 W8 the on/C paired median is **1.226**, with all three on calls slower than C. That adverse end-to-end direction is sufficient not to promote this fixed-lookahead mechanism, without pretending this small screen proves a universal slowdown.
No additional heap allocation is introduced by the source prefetch helper, but allocator peaks were not remeasured in this screen. No full-O(n) record workspace was added. Benchmark inputs/references are separate from algorithm scratch. The source always executes the same logical copy operations; no actual DRAM traffic was measured. The source-prefetch experiment is independent of the preceding full phase study, so absolute times across their different builds/windows are not an A/B comparison.

## Integrity and external evidence

Only this report and hashes are intended for the repository. Raw timing files, frozen source copies, binaries and the research harness remain outside it.

- Prefetch compact source SHA-256: `f92e23158f6410bc1f4bad0576d2aa119886047e05033964ec5247aac12de426`.
- Measured binary SHA-256: `25b0694ace2b7985b939047235b3c3b91114a3918ceb2cdd7f5b985d7f67cd8e`.
- Compiled-source hash manifest SHA-256: `339fed7b593e266e30c6a30440d7765eced956d58b97a05d05cc85dc41ea1d64`.
- Raw timing hash manifest SHA-256: `3b758973b6aa66734a81e302fd266957ae9765c7d6fc15f721182bfa05156be2`.
- Run manifest SHA-256: `c09480f64ae0426668bb0a7dd0fd20a523618f95301c71a1963f54b29305f2b1`.
- Aggregate SHA-256: `b815b735e60a1053a4edc339822ea874a57ef4a753980e56ed8d8f8b185a60bc`.
- Harness source SHA-256: `00ab89a9fbaa1cb92d6c340e1d22bc60a21067759a180de436fa15de0f3b74f5`.

External artifacts: evidence/manifest.json, evidence/*-n*-w*.jsonl, evidence/aggregate.json, evidence/aggregate.txt, evidence/SOURCE_SHA256SUMS, evidence/RAW_SHA256SUMS, evidence/build.log, evidence/window.log, sources/off, sources/on, harness/src/main.rs, run.py and aggregate.py.
