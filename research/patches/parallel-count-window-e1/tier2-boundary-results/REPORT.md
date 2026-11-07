# e1 Direct count-window: shifted and early-exit boundaries

## Decision

All four high-bit shifted-window cases are **inconclusive under the full controls**. C/A median times look favorable (0.603–0.763), with at least eight of nine primary blocks favorable, but none clears the complete A/A-plus-placement envelope. Those tails remain included; this checkpoint does not claim a demonstrated shifted-window speedup.

The six ordered/equal cases also establish neither a gain nor a regression. Across all ten cases there are 0 supported favorable signals and 0 supported regressions. Inconclusive is not equivalence or absence-of-regression proof. The purpose here is bounded high-bit reconstruction and early-exit coverage, not generic-sort or pure-thread-scaling acceptance.

All 630 samples and 139,113 individual timed sorts passed full stable-reference equality. Thirty separate full-sort allocator observations retained zero extra heap. No sample or individual tail was dropped and no replacement run was used. Every per-case regression stop gate remained clear. No runtime, threshold, alignment, padding or kernel change was made.

The earlier Tier 1 checkpoint and the completed eight-cell Tier 2 checkpoint remain unchanged, including their source, manifests, reports, packets and all five binaries. This new report is a separate package, not a revision or pooled analysis.

## Method and attribution

All cases use n=1,000,000, W8 and SplitMix64 seed 1. Seven labels are A1/A2/A_copy, C1/C2/C_copy and strongserial. A is the frozen combined e1 public parallel API; C is its count-window candidate with the research feature OFF. Each duplicate pair calls one identical function pointer, while the copy is a distinct, independently placed source-identical package. strongserial is contextual original serial API, not a same-kernel scheduling control.

Each of nine blocks is a fresh subprocess containing one W8 pool. Labels are randomized and primed outside measurement. Fresh cloning occurs on the actual pool worker before each timer; complete stable-reference equality follows it. Every sample has at least 50 ms cumulative sort time and two sorts. Public API preparation and scratch allocation are included. Process affinity is CPUs 0–7, without worker pins or spin/sleep tuning. No across-W, research-ON, U/public-experiment or same-kernel-S label is measured here.

The statistic is the median of matched-block C duplicate mean / A duplicate mean, not a ratio of the displayed median milliseconds. Claims require the median outside the symmetric union of both raw A/A and placement ranges, at least 8/9 primary directions, and at least 8/9 independent-placement directions. Only the small predeclared regression gate runs between completed cases; full report analysis occurs after immediate lock release.

## Absolute milliseconds and paired ratios

A/C entries are medians of within-block duplicate means. strongserial is the median of its single-label sample means. Since n=1M, ms/sort numerically equals ns/element.

| Input | A ms | C OFF ms | strongserial ms | C/A median [range] | Primary favorable /9 | Placement favorable /9 | Result |
|---|---:|---:|---:|---|---:|---:|---|
| u32/top4bit | 0.948246 | 0.722395 | 0.976120 | 0.763150 [0.578939–0.954975] | 9 | 8 | Inconclusive |
| u32/top8bit | 0.902226 | 0.655458 | 0.868090 | 0.711062 [0.588384–0.854219] | 9 | 9 | Inconclusive |
| u64/top4bit | 1.148249 | 0.783364 | 1.107637 | 0.665500 [0.537891–1.034908] | 8 | 8 | Inconclusive |
| u64/top8bit | 1.043039 | 0.631376 | 1.056294 | 0.602890 [0.549699–1.323722] | 8 | 8 | Inconclusive |
| u32/ascending_d256 | 0.078047 | 0.077890 | 0.078040 | 0.982604 [0.960321–1.036941] | 6 | 3 | Inconclusive |
| u64/ascending_d256 | 0.162518 | 0.162666 | 0.160786 | 0.994783 [0.900071–1.080568] | 5 | 2 | Inconclusive |
| u32/descending_d256 | 0.770977 | 0.775968 | 0.753978 | 0.996350 [0.912619–1.048394] | 5 | 3 | Inconclusive |
| u64/descending_d256 | 0.939853 | 0.956472 | 0.936615 | 1.005804 [0.956563–1.064673] | 3 | 6 | Inconclusive |
| u32/equal37 | 0.073861 | 0.083291 | 0.082034 | 1.125452 [1.030865–1.165691] | 0 | 8 | Inconclusive |
| u64/equal37 | 0.193506 | 0.187696 | 0.169672 | 0.983258 [0.702048–1.049558] | 5 | 4 | Inconclusive |

