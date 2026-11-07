# Current generic roots: measured four-cell screen

Frozen source/harness/parser/binary identity: bf3b2ea730f98fca2dfaf75f3baba732f7f5d67eaea8bafdbae17c684fe11bae. Raw data and per-case strict summaries are in evidence/final-*. All values below require complete reference verification and successful strict parsing. This is a current-baseline/profile study, not an algorithm candidate acceptance.

## Findings and limits

At W8, the native root remains on the critical path: squeeze ends at97.23–98.99% of u64 trace duration and98.61–98.97% of Row16 duration. Final child tails are0.20–0.79ms and0.23–0.41ms respectively. By contrast W4 u64 has about8ms of post-root child work. This supports investigating decomposition of root versus child work rather than merely adding consumers.

Row16 W8 consumers spend a median106.89ms of summed thread CPU inside empty-queue intervals versus79.75ms in child sorts. That is observed polling/wait overhead, not useful utilization or an isolated mutex/yield measurement. Per-run data in the JSON summaries preserve the worker distribution and root/child wall overlap.

The uninstrumented medians favor native combined on these inputs, but individual control tails are too wide for a robust performance acceptance: u64 W8 combined A/A reaches2.121 and Row16 W8 independent placement reaches3.017. No slow samples were removed. This bounded fixed-sort-count screen is separate from fresh-duration acceptance. u64 W8 trace/control1.217 versus off/control1.053 also shows meaningful tracer perturbation; no medians were subtracted to invent pure overhead.

Current public root geometry is B628 for u64/B444 for Row16 (R256/P1), unlike the older profile's B157/B111. This is a concrete reason not to reuse its absolute timing claims as current source. Native remains B128 atW4/W8 u64, B79 atW4/B64 atW8 Row16. Native permutation planning medians are0.60ms u64 and1.25/1.56ms Row16. Every root copies10M live records during final materialization; logical evacuation counts are9,922,400/9,942,048 public versus0 native. Native permutation copies10,265,344/10,529,024 u64W4/W8 and10,163,903/10,264,704 Row16W4/W8 records, including snapshots/padding. These are copy-call lengths, not measured memory traffic.

## Uninstrumented full-call controls

Each cell contains nine measured randomized complete blocks after one verified warmup. Values are medians, A/A and placement show all-block ranges. Strong serial calls the same package serial API, not fixed-decomposition W1.

| Cell | public ms | combined ms | public serial ms | combined serial ms | combined/public | public AA range | combined AA range | public placement range | combined placement range |
|---|---:|---:|---:|---:|---:|---|---|---|---|
| u64 W4 | 38.13 | 36.90 | 83.32 | 83.64 | 0.941 | 0.974–1.861 | 0.861–1.130 | 0.668–1.131 | 0.896–1.762 |
| row16 W4 | 60.46 | 51.56 | 117.96 | 120.28 | 0.818 | 0.830–1.176 | 0.962–1.356 | 0.885–1.257 | 0.971–1.168 |
| u64 W8 | 37.20 | 29.70 | 81.60 | 81.60 | 0.787 | 0.985–1.051 | 0.858–2.121 | 0.872–1.029 | 0.930–1.065 |
| row16 W8 | 63.98 | 49.37 | 116.19 | 120.28 | 0.785 | 0.822–1.041 | 0.808–1.061 | 0.895–1.072 | 0.813–3.017 |

## Diagnostic profiles

Three measured blocks after a warmup, with separate exact A/A and independent placement controls plus trace-on/off. Do not add phase medians or thread-CPU sums to reconstruct wall time. Root materialization end is a common-origin per-run fraction.

