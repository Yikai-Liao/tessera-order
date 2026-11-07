# e1 Direct count-window: two bounded Tier 2 shards

## Decision

The normal research-feature-OFF candidate C clears the full A/A and independent-placement controls against the unchanged combined baseline A in **3/4 narrow-window cases** at n=1,000,000 and W8. u32/d16 remains inconclusive because C placement is broad. Both primary and placement comparisons favor C in all nine u32/d16 blocks, but that does not override the control envelope.

The four full-width fallback cases establish **neither a gain nor a regression**. Broad controls and inconsistent paired directions limit sensitivity; this is not equivalence or proof that regressions are absent. The candidate count route was confirmed absent in all four. The narrow-window result is not a fullrandom or generic-sort speedup.

All 504 samples and 20,385 individual timed sorts passed complete stable-reference equality. All 24 separate allocation observations retained zero extra heap. No timing sample, individual tail, placement or input was excluded. No result was rerun. No runtime, threshold, padding or alignment was changed.

This is a separate bounded checkpoint. The earlier e1 Tier 1 driver sources, manifest/lock, three binaries, report and publication packet remain byte-for-byte unchanged. Its historical report describes only its own completed scope; these Tier 2 results are not pooled into it.

## Labels and protocol

A1/A2 call the same original public parallel-API function; A_copy is an independently placed source-identical Cargo package. C1/C2 and C_copy do the same for the production-feature-OFF candidate. strongserial is the original serial public API and has one contextual label. All seven labels include the complete public call and its internal scratch allocation. C is a distinct package without count-window-research; the common manifest also has separate research packages for the historical Tier 1 binaries, but no research function is invoked by these new bins.

Each of the nine blocks uses a fresh process containing exactly one W8 Rayon pool. Seven labels are randomly ordered within the block and primed outside measurement. Every sample accumulates at least 50 ms and two sorts, cloning fresh input on the actual pool worker before each individual timer and checking full equality afterward. Inputs and the stable reference are prepared outside timers. Process affinity is fixed to CPUs 0–7, with no individual worker pinning, sleep or spin tuning. This screen has no across-worker-count comparison or same-kernel serial scheduling label.

The main statistic is median(C duplicate mean / A duplicate mean) over matched blocks, not the ratio of displayed median times. A claim requires the median outside the symmetric envelope of both algorithms’ complete A/A and placement ranges, at least 8/9 primary blocks agreeing, and at least 8/9 independent-placement comparisons agreeing. Placement ratios are copy / duplicate mean. No tail trimming or confidence interval substitution.

## Absolute time and paired attribution

Each A/C time is the median of nine within-block duplicate means. strongserial is the median of nine single-label sample means. At exactly one million elements, the displayed ms/sort is numerically equal to ns/element.

| Input | A ms | C OFF ms | strongserial ms | C/A median [range] | Primary favorable /9 | Placement favorable /9 | Conclusion |
|---|---:|---:|---:|---|---:|---:|---|
| u32/d16 | 0.914979 | 0.711267 | 0.909772 | 0.781033 [0.609975–0.921633] | 9 | 9 | Inconclusive |
| u32/window_hot50 | 1.255035 | 0.648559 | 1.244086 | 0.518139 [0.499361–0.720800] | 9 | 9 | Favorable signal |
| u64/d16 | 1.104731 | 0.704455 | 1.078421 | 0.648881 [0.550784–0.846541] | 9 | 8 | Favorable signal |
| u64/window_hot50 | 1.504893 | 0.784773 | 1.434578 | 0.551189 [0.487878–0.578915] | 9 | 9 | Favorable signal |
| u32/fullrandom | 1.796847 | 1.684145 | 4.037836 | 0.944908 [0.897165–1.126310] | 7 | 4 | Inconclusive |
| u32/standard_wide_skew99 | 1.623351 | 1.623649 | 1.608958 | 1.011841 [0.959168–1.109710] | 4 | 5 | Inconclusive |
| u64/fullrandom | 2.153739 | 2.196318 | 5.380550 | 0.990341 [0.597706–1.075684] | 5 | 4 | Inconclusive |
| u64/standard_wide_skew99 | 1.829989 | 1.805260 | 1.802123 | 0.956875 [0.799868–1.068937] | 6 | 4 | Inconclusive |

