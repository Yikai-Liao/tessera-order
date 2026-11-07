# Validation and negative first-stage result

## Exact scope

The primary experiment is a stable **MSD-root partition**, not a full-key sort.
The root oracle sorts by key>>56 and compares every complete record, including
stable IDs and wide payloads. The inherited full-sort API and its tests run
serial current-main children after a global root barrier. No child-overlap
implementation is included in this checkpoint.

The benchmark route is forced aligned stable-rank root, not a production router
fallback. The low-cardinality control uses 16 values spread over the full u64
range, populating 16 distinct root buckets. All four real plans reached and
completed histogram, stable rank, block scatter and the selected rearranger.

## Checks

Environment: stable Rust 1.99.1, x86_64 Linux, available CPUs 0–8, experiments
pinned to CPUs 0–7, eight Rayon workers. No target-cpu/RUSTFLAGS overrides.

Final diagnostic-off source:

- `cargo test --offline -j1 --release --lib`: 8 passed.
- `MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri test --offline -j1 --lib region_tests`: 2 passed under default Stacked Borrows. Same kernel, scoped std threads; not an exhaustive schedule exploration or a Rayon-internals proof.
- `cargo build --offline -j1 --release --bins`: passed in the external harness.
- `cargo +1.85 check --offline -j1 --lib`: passed. The unchanged main-core dependency emits its existing `unreachable_pub` warning on this compiler.

Coverage includes unique random and duplicate keys, all equal, sorted/reverse,
skew99, full-width 16-key distributions, lengths around block boundaries,
partial tails, non-power-of-two lengths and three workers, wide payloads,
sequential and actual parallel movement, root and full-sort stable references.
The source review separately establishes exact-token conservation, injective
ownership, disjoint wave endpoints and permanent prefix retirement.

## Real-plan falsification

The existing split-cycle scheduler was modeled using its actual plan, exact
job assignment and per-job block-copy counts. Snapshot preparation precedes
time zero; partial tail copying follows the join. This is an **ideal equal-speed
copy-clock model**, not elapsed CPU time.

| Input, n | Median nonempty bucket ready | First bucket ready | Paths / cycles | Largest component | Exact reciprocal pairs |
|---|---:|---:|---:|---:|---:|
| u64 random, 10M | 98.487% | 91.218% | 2165 / 1 | 266 | 0 |
| Row16 random, 10M | 99.193% | 93.129% | 2176 / 2 | 621 | 0 |
| Row128 random, 1M | 98.884% | 94.498% | 2116 / 2 | 120 | 0 |
| Row16 d16spread, 10M | 99.954% | 99.815% | 168 / 7 | 5868 | 0 |

No records belong to ready buckets at or before 90% of this clock in any case.
Merely adding last-write counters to the old cycle schedule does not expose a
useful broad early-child window on these plans. The reciprocal exchange fast
path has zero observed coverage; it is not being pursued as a general solution.

## Initial diagnostic-on pilot, not an accepted A/B result

Three randomized blocks, allocation included, exact same source inputs and
root geometry. A1/A2 are the original aligned split-cycle root. The broker
recorded one readiness clock per bucket (256 calls) plus an O(R) vector; the
control did not. That imbalance is disclosed, not silently treated as free.
The final source disables those clocks/vector unless `capture_plan` is true.
The initial source and all raw results are preserved outside Git. **Final
source has not yet been re-timed at this checkpoint.** No speedup is claimed.

| Input | A1 / A2 median root ms | Broker root ms | A1 / A2 restore ms | Broker restore ms | Median broker/root-control ratio | A2/A1 range |
|---|---:|---:|---:|---:|---:|---:|
| u64 random10M | 19.573 / 17.246 | 90.630 | 6.105 / 6.155 | 77.547 | 4.831 | 0.706–3.184 |
| Row16 random10M | 34.141 / 36.868 | 95.968 | 18.364 / 20.161 | 80.805 | 2.686 | 1.023–1.080 |
| Row128 random1M | 16.873 / 16.015 | 61.740 | 6.189 / 6.103 | 47.058 | 3.661 | 0.913–1.007 |
| Row16 d16spread10M | 28.167 / 33.214 | 49.265 | 11.273 / 20.255 | 36.549 | 1.610 | 0.982–1.331 |

These are root-mechanism controls, not comparisons against current public or
strongest combined full sorting. There is no placement control in this initial
pilot and only three blocks; the u64 A/A range is particularly wide. The values
are a reason to stop and investigate this transport, not an accepted general
performance conclusion.

