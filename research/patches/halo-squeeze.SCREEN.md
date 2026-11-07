# Bounded parallel halo squeeze: four-case screen

## Finding

Neither the initial pilot nor the corrected exact-entry pilot demonstrates an end-to-end improvement over the strongest combined implementation. The corrected u64 W4 case is slower in all three blocks and its 45.3% paired median slowdown exceeds the C A/A plus placement envelope of 22.9%. Its candidate-repeat envelope is much broader because two long candidate samples remain unexplained. Row16 W4 is also slower in all three corrected blocks. No case supports adoption, and W8 adds measured peak heap. This is a negative screening result for this implementation, not proof that every halo schedule fails.

This is a three-block directional screen, not acceptance or a general scaling claim. All timing samples, including outliers, are retained. The scope is 10,000,000 random SplitMix64 seed-1 u64 keys and repr(C) 16-byte key/index records, at W4 and W8, pinned to CPU0-7.

The exclusive timing window was 2026-11-07T08:39:30Z to 2026-11-07T08:39:51Z. All 84 measured sorts and 28 warmup sorts passed complete stable-reference comparison. Each case ran one verified randomized warmup block followed by three randomized measured blocks.

## End-to-end timing

Milliseconds below are independent medians. C and H are the median over per-block means of their two repeated labels. C is the strongest frozen combined native-root + parallel block-permutation + serial-squeeze/consumer implementation. H is the new group-local parallel halo squeeze plus ordinary contiguous child sorting. Main default and SIMD are commit 72c298494ef382246aa83d08f54193cc3896de07; SIMD enables portable-simd.

| Case | C ms | H ms | Main ms | SIMD ms | H/C paired median | H faster blocks | C A/A + placement envelope | Including H A/A |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| row16 W4 | 58.656 | 67.564 | 146.147 | 135.570 | 1.147 | 0/3 | 1.160 | 1.160 |
| row16 W8 | 53.295 | 52.023 | 152.534 | 140.477 | 1.198 | 1/3 | 1.566 | 3.782 |
| u64 W4 | 56.094 | 81.506 | 121.434 | 116.978 | 1.453 | 0/3 | 1.229 | 3.151 |
| u64 W8 | 27.035 | 26.468 | 99.090 | 91.143 | 1.030 | 1/3 | 1.150 | 1.607 |

The C control envelope is max(max ratio, 1/min ratio) over all C2/C1 and placement/C observations. The last column additionally includes H2/H1 and its inverse. Both are two-sided. Including broad H repeat tails does not explain or excuse a slowdown beyond C controls. Ratios use within-block values, never reconstructed from independent timing medians. Three blocks cannot satisfy the benchmarking guide's default nine-block, 8/9-agreement acceptance rule.

## Controls and paired main comparisons

| Case | C2/C1 range | Placement/C range | H2/H1 range | C/H paired median | Main/H paired median | SIMD/H paired median |
|---|---|---|---|---:|---:|---:|
| row16 W4 | 0.902-1.045 | 0.913-1.160 | 0.946-1.058 | 0.872 | 2.170 | 1.988 |
| row16 W8 | 0.959-0.998 | 1.287-1.566 | 0.942-3.782 | 0.835 | 2.983 | 2.700 |
| u64 W4 | 1.026-1.224 | 0.813-1.064 | 0.471-3.151 | 0.688 | 1.639 | 1.381 |
| u64 W8 | 0.870-1.072 | 0.936-0.987 | 0.976-1.607 | 0.970 | 3.629 | 3.366 |

## Unresolved long samples

All outliers are retained. The corrected pilot includes Row16 W8 H2 block0/slot2 at 174.805 ms (H1 in the same block: 46.216 ms), u64 W4 H1 block0/slot3 at 110.823 ms (H2: 52.190 ms), and u64 W4 H2 block2/slot4 at 160.369 ms (H1: 50.898 ms). u64 W8 H2 block2/slot2 is 42.828 ms versus H1 at 26.656 ms. Every output verified. Some other labels also varied, including Row16 W8 placement at 54.940-84.325 ms.

The exact-entry rerun removes the known H1/H2 closure-monomorphization difference. It does not identify the cause of these tails. Timed runs contain no phase trace or scheduler counters, and perf is unavailable. There is no evidence here assigning them to the algorithm, operating-system scheduling, code placement, caches, or instrumentation. No samples are removed and no slowdown is relabeled as harmless noise. No extra timing run was performed to seek a favorable outcome.

## Preserved initial pilot

The initial pilot ran at 08:36:16-08:36:35 UTC and also verified all 112 measured/warmup sorts. It is preserved independently. C1/C2 correctly shared one match arm. u64 H1/H2 used the public Direct API correctly. Row16 H1/H2 had separate equivalent by-key closure monomorphizations inherited from the prefetch off/on arms, confirmed by binary symbols. Their ratio mixes placement and runtime variation and must not be called exact-entry A/A. The corrected pilot joins H1/H2 into one arm for both types; no algorithm source or allocator binary changed.