The three supported narrow signals correspond to approximately 48.2%, 35.1% and 44.9% less full-API elapsed time for u32/window_hot50, u64/d16 and u64/window_hot50. They are not pure thread-scheduling speedups: the new kernel also changes count/fill implementation, and serial preparation remains in the full API. The earlier e8 same-kernel scheduling control is separate evidence, not a label measured here. No U/public-experiment comparison was run in this shard.

## Complete controls

| Input | A2/A1 range | C2/C1 range | A placement range | C placement range | Symmetric decision envelope |
|---|---|---|---|---|---|
| u32/d16 | 0.921690–1.125429 | 0.893160–1.026887 | 0.978329–1.062273 | 0.659472–1.183377 | 0.659472–1.516364 |
| u32/window_hot50 | 0.967306–1.032039 | 0.798274–1.510262 | 0.967954–1.220900 | 0.939798–1.179350 | 0.662137–1.510262 |
| u64/d16 | 0.975135–1.057831 | 0.916082–1.211330 | 0.911436–1.038870 | 0.840507–1.354623 | 0.738213–1.354623 |
| u64/window_hot50 | 0.992081–1.109017 | 0.864613–1.320810 | 0.956420–1.058101 | 0.856479–1.253135 | 0.757111–1.320810 |
| u32/fullrandom | 0.823401–1.261225 | 0.924467–1.652024 | 0.818804–1.311300 | 0.935257–1.067661 | 0.605318–1.652024 |
| u32/standard_wide_skew99 | 0.884480–1.280256 | 0.914496–1.048880 | 0.953455–1.054116 | 0.957706–1.150150 | 0.781094–1.280256 |
| u64/fullrandom | 0.652278–0.978283 | 0.863774–1.231248 | 0.569956–1.121340 | 0.902685–1.431197 | 0.569956–1.754520 |
| u64/standard_wide_skew99 | 0.842877–1.263886 | 0.929756–1.149362 | 0.808218–1.074933 | 0.947532–1.211862 | 0.791211–1.263886 |

The u32/d16 C placement minimum 0.659472 is retained despite all nine primary directions being favorable. Fullrandom has particularly wide controls: u32 C A/A reaches 1.652024 and u64 A placement reaches 0.569956. These weaken the screen’s ability to detect smaller changes.

## Exact full-sort heap observations

Separate counting-allocator runs include public-call scratch allocation and exclude input/reference construction and the fresh work-buffer clone. Each row is one warmed complete sort, not an average or a lifetime trace. Units are bytes; requested totals count every successful allocation/reallocation request. All rows were reference-checked.

| Input | Label | Peak extra bytes | Total requested bytes | Allocation calls | Retained delta |
|---|---|---:|---:|---:|---:|
| u32/d16 | A1 | 65536 | 65536 | 1 | 0 |
| u32/d16 | C1 | 65536 | 66696 | 3 | 0 |
| u32/d16 | strongserial | 65536 | 65536 | 1 | 0 |
| u32/window_hot50 | A1 | 65536 | 65536 | 1 | 0 |
| u32/window_hot50 | C1 | 65536 | 83976 | 3 | 0 |
| u32/window_hot50 | strongserial | 65536 | 65536 | 1 | 0 |
| u64/d16 | A1 | 65536 | 65536 | 1 | 0 |
| u64/d16 | C1 | 65536 | 66696 | 3 | 0 |
| u64/d16 | strongserial | 65536 | 65536 | 1 | 0 |
| u64/window_hot50 | A1 | 65536 | 65536 | 1 | 0 |
| u64/window_hot50 | C1 | 65536 | 83976 | 3 | 0 |
| u64/window_hot50 | strongserial | 65536 | 65536 | 1 | 0 |
| u32/fullrandom | A1 | 2401160 | 3416170 | 316 | 0 |
| u32/fullrandom | C1 | 2400224 | 3226270 | 310 | 0 |
| u32/fullrandom | strongserial | 471710 | 631966 | 14 | 0 |
| u32/standard_wide_skew99 | A1 | 116400 | 181936 | 11 | 0 |
| u32/standard_wide_skew99 | C1 | 116400 | 181936 | 11 | 0 |
| u32/standard_wide_skew99 | strongserial | 116400 | 181936 | 11 | 0 |
| u64/fullrandom | A1 | 4553400 | 5953932 | 347 | 0 |
| u64/fullrandom | C1 | 4553400 | 5795812 | 342 | 0 |
| u64/fullrandom | strongserial | 919392 | 1142704 | 16 | 0 |
| u64/standard_wide_skew99 | A1 | 125758 | 191454 | 15 | 0 |
| u64/standard_wide_skew99 | C1 | 125758 | 191454 | 15 | 0 |
| u64/standard_wide_skew99 | strongserial | 125758 | 191454 | 15 | 0 |

