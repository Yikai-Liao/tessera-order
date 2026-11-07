# e1 Direct count-window: production-feature confirmation

## Decision

With the research feature OFF, candidate C shows a favorable W8 signal against the same-core combined baseline A in **3/4 cases**, and against immutable public experiment U in **4/4 cases**. u32/d256 versus A remains inconclusive because its full placement controls are broad. Across W2/W4/W8, eight of twelve cells clear controls against each baseline. No tested cell establishes a regression. This is bounded evidence for Direct u32/u64 at n=10M, not general production acceptance.

The predeclared width1 sentinel established neither gain nor slowdown and therefore allowed Tier 1 to proceed. All 1,611 samples and 7,360 individual timed sorts passed full-reference equality. No tails were excluded, no rerun replaced a result, and no Tier 2 or layout/kernel change was performed.

## Labels and measurement

A is the unchanged combined e1 core; C adds the count-window kernel with research OFF; U is public-experiment commit 3653c360; R is source-identical C with research ON and normal scheduling. A/C/U have literal same-pointer A/A duplicates and an independent source-identical package placement. W8 adds R A/A+placement. strongserial is the original serial API as a contextual single label. Eight distinct Cargo packages prevent feature unification into C.

Each input/block/W runs in a **fresh one-pool subprocess**. Within each of nine outer blocks, W1/2/4/8 order is randomized; labels are then randomized within that W stratum. The controller waits for process exit before the next. No co-resident pools or sleep/spin tuning. Every sample accumulates at least 50 ms and two fresh sorts. Cloning occurs on the actual worker before each individual timer; complete stable-reference equality follows it. Full API preparation/allocation are timed. Process affinity 0–7, no worker pins.

W8 R and R_copy each execute an untimed serial-scope full-reference check, then a normal check and warmup. This keeps the research TLS setter reachable; diagnostics verify restoration on the actual worker. No S scheduling label is timed here.

## Absolute milliseconds per public sort

Entries are medians across nine blocks of within-block duplicate means. strongserial has one contextual sample per block. Paired ratios below are medians of block ratios, not ratios of displayed medians.

| Input, n=10M | W | A combined | C OFF | U public | strongserial |
|---|---:|---:|---:|---:|---:|
| u32/d256 | 1 | 11.055155 | 10.854373 | 11.343428 | 11.178870 |
| u32/d256 | 2 | 10.812510 | 6.994496 | 10.617261 | 10.943031 |
| u32/d256 | 4 | 11.513809 | 6.113979 | 11.496954 | 11.011047 |
| u32/d256 | 8 | 11.554943 | 5.047311 | 11.306989 | 11.273022 |
| u32/d256_hot99 | 1 | 16.778646 | 16.318821 | 16.477362 | 16.149810 |
| u32/d256_hot99 | 2 | 16.410593 | 9.396661 | 16.866008 | 16.420701 |
| u32/d256_hot99 | 4 | 16.447437 | 7.149219 | 16.918487 | 15.929244 |
| u32/d256_hot99 | 8 | 17.064877 | 6.525391 | 16.788238 | 16.570853 |
| u64/d256 | 1 | 15.978925 | 15.774912 | 15.703232 | 15.614949 |
| u64/d256 | 2 | 16.387250 | 10.353205 | 15.694199 | 15.580721 |
| u64/d256 | 4 | 15.748832 | 9.676015 | 16.019162 | 15.565195 |
| u64/d256 | 8 | 15.623296 | 9.468167 | 15.441776 | 15.869541 |
| u64/d256_hot99 | 1 | 20.910909 | 20.586348 | 20.768011 | 20.410560 |
| u64/d256_hot99 | 2 | 20.834018 | 12.882921 | 21.312208 | 20.971829 |
| u64/d256_hot99 | 4 | 20.709195 | 10.484675 | 20.805933 | 20.663702 |
| u64/d256_hot99 | 8 | 21.740015 | 10.651519 | 21.343469 | 20.318551 |

## Paired comparisons

F = supported favorable signal; I = inconclusive under full controls. Claims require the median outside the conservative symmetric union of both algorithms’ raw A/A and placement ranges, at least 8/9 agreeing primary blocks, and matching independent-placement direction. All F cells have 9/9 favorable primary blocks. All W1 comparisons are I.