| Input | Broker waves | Exchanges | Direct copies | Largest wave | Control / broker logical copied records |
|---|---:|---:|---:|---:|---:|
| u64 random10M | 520 | 78124 | 7485 | 307 | 10527488 / 28083456 |
| Row16 random10M | 537 | 156248 | 8368 | 614 | 10262592 / 28928512 |
| Row128 random1M | 427 | 31248 | 5617 | 128 | 1130304 / 2640320 |
| Row16 d16spread10M | 64 | 156243 | 1044 | 9189 | 10141824 / 29865024 |

## Allocator high water, final source

Separate allocation-only runs; no allocation-run durations are performance
results. Inputs, stable references and warmed Rayon pool are created before
the baseline count. Returned ends/stats are dropped before retained bytes are
read. All measured outputs passed the exact root oracle.

| Input | B | Root scratch records | Cycle peak extra bytes | Broker peak extra bytes | Retained delta, both |
|---|---:|---:|---:|---:|---:|
| u64 random10M | 128 | 522880 | 8069820 | 9391192 | 0 |
| Row16 random10M | 64 | 260096 | 11258012 | 14369424 | 0 |
| Row128 random1M | 32 | 129056 | 18307838 | 18751136 | 0 |

Broker metadata is larger despite fewer scratch snapshots because its pending
exchange descriptors reserve O(F) entries. This is counted, not hidden.
See README for the explicit `(2PR+P)B` payload and `O(n/B+PR)` metadata bounds.

## Next transport experiment

Most true dependencies are independent open paths, not cycles. The existing
control artificially fills unused physical slots to form a full permutation.
A next candidate can assign true paths by descending length to P jobs, pull each
path once from its initially dead destination, and use a private temporary only
for the few real cycles. It needs no bucket waves or long-cycle snapshots on
these captured cases. A metadata-only LPT model gives maximum job copy loads
9766 / 19532 / 3907 / 19539 for the four cases versus the original control's
11170 / 22109 / 4435 / 20521. This is a work/load model, not measured speed.
Adversarial long paths/cycles still need bounded splits or a documented fallback.

# Follow-on clean direct-path pilot

## Safety and exact geometry

- Default-feature optimized library tests: 13 passed.
- Strict-provenance/default-Stacked-Borrows Miri: 5 focused path tests passed,
  including P=1 direct and P=4 explicit fallback long paths/cycles, all-fixed,
  all-scratch, mixed cycles/paths/tail, rejected duplicate/tail-alias mappings,
  stable record IDs and wide payloads. Same mover with scoped std threads.
- Matched `portable-simd` binary build passed. Stable Rust 1.99.1.
- All TESSERA_* overrides unset. Runtime `fearless_simd::Level::new()` selected
  Avx512 on this machine. Short scalar peel/fallbacks remain normal behavior;
  this selector is not a claim that every operation used vector instructions.
- Real cycle/path source-target interval CSVs and complete bucket-end/plan
  summaries were byte-for-byte identical for every corpus before timing.
- B/P are unchanged: u64 B128/P8, Row16 B64/P8, Row128 B32/P8. Every measured
  path call asserts its explicit fallback flag is false.
- Timed movement has no per-bucket readiness clocks/vector, per-record tracing,
  interval capture or logging. Phase clocks are present for all root variants.
  Input cloning/reference generation and validation happen outside timing.

This is a three-block **direction screen**, not a nine-block acceptance run.
Public placement/A/A controls do not replace every algorithm's own layout
control. No accepted win or production adoption is claimed.

## Clean root-only screen

| Input | A1 / A2 cycle median ms | Broker ms | Direct paths ms | Path paired ratio | A2/A1 range |
|---|---:|---:|---:|---:|---:|
| u64 random10M | 16.060 / 14.266 | 66.550 | 14.058 | 0.8636 | 0.821–1.040 |
| Row16 random10M | 36.888 / 34.491 | 105.806 | 29.555 | 0.8435 | 0.935–0.957 |
| Row128 random1M | 14.825 / 18.684 | 70.689 | 14.270 | 0.8721 | 1.064–1.273 |
| Row16 d16spread10M | 22.865 / 30.647 | 40.491 | 23.332 | 0.8310 | 1.057–1.340 |

All full-record **root partition** oracles passed. Wide A/A variation and the
three-block sample limit prevent turning these medians into a broad speedup.
The diagnostic-off broker remains much slower; no overlap experiment follows.