| Initial case | C ms | H ms | Paired H/C | C control envelope |
|---|---:|---:|---:|---:|
| row16 W4 | 51.707 | 51.236 | 0.9909 | 1.2549 |
| row16 W8 | 52.497 | 52.712 | 0.9885 | 1.2922 |
| u64 W4 | 38.446 | 42.090 | 1.1538 | 1.3455 |
| u64 W8 | 25.742 | 25.340 | 1.0125 | 1.0699 |

No initial paired median effect cleared its C controls. The corrected pilot has a different binary, so differences between the two windows are not treated as a controlled algorithm effect.

## Actual routes and bounded snapshots

Root and halo diagnostics come from separate verified invocations of the initial binary, not timed or allocator-instrumented runs. Algorithm source is byte-identical in both pilots, and both equivalent initial H entry instantiations reported identical geometry and accepted plans. The following are actual accepted route records; raw candidate-Q attempts and explicit fallback markers are retained in the diagnostic logs.

### u64, W4

- halo root n=10000000 record_bytes=8 radix=8 shift=56 shards=4 B=128
- halo candidate n=10000000 B=128 roots=4 R=256 Q=4 raw_cells=98341 cap=404864 largest_group=2501503 max_group=5000000 balanced=true fits=true
- halo groups=4 cells=98341 capacity=98341 cap=404864 snapshot_bytes=786728 job_bytes=448 source_bytes=32 movement_workers=4

### u64, W8

- halo root n=10000000 record_bytes=8 radix=8 shift=56 shards=8 B=128
- halo candidate n=10000000 B=128 roots=8 R=256 Q=8 raw_cells=457279 cap=809728 largest_group=1251737 max_group=2500000 balanced=true fits=true
- halo groups=8 cells=457279 capacity=457279 cap=809728 snapshot_bytes=3658232 job_bytes=896 source_bytes=64 movement_workers=8

### row16, W4

- halo root n=10000000 record_bytes=16 radix=8 shift=56 shards=4 B=79
- halo candidate n=10000000 B=79 roots=4 R=256 Q=4 raw_cells=60915 cap=404864 largest_group=2501503 max_group=5000000 balanced=true fits=true
- halo groups=4 cells=60915 capacity=60915 cap=404864 snapshot_bytes=974640 job_bytes=448 source_bytes=32 movement_workers=4

### row16, W8

- halo root n=10000000 record_bytes=16 radix=8 shift=56 shards=8 B=64
- halo candidate n=10000000 B=64 roots=8 R=256 Q=8 raw_cells=231615 cap=809728 largest_group=1251737 max_group=2500000 balanced=true fits=true
- halo groups=8 cells=231615 capacity=231615 cap=809728 snapshot_bytes=3705840 job_bytes=896 source_bytes=64 movement_workers=8

The actual root B/shard/digit values match the same-case geometry captured by the prior strongest-combined profile. The root geometry remains the native scatter and block-permutation geometry, not a virtual-bucket route. The halo cap is min(floor(n/8), 32*P*ceil(sqrt(n))) record cells. The bound concerns snapshot capacity, not total algorithm heap. Halo groups complete their own movement before ordinary contiguous child sorting; scheduling and overlap differ from C. Isolated squeeze-stage scaling must not be treated as the end-to-end speedup.

## Separate allocator measurements

These numbers are measured once per case and variant with a process-wide counting allocator in a separate binary after warming every pool worker and the complete actual case. They are not sums of capacities, process RSS, or timing results. Input, reference and mutable clone allocations precede the window. Requested-byte totals count successful allocations and replacement realloc sizes. Peaks count additional live requested bytes. Thread stacks, allocator bookkeeping and reserved-but-unrequested memory are not measured.

| Case | Variant | Peak additional requested bytes | Allocation requests | Total requested bytes | Retained additional bytes |
|---|---|---:|---:|---:|---:|
| u64 W4 | C | 3,989,395 | 508 | 6,490,421 | 0 |
| u64 W4 | halo | 3,989,995 | 255 | 7,391,974 | 0 |
| u64 W4 | main | 1,318,748 | 19 | 1,525,820 | 0 |
| u64 W4 | simd | 1,318,748 | 19 | 1,525,820 | 0 |
| u64 W8 | C | 6,168,395 | 635 | 9,685,281 | 0 |
| u64 W8 | halo | 9,191,622 | 371 | 13,215,098 | 0 |
| u64 W8 | main | 1,318,748 | 19 | 1,525,820 | 0 |
| u64 W8 | simd | 1,318,748 | 19 | 1,525,820 | 0 |
| row16 W4 | C | 5,598,674 | 561 | 9,210,448 | 0 |
| row16 W4 | halo | 5,599,274 | 305 | 10,256,723 | 0 |
| row16 W4 | main | 1,849,780 | 22 | 2,087,892 | 0 |
| row16 W4 | simd | 1,849,780 | 22 | 2,087,892 | 0 |
| row16 W8 | C | 7,971,658 | 665 | 13,264,588 | 0 |
| row16 W8 | halo | 10,351,952 | 408 | 16,997,961 | 0 |
| row16 W8 | main | 1,849,780 | 22 | 2,087,892 | 0 |
| row16 W8 | simd | 1,849,780 | 22 | 2,087,892 | 0 |

