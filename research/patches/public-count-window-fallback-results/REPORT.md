# Public count-window: fallback regression gate

## Decision

All ten new n1M/W8 cases completed: **810 samples / 12,580 individual timed sorts**, all full-reference checks passed. C/A establishes 0 gains and 0 slowdowns under the predeclared full controls. The comparisons not classified as gain or slowdown remain inconclusive; that is not equivalence or evidence that regressions are absent. The u64 wide-skew99 primary comparison leans slower in 8/9 blocks but its independent placement and envelope do not confirm it. No rescue run, tail exclusion, input adjustment or runtime tuning occurred.

C/S separately establishes 5 favorable and 1 slower comparisons. In particular, u128/d256 is slower than original strongserial, while C/A is inconclusive. This is a public-policy versus serial difference, not established as a regression introduced by the count hook. The candidate count kernel was positively witnessed not entering in every case.

These observations concern the same frozen public Pool/Job/root baseline and count-only candidate as the prior 17-cell checkpoint, but use ten separate cases and new binaries. They are not pooled with the prior 17 cells or old e1 research-combined results. The prior equal sentinel is not rerun.

## Public API and controls

A1/A2 call the same baseline public parallel function; A_copy uses an independently placed source-identical package. C1/C2/C_copy do the same for the candidate. S1/S2 call original stable_sort or stable_sort_by_key; S_copy uses baseline-copy. All nine function-pointer invariants are checked in each smoke and timing process. Four unchanged private packages isolate placement. For ByKey, all versions use the same named key getter and fn-pointer type, with no cached-key API substitution. S is strongserial, not fixed-geometry same-kernel scheduling.

Nine randomized blocks per cell, exactly one fresh W8 Rayon pool per subprocess, affinity0–7 and no worker pins. Each sample accumulates at least 50 ms and two fresh sorts. Clone occurs on the actual pool worker before each API timer. The timer includes the entire public call and its internal allocation. Complete stable-reference equality and cleanup are outside the API timer. Each process primes all labels outside timing. All per-sort tails are preserved. The actual detected and selected Fearless level is checked independently, not used to infer that ByKey work is vectorized.

Primary ratios are the median of matched-block C duplicate mean / reference duplicate mean. A/A is label2/label1. Placement is copy/duplicate mean. Each C/A or C/S decision uses its own symmetric envelope of both groups’ complete A/A and placement ranges. Supported gain/slowdown requires the median outside that envelope and at least 8/9 primary and 8/9 independent-placement blocks in the same direction. Only confirmed C/A slowdown stops later cases; C/S is independently reported.

## Absolute milliseconds per sort

Values are medians across nine blocks of each within-block duplicate mean; paired ratios are not ratios of the displayed medians. At n1M, the ms number is also ns/element.

| Case | A public ms | C count-only ms | S original serial ms |
|---|---:|---:|---:|
| u32/fullrandom | 1.948209 | 1.934581 | 4.209914 |
| u32/standard_wide_skew99 | 1.627723 | 1.558196 | 1.571630 |
| u64/fullrandom | 2.240243 | 2.272195 | 5.223321 |
| u64/standard_wide_skew99 | 1.710712 | 1.775600 | 1.736591 |
| u128/fullrandom | 4.702113 | 4.691213 | 10.206116 |
| u128/d256 | 3.857544 | 4.004391 | 2.218730 |
| Row16/fullrandom | 4.937966 | 4.795675 | 11.148173 |
| Row16/d256 | 5.592800 | 5.263513 | 5.405314 |
| Owned32/fullrandom | 10.847244 | 10.723996 | 20.839172 |
| Owned32/d256 | 9.825219 | 9.875392 | 9.695497 |

## Paired comparisons and full controls

F=favorable, I=inconclusive, L=slower. Direction counts are primary favorable/slower and independent-placement favorable/slower.