| Input | Path restore median ms | Path copied records | Paths / nonfixed cycles / fixed | Largest component | Max job copies |
|---|---:|---:|---:|---:|---:|
| u64 random10M | 4.782 | 9999872 | 2165 / 0 / 1 | 266 | 9766 |
| Row16 random10M | 10.926 | 10000000 | 2176 / 1 / 1 | 621 | 19532 |
| Row128 random1M | 4.666 | 1000000 | 2116 / 1 / 1 | 120 | 3907 |
| Row16 d16spread10M | 11.026 | 10000448 | 168 / 7 / 0 | 5868 | 19539 |

## Complete stable full-sort screen

Same single binary, fixtures, seeds, CPUs0–7/P8, allocation-included calls,
matching SIMD feature, current serial child, no casts. u64 controls use Direct
APIs. Row16 controls share the exact record type and the same key extractor.
The full output oracle sorts by all 64 key bits and compares complete records.
There is no root-only oracle substitution in these samples.

Controls: current serial main0bc80b49d5efd5bd8b62165b170015e5803226b7;
public d6f303b74a0f19b6ae3c14a781539e3c39b9c4c0 and identical-source placement;
current combined 25-file identity
305068d9d6f4b1d0621c18426eee4945c8b45514f4b89037a276ed60fef37d70.
The subsequent main documentation-only commit does not change this runtime.

| 10M random records | Public A1 / A2 ms | Public placement ms | Strong serial ms | Current combined ms | Old aligned cycle ms | Direct paths ms |
|---|---:|---:|---:|---:|---:|---:|
| u64 | 39.076 / 39.827 | 34.341 | 85.860 | 24.445 | 29.476 | 25.739 |
| Row16 | 65.666 / 67.823 | 57.201 | 119.862 | 45.392 | 41.807 | 38.118 |

Paired median path/combined ratios: u64 **1.0725** (slower), Row16 **0.8277**.
Row16 beats combined in only 2/3 blocks and regresses in the first block.
Public A/A ranges are 0.997–1.227 (u64) and 0.984–1.369 (Row16); public placement
relative to the public pair ranges 0.843–0.999 and 0.857–1.003. Thus Row16 is a
signal worth checking, not an accepted end-to-end improvement. The u64 result
does not beat the strongest current implementation despite the root-only
reduction. No broader matrix, tuning or child-overlap layer is justified yet.

## Direct-path allocator measurement

Separate counting-allocator run, stable root oracle, all allocations counted,
no retained allocation after returned metadata is dropped:

| Input | Direct-path peak extra bytes | Retained delta | Direct temp records |
|---|---:|---:|---:|
| u64 random10M | 7151935 | 0 | 1024 |
| Row16 random10M | 9786852 | 0 | 512 |
| Row128 random1M | 17918069 | 0 | 256 |

The direct path still retains root scratch `(PR+Q)B`, directories, free pools,
histograms and cursors while restoring. Its lower snapshot/copy burden does
not make those allocations disappear. These are root-only heap peaks, not
unmeasured full-sort concurrent-child peaks.

External evidence is held outside the repository in the region-path research
packet: exact source/binary hashes, compiler/CPU/feature metadata, immutable
runner, complete raw samples including outliers, interval CSVs, memory results
and full reference checks. Exclusive window: 2026-11-07 12:47:52–12:48:10 UTC.

# Nine-block causal follow-up: do not promote

The Row16 three-block signal did not pass the predeclared follow-up gate.
This restoration policy is **not accepted for production or as an overall
performance improvement**. No more scheduling variants or overlap layers were
added after this result.

## Frozen setup

The kernel, radix, B, shards, features, inputs and source ownership are unchanged
from the clean screen. Two independent source-identical package copies isolate
placement for the candidate and current combined control. The .rs bytes match
exactly; only package/workspace metadata differ. Separate Row16 types are made
with typed constructors, never cast. Measured layouts for both are size16,
alignment8, key offset0, ID offset8.

Each case uses nine randomized blocks with six causal labels: candidate A1/A2,
candidate placement, combined C1/C2, combined placement. Ready old-cycle,
public-d6 and strong-serial implementations remain contextual labels. Every
label receives a warmup before measurement. The process is restricted to CPUs
0–7 and has eight Rayon workers, with **no individual worker pinning**, matching
the pilot. SIMD is on consistently; the runtime selector is Avx512. All
TESSERA_* algorithm overrides and target-codegen overrides are cleared.