| Input | W | C/A median [range] | Favorable /9 | A result | C/U median [range] | U result |
|---|---:|---:|---:|---|---:|---|
| u32/d256 | 1 | 1.001442 [0.927697–1.138026] | 4 | I | 0.961067 [0.772603–1.084462] | I |
| u32/d256 | 2 | 0.655135 [0.543159–1.187058] | 8 | I | 0.661891 [0.560448–1.097640] | I |
| u32/d256 | 4 | 0.580413 [0.427113–0.849833] | 9 | I | 0.562277 [0.455808–0.859114] | I |
| u32/d256 | 8 | 0.451047 [0.385631–0.561112] | 9 | I | 0.451917 [0.371925–0.591089] | F |
| u32/d256_hot99 | 1 | 1.001301 [0.961234–1.103648] | 4 | I | 0.996525 [0.973707–1.119370] | I |
| u32/d256_hot99 | 2 | 0.565934 [0.532672–0.607616] | 9 | F | 0.556728 [0.531541–0.609138] | F |
| u32/d256_hot99 | 4 | 0.403507 [0.354929–0.606939] | 9 | F | 0.418838 [0.382417–0.568387] | F |
| u32/d256_hot99 | 8 | 0.380284 [0.281586–0.450406] | 9 | F | 0.360957 [0.297680–0.464804] | F |
| u64/d256 | 1 | 1.005743 [0.903446–1.118618] | 4 | I | 0.984690 [0.908504–1.076535] | I |
| u64/d256 | 2 | 0.658793 [0.613894–0.760420] | 9 | F | 0.667156 [0.641694–0.789224] | F |
| u64/d256 | 4 | 0.629311 [0.542539–0.904258] | 9 | F | 0.582868 [0.434398–0.884073] | I |
| u64/d256 | 8 | 0.594516 [0.551094–0.637982] | 9 | F | 0.600704 [0.492776–0.656405] | F |
| u64/d256_hot99 | 1 | 0.993066 [0.886179–1.066909] | 5 | I | 0.978173 [0.921946–1.117490] | I |
| u64/d256_hot99 | 2 | 0.615935 [0.597717–0.871235] | 9 | F | 0.602461 [0.538278–0.832037] | F |
| u64/d256_hot99 | 4 | 0.519699 [0.449760–0.731804] | 9 | I | 0.519127 [0.440833–0.686945] | I |
| u64/d256_hot99 | 8 | 0.474724 [0.417942–0.519476] | 9 | F | 0.480867 [0.432480–0.545063] | F |

## Complete A/A and placement ranges

A/A = second identical label / first; placement = independent copy / duplicate mean in the same block/W. All entries are raw minimum–maximum across nine blocks, including tails.

| Input | W | A A/A | C A/A | U A/A | A placement | C placement | U placement |
|---|---:|---:|---:|---:|---:|---:|---:|
| u32/d256 | 1 | 0.828646–1.082697 | 0.836620–1.209989 | 0.675570–1.124530 | 0.902365–1.144642 | 0.847198–1.056013 | 0.834489–1.028473 |
| u32/d256 | 2 | 0.851183–1.152814 | 0.782733–1.133298 | 0.913142–1.162395 | 0.849922–1.120024 | 0.937960–1.735292 | 0.969786–1.090319 |
| u32/d256 | 4 | 0.995619–1.224495 | 0.513210–1.245319 | 0.791903–1.034141 | 0.877257–1.124132 | 0.728734–1.320419 | 0.927655–1.380658 |
| u32/d256 | 8 | 0.910802–1.157703 | 0.720738–1.739272 | 0.742622–1.095663 | 0.921105–2.334451 | 0.736530–1.612214 | 0.865900–1.174713 |
| u32/d256_hot99 | 1 | 0.980320–1.167255 | 0.947507–1.155515 | 0.929045–1.057288 | 0.959098–1.057738 | 0.885851–1.030014 | 0.972262–1.301103 |
| u32/d256_hot99 | 2 | 0.960858–1.050749 | 0.875758–1.163113 | 0.978220–1.075845 | 0.874255–1.130416 | 0.903385–1.139517 | 0.950670–1.089652 |
| u32/d256_hot99 | 4 | 0.962575–1.323936 | 0.890371–1.248385 | 0.899965–1.230000 | 0.921938–1.111209 | 0.769254–1.112960 | 0.893716–1.022986 |
| u32/d256_hot99 | 8 | 0.883295–1.148089 | 0.902001–1.798677 | 0.950936–1.080847 | 0.939742–2.240459 | 0.734067–1.155938 | 0.960060–1.035045 |
| u64/d256 | 1 | 0.820197–1.148530 | 0.977491–1.149643 | 0.798704–1.151142 | 0.887792–1.087951 | 0.909223–1.107149 | 0.933664–1.246870 |
| u64/d256 | 2 | 0.906557–1.180749 | 0.852527–1.266797 | 0.924566–1.148575 | 0.888894–1.105379 | 0.791471–1.246445 | 0.947014–1.058525 |
| u64/d256 | 4 | 0.880657–1.105975 | 0.824994–1.396354 | 0.375712–1.377940 | 0.936434–1.293100 | 0.641045–1.141753 | 0.582298–1.078456 |
| u64/d256 | 8 | 0.848591–1.145779 | 0.809048–1.133652 | 0.759515–1.072139 | 0.937252–1.143650 | 0.929089–1.233787 | 0.816511–1.067042 |
| u64/d256_hot99 | 1 | 0.940653–1.233299 | 0.784744–1.096697 | 0.890281–1.106796 | 0.932722–1.053971 | 0.935542–1.089042 | 0.911658–1.272498 |
| u64/d256_hot99 | 2 | 0.921219–1.040263 | 0.878687–1.230334 | 0.840029–1.116279 | 0.922977–1.061794 | 0.830823–1.157944 | 0.911720–1.042439 |
| u64/d256_hot99 | 4 | 0.915300–1.104135 | 0.498084–1.177834 | 0.885706–1.033452 | 0.950066–1.737966 | 0.802633–1.076021 | 0.948274–1.124896 |
| u64/d256_hot99 | 8 | 0.656476–1.076291 | 0.869156–1.264662 | 0.697695–1.130918 | 0.835206–1.150012 | 0.826755–1.422633 | 0.813991–1.238177 |