For the admitted narrow inputs, the new allocations add 1,160 requested bytes at R16 or 18,440 at R256 and do not increase the observed 65,536-byte full-call peak. No phase trace was collected, so this does not establish the overlapping allocation lifetimes. There is no n-record scratch array in the new kernel. The unchanged implementation gates histogram/prefix storage by the per-worker square-root budget.

The multi-megabyte fullrandom peaks belong to the existing parallel fallback, where the new count route did not execute. A/C allocation-call and requested-byte differences in those single observations are not evidence that the new count kernel saves memory; scheduling and allocation paths were not phase-attributed. Fixed stack use, allocator metadata, RSS and peak per-thread heap were not measured here.

## Inputs and route proof

All inputs have n=1,000,000 and use SplitMix64 seed 1. FNV-1a hashes below consume each element’s native-width little-endian bytes. Every algorithm, diagnostic, allocation run and timed block matched the same exact input fingerprint.

- d16: next() & 15, followed by multiset-preserving swaps placing [15,0,1] first. It has full varying mask 15 and width 4.
- window_hot50: exactly 500,000 zeros and 500,000 draws 1 + next()%255; Fisher–Yates shuffle with the continued stream, then multiset-preserving swaps placing [255,0,1] first. Width 8. This is a narrow diagnostic, not the standard full-width skew50 distribution.
- fullrandom: next(), truncated to low 32 bits for u32, without prefix swaps.
- standard_wide_skew99: with M=type maximum, use zero if next()%100<99, otherwise (1+next()%9)*floor(M/10) from a second draw. No remapping or prefix swaps. This is the standard ten-value whole-key distribution, distinct from the historical narrow d256_hot99.

| Input | Input FNV-1a | Full varying width | C count route |
|---|---|---:|---|
| u32/d16 | f21a0b4622c82048 | 4 | Entered |
| u32/window_hot50 | 0e65bd47f5bf6afd | 8 | Entered |
| u64/d16 | 9f0c094b4cf01968 | 4 | Entered |
| u64/window_hot50 | b162e94d457456cd | 8 | Entered |
| u32/fullrandom | 006a2471325121ed | 32 | Declined |
| u32/standard_wide_skew99 | da314e6d73bbb5a9 | 32 | Declined |
| u64/fullrandom | 41ce490591624983 | 64 | Declined |
| u64/standard_wide_skew99 | 1e3aad039af561b5 | 64 | Declined |

Diagnostic-only copies prove exact public-call behavior outside timing. The narrow A calls enter the original serial count/fill route. C uses eight histogram rows, R16 or R256 buckets, eight 125,000-record count tasks and eight 125,000-record fill tasks. Tasks are not necessarily mapped one-to-one onto distinct workers. Diagnostic histogram base mod64 is 16 at R16 and 0 at R256; that observation is not a guarantee about timed allocations or a cache-line-sharing performance measurement. No layout was tuned. Wide C calls have no count layout or count/fill work. All diagnostics checked full output equality.

Both A and C focused diagnostics pass for each shard. Each of eight all-label smoke runs catches a deliberately wrong mutating variant and then verifies all seven real labels. Every sample survives JSON serialization checks with exact elapsed sums, sort counts and units. The timing binary has no counting allocator or route hooks; allocation and cfg(test)-only diagnostics are separate binaries.

## Tails and timing windows