| Case | Comparison | Paired median [range] | Full envelope | Primary fast/slow | Placement fast/slow | Result |
|---|---|---|---|---:|---:|---|
| u32/fullrandom | C/A | 1.007641 [0.918775–1.047652] | 0.786369–1.271668 | 4/5 | 3/6 | I |
| u32/fullrandom | C/S | 0.460392 [0.431429–0.477698] | 0.794746–1.258263 | 9/0 | 9/0 | F |
| u32/standard_wide_skew99 | C/A | 0.965416 [0.896795–1.123388] | 0.753389–1.327335 | 7/2 | 3/6 | I |
| u32/standard_wide_skew99 | C/S | 0.992014 [0.909599–1.156143] | 0.753389–1.327335 | 6/3 | 4/5 | I |
| u64/fullrandom | C/A | 0.988908 [0.930983–1.034356] | 0.906723–1.102873 | 6/3 | 4/5 | I |
| u64/fullrandom | C/S | 0.417087 [0.374652–0.447303] | 0.848087–1.179124 | 9/0 | 9/0 | F |
| u64/standard_wide_skew99 | C/A | 1.025246 [0.991349–1.083601] | 0.786114–1.272081 | 1/8 | 4/5 | I |
| u64/standard_wide_skew99 | C/S | 0.997641 [0.868118–1.103317] | 0.884266–1.130882 | 5/4 | 5/4 | I |
| u128/fullrandom | C/A | 0.951917 [0.797593–1.111901] | 0.665776–1.502007 | 5/4 | 7/2 | I |
| u128/fullrandom | C/S | 0.465799 [0.446383–0.534496] | 0.763584–1.309613 | 9/0 | 9/0 | F |
| u128/d256 | C/A | 1.025907 [0.938501–1.104291] | 0.563684–1.774044 | 2/7 | 3/6 | I |
| u128/d256 | C/S | 1.752448 [1.610869–1.978876] | 0.757335–1.320419 | 0/9 | 0/9 | L |
| Row16/fullrandom | C/A | 0.976822 [0.795920–1.192069] | 0.507897–1.968905 | 5/4 | 4/5 | I |
| Row16/fullrandom | C/S | 0.428974 [0.397665–0.513497] | 0.507897–1.968905 | 9/0 | 9/0 | F |
| Row16/d256 | C/A | 0.946705 [0.921605–1.066180] | 0.830915–1.203493 | 7/2 | 4/5 | I |
| Row16/d256 | C/S | 1.000199 [0.906538–1.081763] | 0.824434–1.212954 | 4/5 | 4/5 | I |
| Owned32/fullrandom | C/A | 0.936584 [0.709999–1.140410] | 0.503614–1.985649 | 6/3 | 4/5 | I |
| Owned32/fullrandom | C/S | 0.494297 [0.437653–0.564059] | 0.503614–1.985649 | 9/0 | 9/0 | F |
| Owned32/d256 | C/A | 1.009499 [0.815425–1.116676] | 0.684921–1.460023 | 4/5 | 3/6 | I |
| Owned32/d256 | C/S | 1.029480 [0.805206–1.188117] | 0.684921–1.460023 | 4/5 | 3/6 | I |

## A/A and placement ranges

No broad control range was pruned or replaced.

| Case | A A/A | C A/A | S A/A | A placement | C placement | S placement |
|---|---|---|---|---|---|---|
| u32/fullrandom | 0.935345–1.271668 | 0.963068–1.082234 | 0.930014–1.123988 | 0.917396–1.153253 | 0.953754–1.258263 | 0.923141–1.138391 |
| u32/standard_wide_skew99 | 0.873195–1.110715 | 0.753389–1.013758 | 0.926330–1.095743 | 0.916112–1.002552 | 0.877637–1.092279 | 0.928883–1.078370 |
| u64/fullrandom | 0.973215–1.094666 | 0.973131–1.102873 | 0.848087–1.070676 | 0.926553–1.057554 | 0.942071–1.087522 | 0.946044–1.077178 |
| u64/standard_wide_skew99 | 0.953122–1.045542 | 0.932988–1.029615 | 0.906372–1.063612 | 0.947380–1.272081 | 0.916117–1.062400 | 0.884266–1.045252 |
| u128/fullrandom | 0.733817–1.260919 | 0.943484–1.220649 | 0.847996–1.071149 | 0.665776–1.220051 | 0.763584–1.080464 | 0.915393–1.156341 |
| u128/d256 | 0.952646–1.094553 | 0.861028–1.320419 | 0.810952–1.021444 | 0.942722–1.774044 | 0.874714–1.067854 | 0.965493–1.058321 |
| Row16/fullrandom | 0.929471–1.371159 | 0.888930–1.059356 | 0.895483–1.119145 | 0.776678–1.260750 | 0.920688–1.968905 | 0.853057–1.122191 |
| Row16/d256 | 0.947625–1.141015 | 0.840575–1.126865 | 0.859153–1.181075 | 0.830915–1.069257 | 0.880655–1.079553 | 0.869608–1.212954 |
| Owned32/fullrandom | 0.809412–1.107894 | 0.941901–1.117553 | 0.823989–1.042356 | 0.834423–1.292565 | 0.801511–1.985649 | 0.860234–1.168004 |
| Owned32/d256 | 0.809523–1.115451 | 0.895599–1.460023 | 0.849531–1.214012 | 0.768778–1.151798 | 0.834722–1.225133 | 0.799926–1.119435 |