Shifted comparisons are full-API effects, not pure parallelism: the candidate kernel changes counting/fill work and shares serial preparation with A. Ordered/equal return before the new count kernel; any apparent difference there cannot be attributed to that kernel executing.

## Complete control ranges

| Input | A2/A1 | C2/C1 | A copy / pair mean | C copy / pair mean | Symmetric envelope |
|---|---|---|---|---|---|
| u32/top4bit | 0.866078–1.047951 | 0.609543–1.176381 | 0.903362–1.172991 | 0.852754–1.617212 | 0.609543–1.640573 |
| u32/top8bit | 0.783073–1.181457 | 0.611474–1.343719 | 0.881903–1.097886 | 0.798706–1.329512 | 0.611474–1.635392 |
| u64/top4bit | 0.840062–1.067826 | 0.628189–1.433598 | 0.881933–1.061330 | 0.731627–1.225887 | 0.628189–1.591878 |
| u64/top8bit | 0.910453–1.073078 | 0.590507–1.072364 | 0.923995–1.048900 | 0.692634–1.207278 | 0.590507–1.693460 |
| u32/ascending_d256 | 0.938597–1.115213 | 0.879581–1.049706 | 0.932357–1.018485 | 0.954455–1.271228 | 0.786641–1.271228 |
| u64/ascending_d256 | 0.864687–1.093148 | 0.871697–1.122827 | 0.897591–1.177366 | 0.925372–1.037145 | 0.849353–1.177366 |
| u32/descending_d256 | 0.944088–1.066886 | 0.852038–1.093575 | 0.912633–1.042690 | 0.974910–1.062100 | 0.852038–1.173657 |
| u64/descending_d256 | 0.952196–1.057108 | 0.861787–1.061060 | 0.941877–1.164705 | 0.852372–1.057543 | 0.852372–1.173197 |
| u32/equal37 | 0.936771–1.160136 | 0.959332–1.082379 | 1.045755–1.140946 | 0.841081–1.070330 | 0.841081–1.188947 |
| u64/equal37 | 0.946768–1.945815 | 0.918296–1.187087 | 0.622406–1.029342 | 0.877322–1.030574 | 0.513923–1.945815 |

The broad shifted C A/A minima, about 0.591–0.628, and placements as high as 1.617 are retained. In particular u64/top8bit has a 0.602890 median but a 0.590507 lower envelope, so it remains inconclusive rather than being rounded into acceptance.

u32/equal37 deserves an explicit caution: its primary C/A median is 1.125452 with all nine primary blocks slower. It remains inside the full envelope (upper 1.188947), and only one of nine independent-placement comparisons is slower. This is an unresolved slower primary tendency, not a confirmed regression and not permission to call the early-exit path regression-free. No cause such as binary layout was established.

## Exact input construction and route evidence

- top4bit/top8bit: generate next() & 15 or next() & 255; move existing [digit_mask,0,1] first without changing the multiset. Shift by 28/24 for u32 or 60/56 for u64. Set every bit outside that window to the type-truncated 0x5a5a5a5a5a5a5a5a pattern, masked outside the window. Each key is fixed bits OR shifted digit. Assert the exact varying mask and fixed-bit preservation. Full-reference comparison includes the complete keys, so a fill that corrupts the fixed bits fails.
- ascending_d256/descending_d256: sort the same seed-1 next()&255 multiset in the named direction, with no swaps after ordering. Generator sorting is outside measured calls.
- equal37: one million keys equal to 37, with no forced variation. This is the prior plan’s equal_37 case using the selected label spelling.

Fingerprints are FNV-1a over each native-width little-endian key. They match across diagnostics, allocator observations, smoke and every timed block.

| Input | Input FNV-1a | A/C public-call route |
|---|---|---|
| u32/top4bit | ce88c75c9708e295 | A serial count / C parallel count, low=28, width=4 |
| u32/top8bit | 723c0feeb4c8ac5e | A serial count / C parallel count, low=24, width=8 |
| u64/top4bit | 2b042f702cd2a595 | A serial count / C parallel count, low=60, width=4 |
| u64/top8bit | c6b197b30cd105ce | A serial count / C parallel count, low=56, width=8 |
| u32/ascending_d256 | a1fe5899857e5778 | Preparation early return; no count entry |
| u64/ascending_d256 | 432f4e64251d2358 | Preparation early return; no count entry |
| u32/descending_d256 | f4a8a4ed6baaf428 | Preparation early return; no count entry |
| u64/descending_d256 | aaf13860c7d3f6f8 | Preparation early return; no count entry |
| u32/equal37 | 99b1063bc410ad25 | Preparation early return; no count entry |
| u64/equal37 | 11147acb7fb99725 | Preparation early return; no count entry |

