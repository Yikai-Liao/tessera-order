# Continuous benchmarks

Affected pull requests compare the required SIMD backend against an exact source baseline on GitHub-hosted runners. Each case alternates adjacent A/B and B/A samples; local `compare commit` uses the same protocol. Bencher Cloud archives relative results, not historical regression thresholds. These screens flag changes to investigate, not source-change causality independent of code placement.

## Read feedback from a PR

Run from the repository root with Node.js 24, pinned dependencies and authenticated `gh`:

```sh
pnpm -s tools status main
pnpm -s tools status --pr 123 [--details]
```

1. Match `target.head` to the requested commit. Stale-head feedback is not current evidence.
2. Read `Benchmarks (rust)` in `benchmark`. Missing or incomplete measurements cannot pass; benchmark verdicts (`Measurement failed`, `no comparable baseline`, a slowdown alert) are independent of workflow success.
3. Read `workflows[].state`. The normal `CI` run builds, measures, attempts Cloud archival and publishes the report. An upload failure fails CI while the report keeps its independently computed verdict.
4. Use `--details` for the exact baseline and affected cases. Negative changes mean less time. Stacked PRs compare with their parent branch, not automatically with main.

A read-only feedback request does not authorize new measurements.

## Read main and history

The [dashboard](https://bencher.dev/perf/yikai-liao-s-project) stores current/baseline ratios, not absolute nanoseconds: 1.10 means 10% more time than that run's exact baseline. A PR's points form one trend even when merging main or a retarget changes the baseline, so a step can come from the baseline moving. Do not compare ratios across PRs.

Measurement and Cloud archival run on PRs only; main runs correctness and site checks selected by its diff. Retained older main ratios describe parent-to-commit effects, not cumulative latency, and no historical threshold decides the performance check.

## Pipeline

- The [CI workflow](../../.github/workflows/ci.yml) runs `prepare → measure → report` with the PR branch's code; the [diff router](ci.md) selects measurement for engine, harness, plan and execution-tooling changes. There is no second publisher or plan authority on main.
- **Prepare** calls `compare commit --plan continuous --prepare-only` once. Both revisions use the PR's harness, lockfile and toolchain; the baseline library comes from the merge base with the target SHA from the triggering event. It retains the plan, resolved inventory, provenance, hashes and executables as `prepared-revisions`.
- **Measure**: four complete-case shards call `compare prepared --shard INDEX/COUNT` without Rust or compiling, checking hashes, harness/catalog compatibility and both inventories first. Fourteen-day `paired-*` artifacts keep JSON, reports and logs; combine a shard's `local.json` binary paths with `prepared-revisions` to inspect binaries.
- **Report** reads this run's prepared plan and inventory, verifies complete disjoint shards and matching identity, and recomputes verdicts from raw samples without running artifact binaries. Cloud submission is attempted first so reports carry links; failure leaves the verdict, figures and summary intact.
- Same-repository PR report jobs use the `benchmark` environment and publish checks and Cloud ratios. Forks and Dependabot get artifacts and summaries without Cloud publication; a regression still fails their report job.

Both revisions enable Rayon and the runtime-dispatched SIMD backend, with no forced ISA or `target-cpu=native`. Linux workers run under `setarch -R`, which disables address randomization without host-wide changes; it does not control source-dependent code layout or physical page placement. The runner probes support before building and before executing a prepared pair and fails rather than measure an uncontrolled pair.

### Capacity and recovery

One preparation job and four measurement jobs cover the single backend; ordinary Rust CI keeps its own parallelism and no test job waits for benchmarks. PR updates cancel obsolete runs, and GitHub gives no global priority or preemption guarantee. Only preparation compiles, with `TESSERA_BENCH_TARGET_DIR` and `Swatinem/rust-cache` supplying the Cargo target and the shared CLI builder resolving alignment flags before compiling. The target is four minutes from triggering CI through archival; measure the complete critical path, since budgets, cache hits and timeouts are not completion evidence.

Rerun failed jobs in their original run. Missing or expired artifacts need a new complete run; an archival retry can reuse complete artifacts. A plan change is handled by that branch's preparation and report code in the same run.

## Within-job scheduling

The partitioner balances complete cases by nominal sampling budgets without shrinking coverage. Each job handles one case at a time, pinning serial measurements to one allowed CPU and parallel ones to exactly four (fewer available CPUs fail before a parallel build or measurement). Each sample pair uses two fresh processes, one per revision, with their own input and warmed pool; the coordinator alternates AB/BA order, only one process prepares, verifies or sorts at a time, and identical executable basenames keep the launch argument shape constant. Fresh workers make process-specific state vary across samples instead of treating one biased process pair as independent evidence. Cancellation kills and reaps workers; EOF or malformed responses fail the comparison.

Each sample restores bounded batches (cap 2^18 elements, at least one complete input), times only the sort calls and checks outputs against the stable reference. Copies, validation and IPC are outside the clock, and per-sort scratch allocation and entry into the caller-owned pool are inside. The wall budget includes restoration and verification; nanoseconds per sort divide timed duration by executed sorts. This is not interchangeable with Criterion's calibration.

## Cases

The [continuous plan](../../bench/plans/continuous.toml) caps inputs at 1,000,000 records and selects 97 cases: 61 serial and 36 four-thread. Parallel coverage:

- Random 1M inputs for every smoke record type: `u32`, `u64`, `u128`, `f64`, `row_u32`, `row_u64`, `rec128_u64`.
- Fourteen non-random `u64` distributions at 1M (ordered, nearly ordered, repeated-key, skewed, sparse-digit, clustered).
- Seven keyed-row distributions at 1M for `row_u64`, plus `row_u32/skew99`, to check stable payload order through fast paths and uneven buckets.
- `u128/sparse8`, `f64/finite_signed` and `rec128_u64/sparse_digits` at 1M.
- Random 65,536-record `u32`, `u64`, `row_u32` and `row_u64`, covering entry overhead and the serial fallback below the byte threshold.

The [catalog](../../bench/catalog.toml) owns the per-revision budgets, divided across fresh workers; startup, input preparation and at least one complete batch add overhead beyond them. There is no outer repetition.

| Length | Warm-up | Paired samples | Sampling budget per revision |
|---|---|---|---|
| Below 1,000,000 | 0.5 s | 30 | 1.5 s |
| 1,000,000 | 1 s | 40 | 4 s |

Preview coverage with `pnpm -s tools plan continuous`; preparation resolves the full inventory through Rust and report uses that retained inventory.

## Alerts

For each sample pair compute B/A. Report the median ratio and a distribution-free interval for the population median (exact binomial tails, at least 99% coverage under independent samples). A slowdown alert needs the whole interval above 1.05, an improvement below 0.95; anything else means no alert, not equivalence. `bench/statistics.ts` owns the statistic and no A/A pass is required. Adjacent pairs reduce drift and AB/BA alternation balances order effects, but neither removes preemption, correlated samples, cache effects or code-placement bias, and coverage is per case, not family-wide. Unexpected results need investigation, not automatic source attribution ([placement evidence](../design/evidence.md#reading-small-case-results)).

Cloud uses Testbed `ci` and Measure `paired-time-ratio`, which identify configuration and protocol, not PRs or hardware. Submissions use Bencher branches `pr-N` with hashes identifying heads; BMF carries the median and interval endpoints, and raw pairs stay in the Actions evidence. Configure no history-based thresholds on these Testbeds.

## Reports

The `Benchmarks (rust)` report states the baseline, compared-case count and overall run-time change (with an explicit result when there are no alerts), then changed-case summaries and tables, a focused comparison figure, a compact overview when nothing alerts, a PR trend figure and the complete case table. Ratios display as ×, never nanoseconds. A missing measurement fails its performance check independently; image or Cloud failures do not erase valid numbers. The report job maintains the `benchmark-images` branch for figures.

Each PR submits to its own `pr-N` branch with no Start Point or Head reset, so merging main or rebasing does not restart a trend. The report reads every run on that branch with the current Testbed and hardware, plots each ratio against one over a fixed case cohort and names the latest baseline. Cloud links select the current Head; Cloud failure leaves the measured point visible.

## Setup

Keep `BENCHER_PROJECT`, `BENCHER_API_KEY` and the `benchmark` environment, whose deployment branch policy allows `refs/pull/*/merge`. Only the report job gets credentials and write permissions, which explicitly trusts same-repository PR code with publication; fork PRs get neither. Artifact provenance is self-reported and cannot prove honest measurement by malicious PR code.

## Local use

`pnpm tools compare commit BASE --plan continuous` runs the same protocol (optionally one shard or a smaller plan); there is no default nine-round local run. To reuse builds:

```sh
# Build once; every result directory must be new.
pnpm tools compare commit main --plan continuous --prepare-only --out bench/results/prepared
pnpm tools compare prepared bench/results/prepared --shard 1/4 --out bench/results/paired-1
pnpm tools report bench/results/paired-1    # regenerate offline
```

A prepared pair holds the plan, provenance and executable hashes but no samples, so it is not evidence. `compare prepared` requires the original harness and catalog, rejects modified or escaping executable paths, verifies both listings and retains the executed copies. `--shard INDEX/COUNT` partitions the resolved listing deterministically, keeping each case's full sampling budget; omit it to run everything on one host. The four-thread plan needs Linux and `taskset` with four allowed CPUs, and serial and parallel cases never overlap. Interrupted measurement is not resumable. Builds reuse the checkout-local cache by default; `TESSERA_BENCH_TARGET_DIR` selects another caller-owned Cargo cache. Never share it across trust boundaries. Older protocols and absolute-time histories use their original tooling revision.

Upload is explicit and never executes artifact code. With the pinned Bencher CLI, the `prepared-revisions` artifact in `PREPARED`, every `paired-*` artifact in separate directories under `ARTIFACTS` and `BENCHER_API_KEY` in the environment:

```sh
pnpm tools continuous submit --project PROJECT --hash COMMIT --base-hash BASE_COMMIT --pr 123 \
  --prepared PREPARED --artifacts ARTIFACTS --out bench/results/submission
```

It validates the prepared plan and inventory, disjoint shard coverage, the revision pair, common settings and complete positive sample pairs, then uploads before rendering so reports carry links. It retains `bmf.json` and the Cloud response `bencher.json` and exits nonzero on validation or upload failure; raw-sample verdicts are unaffected. Omitting `--pr` keeps legacy manual main upload. The evidence limits are in [benchmarking](benchmarking.md#working-tree-comparisons).
