# Fixed R16 versus R256 causal control

This is one existing root-cap/decomposition control at W8, not a new algorithm or a pure-radix ablation. Production sources are unchanged from the current-core screen. Each measured control label accumulated at least50ms of sort-call timers on fresh, reference-verified clones, using a warmed allocator and repeated fixed input. All call durations/repetition counts are retained.

## Result

Do not promote R16 as faster. The paired native median is3.0% slower for u64 and2.0% faster for Row16; independent placement gives2.7% slower and0.7% faster. These small differences do not separate from same-code/placement variation or contemporaneous public/serial drift. No additional radix was tested to find a winner.

The causal effect is clear in the traces: default materialization ends around99% of the call, while R16 ends around80–85% and leaves4.37–5.27ms u64 or6.44–8.96ms Row16 child tails. Root work and empty-wait CPU shrink, but child CPU rises from67.72to89.29ms for u64 and87.40to133.67ms for Row16 (separate median thread-CPU sums). This shifts the critical path rather than establishing a lower full-call time. The separate non-timed work audit below found the same partition/pass depth under both configurations, so extra passes do not explain the longer child work.

## Paired full-call results

| Type/label | default ms median | R16 ms median | paired R16/default median | range |
|---|---:|---:|---:|---|
| u64/combined | 24.44 | 25.00 | 1.030 | 0.797–1.148 |
| u64/combined_placement | 25.17 | 25.35 | 1.027 | 0.781–1.234 |
| u64/public | 36.58 | 36.46 | 0.997 | 0.931–1.073 |
| u64/public_placement | 36.91 | 37.22 | 0.997 | 0.832–1.118 |
| u64/public_serial | 81.36 | 82.78 | 0.994 | 0.923–1.092 |
| u64/combined_serial | 83.35 | 82.42 | 1.009 | 0.950–1.122 |
| row16/combined | 46.09 | 45.37 | 0.980 | 0.806–1.200 |
| row16/combined_placement | 47.92 | 45.27 | 0.993 | 0.826–1.022 |
| row16/public | 65.06 | 62.30 | 0.988 | 0.893–1.098 |
| row16/public_placement | 63.64 | 65.03 | 0.958 | 0.880–1.179 |
| row16/public_serial | 117.69 | 119.80 | 1.018 | 0.901–1.345 |
| row16/combined_serial | 120.53 | 120.38 | 0.994 | 0.930–1.143 |

## Within-config controls

| Type/config | native AA range | native placement range | public AA range | public placement range |
|---|---|---|---|---|
| u64/default | 0.892–1.073 | 0.866–1.063 | 0.822–1.124 | 0.928–1.062 |
| u64/r16 | 0.732–1.065 | 0.863–1.101 | 0.960–1.231 | 0.937–1.085 |
| row16/default | 0.744–1.161 | 0.905–1.194 | 0.900–1.249 | 0.848–1.085 |
| row16/r16 | 0.945–1.064 | 0.827–1.134 | 0.935–1.116 | 0.903–1.061 |

## Existing root traces

Each cell/config has only3measured traces with independent exact/placement/trace-off controls. Fields below are descriptive interval medians; do not sum them to reconstruct wall time. Full per-run overlap/worker CPU/geometry remains in summaries.

| Type/config | trace wall ms | scatter ms | permutation ms | squeeze ms | root-end fractions | final child tails ms | empty-wait CPU ms | trace/control median | off/control median |
|---|---:|---:|---:|---:|---|---|---:|---:|---:|
| u64/default | 27.64 | 5.77 | 4.80 | 15.51 | 98.83%, 99.16%, 99.12% | 0.23, 0.20, 0.18 | 37.15 | 1.105 | 1.137 |
| u64/r16 | 25.37 | 3.61 | 3.63 | 12.42 | 81.14%, 81.42%, 79.78% | 4.37, 5.27, 5.06 | 12.80 | 0.906 | 0.965 |
| row16/default | 45.75 | 7.06 | 10.60 | 27.25 | 99.24%, 99.45%, 98.96% | 0.29, 0.24, 0.37 | 87.48 | 0.956 | 0.882 |
| row16/r16 | 43.48 | 5.75 | 6.70 | 23.15 | 81.88%, 84.75%, 82.59% | 8.96, 6.44, 7.51 | 48.34 | 1.042 | 1.016 |

Full reference-verified calls across48fresh processes: 1280. Individual call durations and warmup/sample aggregates remain in raw JSONL.

Default ran first14of18control pairs and4of6trace pairs in the retained random draw. This was not counterbalanced; paired public and serial negative-control drift and the config order in every pair must be considered. No additional cap or distribution was tried. Existing root traces prove post-cap actual R/B and complete published-child coverage, not partition-depth/pass-work counts; the separately prepared depth diagnostic was not part of timing; a later non-timed follow-up is reported below. No allocator-peak or PMU/bandwidth claim is made.

## Order and execution audit

The under-lock run covered18:04:12–18:07:43Z and released before descriptive analysis. Final pre-run identity11296c9d5e7318a982ce115ff660138aa6c94fb801cc3b7fc0d8d3b43ba74aeb was verified before and after; it includes wrappers, parser, environment setup, exact schedule/seeds, validation evidence and both old/new binary identities. All six original source trees and the old bf3b2ea7 experiment remain unchanged. Raw result hashes are separately recorded after the run.