## Separate full-sort heap observations

Every row is one allocator-instrumented complete public sort after warmup, excluded from timing evidence. Input, stable reference, pool and actual-worker clone already exist before the captured baseline. The recorded peak is extra live requested bytes, not RSS, total process memory or per-worker stack. All 30 observations retain zero extra heap. Differences on existing fallback paths are not attributed to the new count kernel, which does not execute.

| Case | API | Peak extra bytes | Total requested bytes | Allocations | Retained delta |
|---|---|---:|---:|---:|---:|
| u32/fullrandom | A1 | 634118 | 1178638 | 90 | 0 |
| u32/fullrandom | C1 | 666262 | 1272866 | 93 | 0 |
| u32/fullrandom | S1 | 471710 | 631966 | 14 | 0 |
| u32/standard_wide_skew99 | A1 | 116400 | 181936 | 11 | 0 |
| u32/standard_wide_skew99 | C1 | 116400 | 181936 | 11 | 0 |
| u32/standard_wide_skew99 | S1 | 116400 | 181936 | 11 | 0 |
| u64/fullrandom | A1 | 1113792 | 1794984 | 151 | 0 |
| u64/fullrandom | C1 | 1112688 | 1826464 | 152 | 0 |
| u64/fullrandom | S1 | 919392 | 1142704 | 16 | 0 |
| u64/standard_wide_skew99 | A1 | 125758 | 191454 | 15 | 0 |
| u64/standard_wide_skew99 | C1 | 125758 | 191454 | 15 | 0 |
| u64/standard_wide_skew99 | S1 | 125758 | 191454 | 15 | 0 |
| u128/fullrandom | A1 | 1623356 | 3037404 | 193 | 0 |
| u128/fullrandom | C1 | 1685196 | 2971132 | 192 | 0 |
| u128/fullrandom | S1 | 1300876 | 1686316 | 16 | 0 |
| u128/d256 | A1 | 1233660 | 1299196 | 8 | 0 |
| u128/d256 | C1 | 1233660 | 1299196 | 8 | 0 |
| u128/d256 | S1 | 65536 | 65536 | 1 | 0 |
| Row16/fullrandom | A1 | 1625644 | 2618348 | 187 | 0 |
| Row16/fullrandom | C1 | 1686860 | 2745340 | 189 | 0 |
| Row16/fullrandom | S1 | 1301180 | 1649500 | 16 | 0 |
| Row16/d256 | A1 | 1233660 | 1266428 | 8 | 0 |
| Row16/d256 | C1 | 1233660 | 1266428 | 8 | 0 |
| Row16/d256 | S1 | 1233660 | 1266428 | 8 | 0 |
| Owned32/fullrandom | A1 | 2028154 | 2715766 | 310 | 0 |
| Owned32/fullrandom | C1 | 2027958 | 2682576 | 308 | 0 |
| Owned32/fullrandom | S1 | 1774432 | 1891264 | 18 | 0 |
| Owned32/d256 | A1 | 1753760 | 1770144 | 8 | 0 |
| Owned32/d256 | C1 | 1753760 | 1770144 | 8 | 0 |
| Owned32/d256 | S1 | 1753760 | 1770144 | 8 | 0 |

## Exact data and stable oracle

All cells use n1,000,000, seed1 and W8. Scalar fullrandom uses SplitMix64 next(), truncated to 32 bits for u32. Standard wide-skew99 uses zero if next()%100<99, otherwise (1+next()%9)*floor(MAX/10), consuming the second draw only for minority values. It has ten values spread over the key width and is distinct from narrow d256_hot99. u128 fullrandom concatenates two successive draws, first in the upper 64 bits. All new d256 records use one draw&255 without prefix swaps.

Row16 is repr(C){key:u64,id:u64}; Owned32 is repr(C){key:u64,payload:String}, with payload formatted as hexadecimal 16-digit original id followed by :tessera-owned-payload. Stable reference sorts by key only and full equality compares every field, including complete String contents. Thus duplicate-key reordering or payload loss fails validation. The negative validator fills a fresh clone with identical records and must fail the full oracle while leaving the original input fingerprint unchanged.