Test-only copies witness the exact Direct count call arguments. Shifted C has eight rows, 16/256 buckets and eight 125,000-record count tasks plus eight 125,000-record fill tasks. A takes the original serial count route with the same low/width. The actual task-to-worker mapping is not constrained to one task per worker. Histogram base mod64 observations are recorded separately and were not tuned or treated as a cache-sharing measurement.

For all six ordered/equal cases, both public calls are positively witnessed returning from prepare_sort before the count branch, not merely failing to set a count flag. No count layout or count/fill work appears. Narrow forwarding is explicitly instrumented and observed zero: these are Direct u32/u64 calls, not a positive u128/Narrow adaptation test.

Ten all-label smoke runs verify full output and catch a deliberately wrong mutating variant. The equal37 negative control writes zero, so it remains nontrivial. Four groups each run focused A and C diagnostic tests (eight focused tests total), with full-reference checks. JSON report checks retain exact elapsed sums, counts, units and fingerprints. Instrumentation and counting allocation are absent from the timing binary.

## Exact full-sort heap observations

Input/reference construction and the fresh work-buffer clone are excluded. These are single warmed allocator observations, not lifetime traces or per-thread peaks. Every retained delta is zero.

| Input | Label | Peak extra bytes | Total requested bytes | Allocation calls | Retained delta |
|---|---|---:|---:|---:|---:|
| u32/top4bit | A1 | 65536 | 65536 | 1 | 0 |
| u32/top4bit | C1 | 65536 | 66696 | 3 | 0 |
| u32/top4bit | strongserial | 65536 | 65536 | 1 | 0 |
| u32/top8bit | A1 | 65536 | 65536 | 1 | 0 |
| u32/top8bit | C1 | 65536 | 83976 | 3 | 0 |
| u32/top8bit | strongserial | 65536 | 65536 | 1 | 0 |
| u64/top4bit | A1 | 65536 | 65536 | 1 | 0 |
| u64/top4bit | C1 | 65536 | 66696 | 3 | 0 |
| u64/top4bit | strongserial | 65536 | 65536 | 1 | 0 |
| u64/top8bit | A1 | 65536 | 65536 | 1 | 0 |
| u64/top8bit | C1 | 65536 | 83976 | 3 | 0 |
| u64/top8bit | strongserial | 65536 | 65536 | 1 | 0 |
| u32/ascending_d256 | A1 | 0 | 0 | 0 | 0 |
| u32/ascending_d256 | C1 | 0 | 0 | 0 | 0 |
| u32/ascending_d256 | strongserial | 0 | 0 | 0 | 0 |
| u64/ascending_d256 | A1 | 0 | 0 | 0 | 0 |
| u64/ascending_d256 | C1 | 0 | 0 | 0 | 0 |
| u64/ascending_d256 | strongserial | 0 | 0 | 0 | 0 |
| u32/descending_d256 | A1 | 0 | 0 | 0 | 0 |
| u32/descending_d256 | C1 | 0 | 0 | 0 | 0 |
| u32/descending_d256 | strongserial | 0 | 0 | 0 | 0 |
| u64/descending_d256 | A1 | 0 | 0 | 0 | 0 |
| u64/descending_d256 | C1 | 0 | 0 | 0 | 0 |
| u64/descending_d256 | strongserial | 0 | 0 | 0 | 0 |
| u32/equal37 | A1 | 0 | 0 | 0 | 0 |
| u32/equal37 | C1 | 0 | 0 | 0 | 0 |
| u32/equal37 | strongserial | 0 | 0 | 0 | 0 |
| u64/equal37 | A1 | 0 | 0 | 0 | 0 |
| u64/equal37 | C1 | 0 | 0 | 0 | 0 |
| u64/equal37 | strongserial | 0 | 0 | 0 | 0 |

Shifted C adds 1,160 requested bytes at width4 or 18,440 at width8 without increasing the observed full-call peak of 65,536 bytes. No phase trace was taken, so equal peaks do not establish allocation lifetimes. The new kernel has no n-record scratch and retains its hard per-worker square-root storage gate. Ordered/equal A/C/strongserial all allocate zero heap in these observations. Stack, allocator metadata, RSS and peak per-thread memory were not measured.

## All tails and execution windows