There were1,280complete verified calls:1,040control and240diagnostic. Of these,552were warmup and728measured. There are1,104sample aggregates. Every measured control aggregate met50ms; every constituent call's full-index oracle and timer is retained. All48process headers/config environments and strict geometry/route gates passed. There was no parser/reference failure, rerun, deletion or extra cap. New disk growth remained below20MiB and176MiB was free after analysis.

Order subgroups have only7default-first and2R16-first pairs per type, so they are descriptive, not independent estimates. They reinforce caution about claiming a small cap effect:

| Type/first config | pairs | native ratio median | placement ratio median | public negative ratio | native-serial negative ratio |
|---|---:|---:|---:|---:|---:|
| u64_default_first | 7 | 1.027 | 0.974 | 0.985 | 0.986 |
| u64_r16_first | 2 | 1.100 | 1.097 | 1.037 | 1.066 |
| row16_default_first | 7 | 0.980 | 0.993 | 1.003 | 1.034 |
| row16_r16_first | 2 | 1.024 | 0.974 | 0.930 | 0.975 |

The small child-work diagnostic copy was not part of timing. A later four-call non-timed audit is reported below. No actual allocator-peak measurement was warranted by this inconclusive performance signal.

## Existing per-run child-tail audit

Publication means fully materialized before enqueue. Last-materialized→last-child measures the remaining child completion after the final root bucket is published, distinct from squeeze-end→last-child. Worker counts/CPU are observed child work, not assumed pass counts.

| Type/config/block | child CPU sum ms | worker child counts (worker0..7) | final publication→last child ms | final publication→its own child ms | critical child is final bucket |
|---|---:|---|---:|---:|---|
| u64/default/0 | 67.72 | 0,32,35,46,36,34,40,33 | 0.24 | 0.23 | False |
| u64/r16/0 | 88.35 | 1,2,2,2,2,3,2,2 | 4.38 | 4.38 | True |
| row16/default/0 | 89.48 | 0,31,53,34,34,34,35,35 | 0.29 | 0.29 | True |
| row16/r16/0 | 175.13 | 1,3,2,3,1,2,2,2 | 8.96 | 8.96 | True |
| u64/default/1 | 62.18 | 0,35,44,26,36,40,40,35 | 0.20 | 0.19 | False |
| u64/r16/1 | 89.29 | 1,3,2,1,3,1,2,3 | 5.27 | 5.20 | False |
| row16/default/1 | 72.51 | 0,56,40,7,43,49,13,48 | 0.24 | 0.24 | True |
| row16/r16/1 | 133.67 | 0,2,2,2,2,3,3,2 | 6.45 | 6.45 | True |
| row16/r16/2 | 132.56 | 0,2,3,3,2,2,2,2 | 7.51 | 7.51 | True |
| row16/default/2 | 87.40 | 0,26,42,34,36,32,51,35 | 0.37 | 0.37 | True |
| u64/r16/2 | 93.04 | 1,2,2,2,3,2,2,2 | 5.07 | 4.19 | False |
| u64/default/2 | 69.75 | 0,34,35,51,37,38,13,48 | 0.18 | 0.18 | True |

## Separate non-timed actual-work audit

After the timing experiment, one full-reference-checked call per type/config ran from the separately prepared safe-TLS diagnostic copy. The timed sources/binaries were unchanged. No duration samples were emitted or used. Applied root widths8/default and4/R16, P8, expected B, full child coverage and zero successful parallel-count entries were explicitly checked.

| Type/config | child block calls | child block input records | tile MSD calls | tile MSD input records | LSD calls | deeper block/tile levels |
|---|---:|---:|---:|---:|---:|---|
| u64/default | 256 | 10000000 | 4096 | 10000000 | 0 | none |
| u64/r16 | 16 | 10000000 | 4096 | 10000000 | 0 | none |
| row16/default | 256 | 10000000 | 8192 | 10000000 | 0 | none |
| row16/r16 | 16 | 10000000 | 4096 | 10000000 | 0 | none |

The first serial child block partition is depth0 in these counters, separate from the native root. All four cases have exactly that one child block level and one tile-MSD counting-pass level, each processing10M input records. There are no extra recursive block/tile levels, no LSD passes and no histogram overflow. Therefore the R16 child-CPU/tail increase is not evidence of an extra full partition/pass level. Chunk sizes and fanouts change; cache/bandwidth/instruction explanations remain unmeasured. Record-work sums overlap across stages and are not physical traffic.

Diagnostic provenance identity: 522d3300856fe3c69d1e220bd21f568e7848e856a72ddb7a5697456378e6e7ac. Compiler rustc1.99.1 commit58cf8f9c35b355199727627d07ae34a4e2846d67, x86_64-unknown-linux-gnu. Exact cached dependency rlibs are hashed; features match the existing Cargo fingerprint: default, std, rayon, portable-simd. Direct rustc emitted only a diagnostic rlib/executable, with release optimization, unwind, debug assertions/overflow checks off. This is not a performance-binary equivalence claim. The shared audit ended18:15:17Z, with167MiB filesystem reserve and stage cumulative growth below26MiB. No further candidate or allocator study ran.