FNV-1a encodes primitive values in complete native-width little-endian bytes, including all 16 bytes of u128; Row16 encodes key then id; Owned32 encodes key, u64 little-endian content length and all String bytes, never pointers or padding. For the six new-type inputs, an independent Python implementation pins the complete-vector hash and first 16 keys before compilation; Rust checks them before measuring. Scalar hashes match their predeclared exact standard generators.

| Case | Input FNV-1a | Positive route witness |
|---|---|---|
| u32/fullrandom | 006a2471325121ed | prepare → original Pool/root fallback; C count not entered |
| u32/standard_wide_skew99 | da314e6d73bbb5a9 | prepare → original sort_node fallback; C count not entered |
| u64/fullrandom | 41ce490591624983 | prepare → original Pool/root fallback; C count not entered |
| u64/standard_wide_skew99 | 1e3aad039af561b5 | prepare → original sort_node fallback; C count not entered |
| u128/fullrandom | 49f8c5d5c643b3a9 | prepare → original Pool/root fallback; C count not entered |
| u128/d256 | 7b3e14686dad0898 | prepare → Narrow<0> → forward → Direct hook → sealed u128 decline → original sort_node |
| Row16/fullrandom | 6ab259ce5d704093 | prepare → original Pool/root fallback; C count not entered |
| Row16/d256 | f94b6b0a8944087c | prepare → ByKey default decline → original sort_node |
| Owned32/fullrandom | 925690e64fcf152b | prepare → original Pool/root fallback; C count not entered |
| Owned32/d256 | 818c970aab8c90a8 | prepare → ByKey default decline → original sort_node |

Candidate geometry witnesses record entered=false, no histogram layout and no count/fill work for all ten cases. A and S also have positive entry/fallback observations. The S diagnostic observes original serial entry, not each subsequent serial stage; no exact stage attribution is made for the u128/d256 C/S gap. Diagnostic copies add cfg(test)-only events and disable unrelated test fixtures. Their modifications never enter the timing binary.

## Actual owned shape and clone boundary

Owned32 actually measures size 32 bytes, alignment 8, key/payload offsets [0,8], and needs_drop=true. Each input has1,000,000 separately owned Strings, every length 38 and capacity 44: 32,000,000 vector bytes plus 44,000,000 requested payload-capacity bytes before allocator metadata. This is not a 32-byte inline payload. Row16 measures16bytes/alignment 8; u128 measures 16 bytes/alignment 16.

The following independent clone timing is outside every public-sort timer, taken on the actual pool worker. It is the median and full range of individual C1/C2 clone durations, not another sort speed ratio. Owned clones are deep clones. Stable-reference construction/comparison and destruction are also excluded; total wall cost therefore exceeds the reported API and clone costs. Input String capacities are observed above; clone String capacities were not separately captured.

| Case | C clone median ms | Individual clone range ms |
|---|---:|---|
| u128/fullrandom | 1.296721 | 1.139786–1.895671 |
| u128/d256 | 1.328178 | 1.184933–7.726128 |
| Row16/fullrandom | 1.372524 | 1.172242–5.609416 |
| Row16/d256 | 1.298763 | 1.173884–3.482224 |
| Owned32/fullrandom | 32.415579 | 20.345088–39.898785 |
| Owned32/d256 | 28.455904 | 25.932588–37.302599 |

## Complete samples, tails and lock windows

Each input has its own preserved raw directory and actual exclusive window. No live competing native work is admitted during those windows. Analysis occurs after release.