| Cell/design | trace wall ms | scatter ms | permutation ms | materialization ms | child CPU sum ms | empty-wait CPU sum ms | materialization end fractions | final child tails ms | trace/control median | off/control median |
|---|---:|---:|---:|---:|---:|---:|---|---|---:|---:|
| u64 W4 public | 31.44 | 13.19 | included | 16.95 | 52.57 | 0.00 | 95.19%, 95.27%, 86.97% | 1.47, 1.44, 4.57 | 0.958 | 1.029 |
| u64 W4 combined | 34.84 | 6.92 | 7.88 | 11.74 | 66.95 | 0.10 | 77.05%, 74.97%, 78.29% | 7.99, 8.60, 7.99 | 1.067 | 0.857 |
| row16 W4 public | 66.20 | 19.12 | included | 46.37 | 83.88 | 0.00 | 98.77%, 99.15%, 98.93% | 0.75, 0.52, 0.69 | 1.123 | 1.110 |
| row16 W4 combined | 55.60 | 10.98 | 15.01 | 28.74 | 86.71 | 0.28 | 87.23%, 99.30%, 94.15% | 6.68, 0.33, 3.33 | 0.978 | 1.016 |
| u64 W8 public | 36.70 | 13.19 | included | 22.95 | 77.24 | 0.00 | 98.62%, 98.99%, 91.82% | 0.43, 0.35, 2.57 | 1.030 | 0.989 |
| u64 W8 combined | 24.91 | 4.05 | 4.96 | 12.67 | 74.57 | 12.04 | 98.99%, 98.59%, 97.23% | 0.20, 0.26, 0.79 | 1.217 | 1.053 |
| row16 W8 public | 58.46 | 18.58 | included | 37.86 | 87.84 | 0.00 | 99.27%, 99.29%, 99.39% | 0.41, 0.33, 0.34 | 1.016 | 1.043 |
| row16 W8 combined | 51.02 | 6.55 | 16.11 | 26.91 | 79.75 | 106.89 | 98.61%, 98.97%, 98.95% | 0.41, 0.23, 0.25 | 1.013 | 1.080 |

## Interpretation boundaries

Public root scatter and materialization remain serial; its child jobs use materialized individual-bucket pointers and a reusable workspace pool. Combined has parallel native scatter and block permutation, then serial squeeze and fixed polling consumers. Empty-wait CPU includes try_recv/mutex/polling/yield/trace overhead, not pure yielding. Materialized-to-child-start is not enqueue delay. Per-worker useful-child CPU is separate from producer wall time. Copy counts are logical arguments to copy operations, not measured memory traffic. No PMU/cache/bandwidth claim is supported.

The separate real pre-permute directory and conservative retained-capacity ledger were supplied to the structural-design worker. Any candidate decision from that worker is a separate result and was not mixed into these source controls. Native safety/strict-Miri scope and current public test-module exclusions are documented in VALIDATION-SCOPE.md and evidence/source-validation-summary.md.

## Execution and coverage audit

Exclusive FD9 held17:48:13–17:49:45Z, released before descriptive analysis. All640 calls passed the complete reference,552 were measured; no matrix expansion or failed cells. Final frozen151-file identity was checked under the lock and again afterwards. execute-approved.sh, freeze.py and sourced env.sh were not in that pre-run manifest; their exact after-run hashes and wrapper comparison are explicitly supplemental evidence, not a retroactive freeze. The wrapper preserves the same eight calls/cells, labels and arguments while deferring descriptive analysis until after RELEASE.

The frozen parser validates complete labels/event invariants but does not itself enforce fixed n/block counts/control fingerprints. Separate post-run-header-checks.json independently verifies all8 exact10M/seed1/W4or8/9control-or3trace/warmup1 headers, fingerprint c2333e730de5dd2c and every12/10-label block. It also records the frozen repr(C) Row16 layout/size assertion and full-index reference. All32 traced calls entered the generic root and published/finished256 children, excluding the early-return root count path. Uninstrumented calls have no separate route counters; their path follows identical inputs and source controls.

Native --lib safety scope is49passed/1ignored plus3strict Miri filters. Preserved package-renamed integration test files import tessera_sort and need an alias adapter before they are runnable; they were not run. Public Job/Pool and sparse-root test modules are not part of this native guard set. The ignored native test is the opt-in10M geometry test. Thus this is not a full integration-suite or actual-Rayon-Miri claim.