Each sample prepares a fresh input on the actual executing pool worker before
the timer. Sort calls accumulate at least25ms, with at least two sorts. Actual
repetition counts and every individual duration are retained. All samples used
two sorts: 162 measured sorts per case, 324 in total. Minimum accumulated
sample times were40.077ms (u64) and70.123ms (Row16). Every complete output was
compared to the full stable reference outside timing; no samples or tails were
dropped. Original three-block data remain archived unchanged.

## Full-sort results, 10M random records

Values below are medians of per-sort times, not accumulated two-sort durations.

| Record | Path A1 / A2 ms | Path placement ms | Combined C1 / C2 ms | Combined placement ms | Old cycle ms | Public d6 ms | Strong serial ms |
|---|---:|---:|---:|---:|---:|---:|---:|
| u64 | 25.906 / 27.020 | 26.038 | 24.926 / 23.602 | 23.738 | 26.646 | 36.988 | 84.834 |
| Row16 | 39.250 / 39.477 | 40.691 | 44.958 / 44.727 | 45.518 | 40.756 | 64.661 | 119.341 |

Within each block, compare the mean of candidate A1/A2 with the mean of
combined C1/C2, then take the median of those block ratios:

- u64: **1.1278**, candidate slower in **8/9** blocks. Candidate pair versus
  combined placement is1.1334 and slower in9/9. This is a consistent unfavorable
  direction, not a slowdown magnitude established outside its own noise envelopes.
- Row16: **0.8818**, candidate faster in only **7/9** blocks, below the required
  8/9 agreement. Candidate-placement/combined-pair is0.8984, candidate-pair/
  combined-placement0.9178, and placement/placement0.9209. Each has only7/9
  faster blocks. The apparent median benefit remains inside measured controls.

Both algorithms have their own A/A and placement envelopes:

| Record | Path A2/A1 range | Path placement / path-pair range | Combined C2/C1 range | Combined placement / combined-pair range |
|---|---:|---:|---:|---:|
| u64 | 0.875–1.140 | 0.841–1.406 | 0.734–1.160 | 0.888–1.100 |
| Row16 | 0.811–1.257 | 0.937–1.541 | 0.910–1.026 | 0.897–1.107 |

Block-level primary path-pair/combined-pair ratios (blocks0–8, none removed):

- u64:1.1278,1.0427,1.2359,1.1990,1.2472,0.9661,1.0852,1.1349,1.0490. Only block5 favors the candidate.
- Row16:0.8761,0.8731,0.8110,0.9367,0.8781,0.8818,1.1384,0.9279,1.0376. Blocks6 and8 reverse the apparent benefit.

Individual-sort tails are retained. Candidate-placement maxima are41.763ms
(u64) and82.306ms (Row16); the u64 old-cycle context reaches119.668ms. None
was filtered as an outlier. The external aggregation contains every label's
min/max and all individual durations, not only the favorable median.

Public or serial controls were not substituted for these envelopes. The path
copy reduction is real, but these measurements do not establish an end-to-end
win over the strongest current implementation. The algorithm is left as a
validated research control, not promoted or hidden behind new routing.

## Actual full-sort heap high water

These are independent **complete-sort** counting-allocator runs, one call per
implementation/case after pool setup, not sums of capacities or root-only
figures. No instrumented duration is used as performance evidence. Inputs and
stable references are outside the measured allocation interval. Every full
output passed and every implementation retained zero additional bytes.

| 10M random records | Direct path peak bytes | Current combined peak bytes | Strong serial peak bytes | Retained delta, all |
|---|---:|---:|---:|---:|
| u64 | 7151935 | 6168395 | 2770254 | 0 |
| Row16 | 9786852 | 7971658 | 3902036 | 0 |

The path peak happens to equal its earlier root-only peak because the root
scratch is released before serial children start. This equality is measured,
not assumed. The candidate uses more peak memory than current combined, while
remaining within its documented O(P sqrt(n)) bound for fixed radix.

Formal exclusive CPU window: 2026-11-07 12:57:33–12:58:11 UTC. The external
formal evidence packet contains both complete81-sample case files, warmups,
individual-sort tails, exact binary/source hashes, source-equality manifest,
feature tree, actual layout dump, affinity and compiler metadata, immutable
runner, aggregation script and separate full-sort allocation results. No raw
samples, research harness, binary or profiles are committed here.
