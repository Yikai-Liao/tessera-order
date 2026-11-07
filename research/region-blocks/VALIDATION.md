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