Retained individual tails include 68.746687ms (u32/d256 W8 R1, block 1), 40.278555ms (same case W8 A_copy, block 0), 66.194828ms (u64/d256 W4 U1, block 0), and 50.594175ms (u64/hot W4 A_copy, block 1). Their cause is unproven. Broad controls block claims; they are not explained away as harmless noise or assigned to layout, host interference or worker waiting.

## Research ON/OFF and public-policy curves

| W8 input | R ON ms | R/C median [range] | R A/A | R placement |
|---|---:|---:|---:|---:|
| u32/d256 | 5.391568 | 1.064409 [0.969001–2.283812] | 0.214183–1.107492 | 0.349396–1.103832 |
| u32/d256_hot99 | 6.358966 | 0.981127 [0.652832–1.223213] | 0.760040–1.180957 | 0.762726–1.786935 |
| u64/d256 | 9.234108 | 0.990790 [0.939697–1.154035] | 0.889836–1.239594 | 0.799964–1.059590 |
| u64/d256_hot99 | 10.397225 | 0.998207 [0.874480–1.205818] | 0.888155–1.171237 | 0.801045–1.262649 |

No R/C difference clears its controls. This is **no detected deviation, not equivalence**; u32/d256 has particularly weak precision. C’s normal-feature benefit is not dependent on enabling the research selector.

Paired complete-API C(W)/C(W1), descriptive matched-block **public-policy** ratios:

| Input | W2/W1 | W4/W1 | W8/W1 |
|---|---:|---:|---:|
| u32/d256 | 0.644394 | 0.554765 | 0.438753 |
| u32/d256_hot99 | 0.569786 | 0.431919 | 0.391544 |
| u64/d256 | 0.653343 | 0.599183 | 0.584925 |
| u64/d256_hot99 | 0.621467 | 0.509302 | 0.510316 |

W1 takes the original serial fallback; W2+ changes both intrinsic implementation and scheduling. These are not pure-kernel scalability claims or independent acceptance tests. W4→W8 is nearly flat for u64/hot. Cross-W runs are stratified, not fully interleaved labels; active-pool waiting, frequency and host effects remain unmeasured. The historical e8 same-kernel S result is separate and is not pooled here.

## Width1 sentinel and memory

u64/window_01, n=10M/W8: A 19.958824ms, C 20.527440ms, strongserial 20.065521ms. Paired C/A 0.993654 [0.942452–1.201357], five favorable/four slower blocks. Independent-placement median 0.951254; full symmetric control envelope [0.595862, 1.678241]. Inconclusive; no confirmed regression, so the declared stop gate was not met.

| Sentinel control | Range |
|---|---:|
| A_AA | 0.950400–1.049611 |
| C_AA | 0.934735–1.057802 |
| A_placement | 0.964206–1.678241 |
| C_placement | 0.967121–1.219392 |

All 68 Tier 1 full-sort allocator observations and three sentinel observations had peak extra requested heap 65,536 bytes, retained delta 0, and correct full output. Input/reference/clone/pool allocations are excluded.

| Configuration | W | Allocation calls | Requested total bytes | Extra versus A | Observed peak bytes |
|---|---:|---:|---:|---:|---:|
| A/U/strongserial, four eight-bit inputs | 1/2/4/8 | 1 | 65,536 | 0 | 65,536 |
| C OFF, four eight-bit inputs | 1 | 1 | 65,536 | 0 | 65,536 |
| C OFF, four eight-bit inputs | 2 | 3 | 71,688 | 6,152 | 65,536 |
| C OFF, four eight-bit inputs | 4 | 3 | 75,784 | 10,248 | 65,536 |
| C OFF/R ON, four eight-bit inputs | 8 | 3 | 83,976 | 18,440 | 65,536 |
| C OFF, width1 sentinel | 8 | 3 | 65,688 | 152 | 65,536 |