| Input | Samples | Individual timed sorts | Individual sort ms range, all labels |
|---|---:|---:|---|
| u32/d16 | 63 | 3971 | 0.354506–4.849008 |
| u32/window_hot50 | 63 | 3376 | 0.412412–7.605624 |
| u64/d16 | 63 | 3490 | 0.364581–5.516591 |
| u64/window_hot50 | 63 | 2925 | 0.490842–6.016421 |
| u32/fullrandom | 63 | 1707 | 1.289459–8.140044 |
| u32/standard_wide_skew99 | 63 | 1959 | 1.459146–8.885784 |
| u64/fullrandom | 63 | 1229 | 1.546199–26.032377 |
| u64/standard_wide_skew99 | 63 | 1728 | 1.602723–5.565759 |

All per-label sample means, duplicate/placement vectors and every individual duration remain in the private evidence package. The 26.032377 ms u64/fullrandom tail and all other extremes remain included.

- Initial selective build: shared lock 2026-11-07 15:01:21–15:02:01 UTC. Failed only because the new driver’s negative-validator catch_unwind required a generic RefUnwindSafe bound. No Tier 2 sort had run. The driver-only bound was added before its final freeze.
- Successful selective release build: shared lock 15:06:51–15:07:44 UTC. Only the two new bins and two isolated diagnostic test binaries were built. Diagnostic unused-test-helper warnings are retained. This was not a new complete runtime test-suite run; the unchanged paired runtime’s previously completed safety gates remain the prerequisite.
- Shard 1 preflight/route/heap: shared lock 15:07:55–15:07:56 UTC. Timing: exclusive 15:08:13–15:08:35 UTC, four cases. Its three favorable/one inconclusive result and absence of confirmed regression permitted shard 2.
- Shard 2 preflight/route/heap: shared lock 15:09:05–15:09:06 UTC. Timing: exclusive 15:09:36–15:09:55 UTC, four cases.
- The whole shell held FD9 on the existing lock, device 27/inode 1578019. Each window released before analysis. No timing overlap, cleanup or hidden replacement run was introduced by this task.
- A post-screen metadata assertion initially expected the short string Avx512 rather than its full Debug representation. The corrected audit verified the already-recorded detected and selected AVX512 values in all 72 processes; no runtime or sample changed.

## Build and frozen identities

AMD EPYC 9V74; portable-simd enabled; Fearless detected and actually selected AVX512 in all 72 timed subprocesses. Rust 1.99.1 (58cf8f9c3), Cargo 1.99.1, LLVM 23.1.1, Rayon 1.12.0 and fearless_simd 1.2.0. Default release optimization with debuginfo, no target-cpu/native, custom RUSTFLAGS or LTO override. TESSERA_* and relevant Rust/Rayon overrides were cleared and logged. CARGO_INCREMENTAL=0, offline builds, -j1. One existing target was reused; no frozen binary or another task’s target was removed. Final free disk was about 429 MiB.

- Upstream e1 core: e1d25297685cecb205d18114e898a39878e3fd83.
- Combined A source identity: ab73401745af53e114015e4ba6cc6a87ef91a5cfe3a1daa73036049189050e2a.
- Candidate C source identity: 8f300a35a875c8949d80fc306ed2ec24ddc9fbe6747663d4768369610b641b35.
- Timing binary SHA-256: 11101a16100555de2f907c7f1543392a1b8dc9d1735443e5015d5761fc6eb634.
- Allocation binary SHA-256: 60bb03688f074928a3a06dfdb00629d43925815c87bc4ab27b236b6717f1f75a.
- Pre-timing driver-manifest SHA-256: c85581a171035cfda8a1f87fcddec21cf2d28d31213b8e5a6e5653311da9778f.
- Unchanged earlier Tier 1 REPORT SHA-256: c57cda6e4dc59ea6d3871d6601f26e6489cec76ac9417427ff1b031b559a2260.

Source maps, diagnostic-only diffs, build failures, compiler/features, lock logs, full per-block samples, allocation observations and final checks are retained privately and hashed. The publication packet contains only this report, identities and checksums, not raw data, harness or binaries.

## Limits

Only these eight W8, n1M, seed1 Direct u32/u64 cells were run. There is no high-bit shifted-window, ordered/equal, smaller-size, record/owned, u128 or other-platform coverage in this checkpoint. No new kernel or layout was introduced. Broad controls prevent treating the inconclusive cells as either accepted speedups or established equivalence. Any further scope requires its own frozen inputs and controls; none was pre-run here.