| Case | Samples | Timed sorts | Every-sort duration range ms | Exclusive ACQUIRED / RELEASE UTC | Raw directory |
|---|---:|---:|---|---|---|
| u32/fullrandom | 81 | 1751 | 1.564276–8.334161 | 2026-11-07T16:42:59Z / 2026-11-07T16:43:05Z | evidence/u32-fullrandom |
| u32/standard_wide_skew99 | 81 | 2585 | 1.426236–7.200742 | 2026-11-07T16:43:57Z / 2026-11-07T16:44:03Z | evidence/u32-standard_wide_skew99 |
| u64/fullrandom | 81 | 1494 | 1.800160–9.732193 | 2026-11-07T16:44:14Z / 2026-11-07T16:44:21Z | evidence/u64-fullrandom |
| u64/standard_wide_skew99 | 81 | 2367 | 1.538555–6.337195 | 2026-11-07T16:44:30Z / 2026-11-07T16:44:38Z | evidence/u64-standard_wide_skew99 |
| u128/fullrandom | 81 | 736 | 3.561335–20.286043 | 2026-11-07T16:51:00Z / 2026-11-07T16:51:09Z | declined/evidence/u128-fullrandom |
| u128/d256 | 81 | 1332 | 1.958677–31.568761 | 2026-11-07T16:51:33Z / 2026-11-07T16:51:43Z | declined/evidence/u128-d256 |
| Row16/fullrandom | 81 | 709 | 3.773582–30.063454 | 2026-11-07T16:52:26Z / 2026-11-07T16:52:35Z | declined/evidence/Row16-fullrandom |
| Row16/d256 | 81 | 788 | 4.621728–14.990470 | 2026-11-07T16:52:57Z / 2026-11-07T16:53:06Z | declined/evidence/Row16-d256 |
| Owned32/fullrandom | 81 | 361 | 7.572718–33.769440 | 2026-11-07T16:53:30Z / 2026-11-07T16:55:41Z | declined/evidence/Owned32-fullrandom |
| Owned32/d256 | 81 | 457 | 7.982414–23.389504 | 2026-11-07T16:55:55Z / 2026-11-07T16:58:28Z | declined/evidence/Owned32-d256 |

## Validation, source identity and preserved technical failures

The original final paired release/default/MSRV/Clippy/no_std and scoped scalar Miri safety gates precede this experiment. This extension does not claim to rerun those complete suites or Miri with a real Rayon pool. Its shared native checks are two focused diagnostic tests for the scalar phase and two for the six-type phase,90 all-label warmup full references,10 wrong-mutating-validator checks and 30 separate allocation observations. Each timed sort also checks the complete reference. Actual Fearless detected and selected dispatch is AVX512 in all 90 timing subprocesses. Process affinity is0–7. Rust1.99.1/LLVM23.1.1, release with debuginfo, offline -j1/incremental0; frozen registry records and rayon+portable-simd production features are reused. No research feature, selector or TLS is present. TESSERA_* and compiler/Rayon overrides are cleared and logged.

Shared scalar build16:41:06–16:41:41Z passed. The first shared preflight at16:41:55Z stopped before any sort because --locked rejected private diagnostic-package lock metadata. Offline normalization16:42:26Z preserved all registry records, inputs, production sources and binaries; the original freeze and failure remain recorded. The normalized shared preflight16:42:26–16:42:37Z passed. The first scalar analysis found a missing-space syntax error in the copied analyzer; the frozen analyzer remains unchanged and an explicitly recorded analyze_case_v2.py corrects that parser error and its displayed C/A gate. No sample was rerun.

Independent six-type generator hashing ran shared16:46:50–16:47:17Z. All six generators/getters/oracles and controls were then frozen together. The second timing/memory bin pair built shared16:48:59–16:50:05Z; its preflight ran shared16:50:20–16:50:54Z. Final linked bins are about 27 MiB each; lowest observed link-check free space was 290 MiB, above the 150 MiB reserve. No frozen artifact or target was deleted. Four runtime packages and one target directory were reused, with four new distinct benchmark binaries.

- Public baseline commit: 987fbdfe757cc81330db04ee11ed6e454b2e5142.
- Runtime core: 231e1f360a45647d98809539b1a779a62770f88c.
- Baseline source identity: 19659df2fd57dfcde3b82892bb01b84a6a5770cadd5094bb9815a6cfd70d9f17.
- Candidate source identity: b4c51ddca807a94260e6399800f17f06b4463fde6a4162f07a9f6df094d84f45.
- Scalar initial source freeze:880e19617410eb65f78b6fa66b49039a0621fea0fc1bde90586ff7f22e6cc20a.
- Scalar normalized diagnostic/source freeze:3674017abc4dadead3a65e3e154fd62720efc84e4f150153b5176bbd4230e667.
- Six-type source freeze:ebcb8f277a15212be92981abf579ce2644e45c3d7d1e925509c46de1eea0f1a4.

## Limits

One machine, one seed, W8 only and one million records per case. Inconclusive C/A controls do not establish equivalence or rule out smaller regressions, particularly where primary direction leans slower or controls are broad. Existing public-versus-serial differences are not count-hook improvements. Owned32 is one specific String workload and does not cover all ownership or !Sync shapes. u128/d256 covers the low-half narrowing/decline, not high-half positive coverage. No generic speedup, wider scalability proof, runtime adoption, code change, commit or push follows from this gate. The prior 17-cell report, sources, two binaries, raw evidence and older seven binaries remain unchanged. This source-only publication packet excludes raw data, harness and binaries.