## Negative cases and limitations

- At n=131072, two shards, root width 12 / shift 52 / R=4096, the owner's legacy public-probe diagnostic found Q2 raw snapshot cells 65,233 above cap 16,384. The candidate therefore uses serial squeeze and one local child visitor. This is not the old consumer pipeline. Two old tests requiring non-origin consumer-key panic witnesses cannot require that witness on this fallback. Their failure was the missing expected witness; those concentrated cases do not reach the subsequent record-validation assertion. This limitation is not hidden by the four random large cases.
- Before candidate copying/building, the owner verified 51 unit + 21 integration default debug tests, 10 applicable opt-in debug public tests, 8 strict-provenance Stacked-Borrows Miri tests under serial/scoped schedules, 6 doctests, Clippy with -D warnings, no-default and Rust 1.85 all-feature checks, and warning-denied rustdoc. The whole opt-in public suite is not claimed passing. Actual Rayon-under-Miri is not covered. The Rust 1.85 run was a compile check, not tests. An additional optimized release gate passed all 10 halo movement/panic tests before this pilot. The release integration executables built but ran zero matching tests under the halo_ name filter; they are not claimed as passing full release integration coverage. Exact results are retained in release-gate.log.
- The screen does not test all core distributions, key types, sizes, seeds, worker counts, owned records or non-Sync records. It does not measure fixed-decomposition scaling, hardware memory traffic, cache/TLB counters or peak stack bytes.
- u64 uses the public Direct par_stable_sort/stable_sort APIs. Row16 uses by-key APIs. All variants receive identical actual types and layout without reinterpretation. Every clone and timer executes on an actual Rayon pool worker, with cloning and full-reference checking outside the timer.
- TESSERA_HALO_SQUEEZE=1 is set before process startup; unrelated overrides are cleared. The original C source has no halo flag. TESSERA_HALO_STATS is absent during timing and allocator measurement.
- Harness self-tests deliberately corrupt and mutate outputs for every label, warmup and measured block, verify rejection, and verify fresh-input restoration on subsequent sorts. Raw data aggregation checks count, labels, slots, verification, units, input fingerprints, frozen sources and measured binary identity.

## Integrity and reproduction

The original source, drivers and evidence are unchanged. Raw data, harnesses and profiles stay outside the repository. Only this English report and its hashes are commit-safe. No shared-repository edits, commits or pushes were made.

- Owner frozen-source archive SHA-256: `195ad328a4da3c10e25b1f19b2a18b267e9e6495d038c61f6406bf1ceeedfd2c`
- Owner frozen-source manifest SHA-256: `c66e83aeee558b31312895b51ba284f3a4f1a7df370b9147cef3b44cf21a287e`
- Initial timing binary SHA-256: `fc9b40ecd7939d51fa299d4f6a51dd6f3e7b82d6090f6358827ae4a1d0a7273a`
- Initial run manifest SHA-256: `5ed3dc262cc7f29e64562e23a10f745c5c6cf6f8289eb25b86191fe060685059`
- Initial raw evidence manifest SHA-256: `4a914045e0b09dc636f934c15563ddc00a01e4f04383311f4293dd7987a84a62`
- Optimized release gate log SHA-256: `6ba35cb015de805c86f3cbe2cbe60fc5bba92a80da23a6341daa99acaa0f2d4f`
- Timing binary SHA-256: `c6b546dc4a49d0a4946c75800a3284c5037a912b5097884ee5c3da307a07e65f`
- Counting-allocator binary SHA-256: `6b0f06fe83cbbd3e192ce0f6b23e0048dcf9b8523840af5a07136e8b6dceb7e7`
- Compiled-source manifest SHA-256: `9b8f82390322823fdfd3e5d225c2a250dfa94e69af732892e30bcd227e93d146`
- Run manifest SHA-256: `d63486669d13aceef842cac8b1e2cf734282c36015461a1ae31b91d019f7a33e`
- Timing driver SHA-256: `f4f34982b9a1639e7ae536a47c626bf315d8ffc682ba632c0c1ec4893296026a`
- Allocator driver SHA-256: `2dd70d3edb18b9a48dc01ac8fc81ae6010bc7c4918ead639247ca77ff63b0205`
- Aggregate SHA-256: `1e74eb7148ba1114edde395f7502932d64b581b5aa24aa410efd9284e28b3bdd`
- Heap data SHA-256: `33e5c278eb6a62a6b4604617084cfd737879fd2e686d53c40fe3cfe71ba43a79`
- Raw evidence hash manifest SHA-256: `5b30cdadbf36ee96e9d557fbecb8f59e1e809f1d4e0de268fd3a84397cdbe244`

Exact commands and protocol are retained in build.sh, locked-run.sh, run.py, run-heap.sh, run-diagnostics.sh, selftest.sh, aggregate.py and PROTOCOL.md beside this report. The measured build is the default release profile without target-cpu=native, branch alignment or an LTO override.