Exact new vectors request (W×R+R+1) usize cells: 18,440 bytes at W8/R256 and 152 at W8/R2. Extra requests did not increase observed full-sort peak; no phase trace establishes their lifetime, and stack/runtime overhead is unmeasured. The source keeps its checked square-root budget and no n-record scratch. Diagnostic bases were 16 mod64; possible row cacheline sharing remains unmeasured and no padding was tuned.

## Inputs, validation and identities

The four Tier 1 vectors reuse SplitMix64 seed 1: d256 = next() & 255; d256_hot99 = exactly 99% zero and 1% values 1..255, shuffled. Existing 255,0,1 are swapped into the first three positions, preserving the multiset. The width1 sentinel uses next() & 1 and existing first 3 [1,0,1]. Neither diagnostic skew/two-value distribution is the standard full-width distribution.

| Input | Bytewise FNV-1a fingerprint | Timed sorts | Individual duration range ms | Exclusive UTC window, 2026-11-07 |
|---|---|---:|---:|---|
| u32/d256 | 2e2841855d838d04 | 2325 | 4.000252–68.746687 | 14:39:23–14:40:54 |
| u32/d256_hot99 | f6fe43a8cddc8ae4 | 1824 | 4.455703–47.643637 | 14:41:31–14:42:58 |
| u64/d256 | fa9d13862cbe1a44 | 1659 | 7.875099–66.194828 | 14:43:16–14:45:24 |
| u64/d256_hot99 | 558a5e14eed7a934 | 1365 | 7.886130–50.594175 | 14:45:42–14:47:45 |
| u64/window_01 | 8d7f04e5e56d96a4 | 187 | 17.342809–47.868854 | 14:37:40–14:37:58 |

All 71 exact route observations passed: A/U/strongserial use original Direct counting; C W1 falls back; C W2/4/8 uses W count and W fill chunks of n/W records, R256 for the main cases and R2 for the sentinel. C-OFF/R-ON W8 geometry matches, and R TLS restoration is verified. All 16 small main smokes plus the sentinel smoke passed. The main validator caught an intentionally wrong mutating variant; fresh-input and JSON duration/count checks passed.

Independent paired safety checks passed production-OFF release tests (A 63/C 71 unit plus 23 integration each; one preexisting ignored diagnostic), default backend, no_std, Clippy -D warnings and Rust 1.85 checks. Two research-scope tests and two strict non-Rayon wide-tile Miri tests passed. Separate diagnostic copies execute only the targeted witnesses; they are not claimed as a full safety-suite rerun. No sort, route, reference or retained-allocation failure occurred. Pre-refinement binaries were never executed.

All builds/diagnostics/allocator runs used shared FD9; timing used exclusive FD9 on existing device 27 / inode1578019, released before analysis. Final shared rebuild released 14:36:22, validation 14:37:26, allocator run 14:38:35; last timing release 14:47:45. Sources and three final binaries were rechecked after the screen.

AMD EPYC 9V74; Rust 1.99.1/LLVM 23.1.1; Rayon 1.12.0 and fearless_simd 1.2.0. Detected and actually selected dispatch: AVX512. portable-simd enabled throughout. Default release with debug information, no target-cpu/LTO/alignment/TESSERA overrides. Compiler, features, environment and host captures are retained.

- Shared runtime: e1d25297685cecb205d18114e898a39878e3fd83.
- A source identity: ab73401745af53e114015e4ba6cc6a87ef91a5cfe3a1daa73036049189050e2a.
- C/R source identity: 8f300a35a875c8949d80fc306ed2ec24ddc9fbe6747663d4768369610b641b35.
- U immutable commit: 3653c3607b9f567e2f03ceb6fdd131e18eaee519; extracted manifest identity 0372d06a6b5ce1d9db9d064634ce620375e6ae7be9e9b2946324d290e3a89e12.

Identities are SHA256 of canonical sorted complete file-hash maps. Runtime Rust files in each placement match their origins byte for byte; private Cargo names/workspace membership are the only vendoring changes. Binary SHA256:

- 995de431328552b0e3f2d2622d7cb2cd8f7ff8ed1748f3a82b87e30adda67900  target/release/tessera-parallel-count-window-e1-screen
- c0dfbc8d0cd42a4dda356ac8298f90050375d8eb074c02bd5d47fcbfa8302412  target/release/full-memory
- 8643a2bb1cca5cca40c8dbc4c3f0a5fee555b527c1c5a5cc5d3de56947a3326a  target/release/sentinel

**Remaining scope:** other lengths, shifts, widths 2–7, fullrandom, standard wide skew, generic/owned records and cross-run/machine repeatability are untested here. Tier 2 is not run. The source-only packet contains this report and identities/checksums; all raw observations, long tails, diagnostic diffs and harness remain preserved privately. Previous e8 evidence is unchanged.