| Input | Samples | Individual timed sorts | Individual sort ms range, all labels |
|---|---:|---:|---|
| u32/top4bit | 63 | 3833 | 0.367870–5.882086 |
| u32/top8bit | 63 | 4152 | 0.346104–18.140720 |
| u64/top4bit | 63 | 3298 | 0.391774–11.056021 |
| u64/top8bit | 63 | 3739 | 0.359903–7.711645 |
| u32/ascending_d256 | 63 | 40379 | 0.062244–5.628082 |
| u64/ascending_d256 | 63 | 19348 | 0.133732–3.305534 |
| u32/descending_d256 | 63 | 4098 | 0.702611–3.950442 |
| u64/descending_d256 | 63 | 3319 | 0.855191–6.666294 |
| u32/equal37 | 63 | 39788 | 0.064678–3.278004 |
| u64/equal37 | 63 | 17159 | 0.135785–9.353527 |

All per-label sample means, paired control vectors and individual durations are retained privately. The much larger repeat counts on fast ascending/equal paths are a consequence of the same 50 ms requirement; clone/reference work remains untimed, so wall time need not track sort time.

- Selective shared build: 2026-11-07 15:18:35–15:20:09 UTC, 94 seconds. The two new bins and two small isolated diagnostic binaries compiled. Reusing the frozen old control module leaves an unused input-generator warning; diagnostic fixture warnings are retained. No new full runtime safety-suite run is claimed; this screen reuses the unchanged paired source’s completed safety gates.
- Source/script/diagnostic freeze preceded all boundary sort validation and timing. No frozen source was edited afterward.

| Group | Shared validation window UTC | Exclusive timing window UTC | Cases |
|---|---|---|---:|
| shift | 2026-11-07T15:20:36Z–2026-11-07T15:20:37Z | 2026-11-07T15:21:00Z–2026-11-07T15:21:22Z | 4 |
| ascending | 2026-11-07T15:21:30Z–2026-11-07T15:21:31Z | 2026-11-07T15:21:43Z–2026-11-07T15:22:13Z | 2 |
| descending | 2026-11-07T15:22:19Z–2026-11-07T15:22:20Z | 2026-11-07T15:22:30Z–2026-11-07T15:22:42Z | 2 |
| equal | 2026-11-07T15:23:13Z–2026-11-07T15:23:14Z | 2026-11-07T15:23:23Z–2026-11-07T15:23:52Z | 2 |

The whole shell held the existing shared/exclusive FD9 lock, device 27/inode 1578019. Four shift cases used one window; each ordered/equal window held only two cases. The shift window had a fixed 120-second budget and the ordered/equal windows 180 seconds each, checked between completed cases. None hit the budget, regression, route, reference, retained-heap or disk stop. Each release was immediate before report analysis. No new targets, cleanup or reruns were used.

## Frozen identities and environment

AMD EPYC 9V74, process CPUs 0–7. Rust 1.99.1 (58cf8f9c3), Cargo 1.99.1, LLVM 23.1.1, Rayon 1.12.0, fearless_simd 1.2.0. Portable SIMD enabled and actual detected/selected AVX512 captured per process. Default release optimization plus debuginfo, no native/LTO/RUSTFLAGS override. TESSERA_* and relevant Rust/Rayon overrides are cleared and logged. CARGO_INCREMENTAL=0, offline -j1. A/C remain separate packages and C has no count-window-research feature. One existing target was reused; available disk remained over 360 MiB.

- Upstream core: e1d25297685cecb205d18114e898a39878e3fd83.
- A source identity: ab73401745af53e114015e4ba6cc6a87ef91a5cfe3a1daa73036049189050e2a.
- C source identity: 8f300a35a875c8949d80fc306ed2ec24ddc9fbe6747663d4768369610b641b35.
- Boundary source/script/diagnostic manifest SHA-256: 30ceff0462206494ef4f7fd9a266ef6bd56dff161a8142991a4a7019409d4be0.
- Timing binary SHA-256: 85339aeb487de585477da8d60f4c8065a5af03d7254f9224ccf085c6cf85a180.
- Allocation binary SHA-256: 3d21eb8e77a9bb5d718fc4156b88f66863c4a3e9844439620f022b70746a0f7d.
- Unchanged eight-cell Tier 2 report SHA-256: 4e42c57bb28e9e6682542f0c747fa8b0df9b48c2d1e127edf8312189653b8c0a.

Full raw data, instrumented-only diffs, source maps, logs, compiler metadata, diagnostics and binaries remain private. The publication packet is report, identities and checksums only. It contains neither harness nor raw timing data.

## Remaining limits

This checkpoint covers only ten Direct u32/u64 n1M/W8/seed1 boundary cells. It does not add positive Narrow/u128 adaptation, ByKey, Row16, owned records, other widths/shifts/seeds, a new public-entry port, further sizes or other hardware. Those remain unmeasured here. Broad controls leave the favorable-looking shifted cases unaccepted, and ordered/equal null results do not prove exact equivalence. No further experiment was started.
