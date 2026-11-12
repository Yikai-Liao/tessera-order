# Continuous benchmarks

Affected pull requests compare their code with an exact source baseline on GitHub-hosted runners, alternating adjacent A/B and B/A samples per case; local `compare commit` uses the same protocol. Bencher Cloud archives the PRs' relative results; no historical threshold decides a verdict. Independent [main tracking](#main-long-term-tracking) records absolute time. These screens flag changes to investigate; they do not separate a source change from code placement.

## Read feedback from a PR

Run from the repository root with installed dependencies and an authenticated `gh`:

```sh
bun run --silent tools status main
bun run --silent tools status --pr 123 [--details]
```

1. Match `target.head` to the requested commit. Feedback for another head is not current evidence.
2. Read `Benchmarks (rust)` in `benchmark`. Missing or incomplete measurements cannot pass. Benchmark verdicts (`Measurement failed`, `no comparable baseline`, a slowdown alert) are independent of workflow success.
3. Read `workflows[].state`. The `CI` run builds, measures, attempts Cloud archival and publishes the report. A failed upload fails CI; the report keeps its own verdict.
4. Use `--details` for the exact baseline and affected cases. Negative changes mean less time. Stacked PRs compare with their parent branch, not with main.

A read-only feedback request does not authorize new measurements.

## Read main and history

The PR series on the [dashboard](https://bencher.dev/perf/yikai-liao-s-project) store current/baseline ratios, not nanoseconds: 1.10 means 10% more time than that run's baseline. A PR's points form one trend even when merging main or retargeting moves the baseline, so a step can come from the baseline. Do not compare ratios across PRs.

Paired measurement runs on PRs only. Main runs the correctness and site checks its diff selects, plus opt-in main tracking. Without a benchmark check, `status main` returns `benchmark.state: "not_applicable"`; an unmeasured PR or other ref returns `"missing"`. Older main ratios describe parent-to-commit effects, not cumulative latency.

## Pipeline

The [CI workflow](../../.github/workflows/ci.yml) runs `prepare → measure → report` with the PR branch's code when the [diff router](ci.md) selects measurement (engine, harness, plan or execution-tooling changes).

- **Prepare** runs `continuous prepare` once. Both revisions use the PR's harness, lockfile and toolchain; the baseline library comes from the merge base with the event's target SHA. It uploads the plan, resolved inventory, provenance, hashes and executables as an attempt-named `prepared-revisions-*` artifact and publishes its immutable ID. `preparation.log` keeps build diagnostics, also on failure.
- **Measure** runs the complete-case shards that [`CONTINUOUS_SHARDS`](../../bench/continuous/paired.ts) defines. Each downloads that exact artifact and runs `continuous measure --shard INDEX/COUNT` without compiling, after checking hashes, harness/catalog compatibility and both inventories. Fourteen-day `paired-*` artifacts keep JSON, reports and logs, named with the preparation ID, so a partial rerun cannot mix builds. A shard's `local.json` paths locate its binaries inside the prepared artifact.
- **Report** reads this run's prepared plan and inventory, verifies complete disjoint shards with matching identity, and recomputes verdicts from raw samples without running binaries. Cloud submission comes first so the report carries links; its failure leaves verdict, figures and summary intact.
- Same-repository PRs report through the `benchmark` environment and publish checks and Cloud ratios. Forks and Dependabot get artifacts and summaries without Cloud publication; a regression still fails their report job.

Both revisions enable Rayon and the runtime-dispatched SIMD backend, with no forced ISA or `target-cpu=native`. Linux workers run under `setarch -R`, which disables address randomization without host changes but does not control code layout or physical page placement. The runner checks this support before building and before measuring, and fails rather than measure an uncontrolled pair.

### Capacity and recovery

Only preparation compiles, with `TESSERA_BENCH_TARGET_DIR` and `Swatinem/rust-cache` supplying the Cargo target. Ordinary Rust CI keeps its own parallelism and never waits for benchmarks. PR updates cancel obsolete runs; GitHub offers no priority or preemption guarantee. The target is four minutes from trigger to archival, judged on the measured critical path rather than budgets or cache hits.

Rerun failed jobs in their original run. Missing or expired artifacts need a new complete run; an archival retry can reuse complete artifacts. A plan change is handled by the branch's own preparation and report code.

## Within-job scheduling

The partitioner balances complete cases by nominal sampling budget without shrinking coverage. A job measures one case at a time, pinning serial cases to one allowed CPU and parallel cases to exactly four (fewer fail before building). Each sample pair uses two fresh processes, one per revision, each with its own input and warmed pool. The coordinator alternates AB/BA order, lets only one process prepare, verify or sort at a time, and keeps identical executable basenames so launch arguments match. Fresh workers vary process-specific state across samples instead of trusting one process pair. Cancellation kills and reaps workers; EOF or a malformed response fails the comparison.

Each sample restores bounded batches (at most 2^18 elements, at least one complete input), times only the sort calls and checks outputs against the stable reference. Copies, validation and IPC are outside the clock; per-sort scratch allocation and entry into the pool are inside. Nanoseconds per sort divide timed duration by executed sorts. This is not Criterion's calibration.

## Cases

The [continuous plan](../../bench/plans/continuous.toml) caps inputs at 1,000,000 records and mixes serial and four-thread cases; preview it with `bun run --silent tools plan continuous`. Parallel coverage:

- Random 1M inputs for every smoke record type: `u32`, `u64`, `u128`, `f64`, `row_u32`, `row_u64`, `rec128_u64`.
- Fourteen non-random `u64` distributions at 1M (ordered, nearly ordered, repeated-key, skewed, sparse-digit, clustered).
- Seven keyed-row distributions at 1M for `row_u64`, plus `row_u32/skew99`, for stable payload order through fast paths and uneven buckets.
- `u128/sparse8`, `f64/finite_signed` and `rec128_u64/sparse_digits` at 1M.
- Random 65,536-record `u32`, `u64`, `row_u32` and `row_u64`, for entry overhead and the serial fallback below the byte threshold.

Larger lengths are explicit local plan overrides; historical reports keep their original inventories. The [catalog](../../bench/catalog.toml) owns per-revision budgets, divided across fresh workers; startup, input preparation and at least one complete batch add to them.

| Length | Warm-up | Paired samples | Sampling budget per revision |
|---|---|---|---|
| Below 1,000,000 | 0.5 s | 30 | 1.5 s |
| 1,000,000 or more | 1 s | 40 | 4 s |

## Alerts

For each sample pair compute B/A. The report gives the median ratio and a distribution-free interval for the population median (exact binomial tails, at least 99% coverage under independent samples); `bench/statistics.ts` owns the statistic. A slowdown alert needs the whole interval above 1.05, an improvement below 0.95; anything else is no alert, not equivalence. No A/A pass is required. Adjacent pairs reduce drift and alternation balances order effects, but neither removes preemption, correlated samples, cache effects or placement bias, and coverage is per case, not family-wide. Investigate unexpected results instead of attributing them to the source ([placement evidence](../design/evidence.md#reading-small-case-results)).

Cloud stores PR ratios under Measure `paired-time-ratio` on Bencher branches `pr-N`, with hashes identifying heads. BMF carries the median and interval endpoints; raw pairs stay in the Actions evidence. Configure no history-based thresholds.

### Testbed identity

Use the existing, user-named Testbeds: `ci` for PR comparisons and `main` for main tracking. Testbed names come from the user: agents must not invent, create, rename, replace or migrate Testbeds, or change references to them in code, configuration or documentation, unless the user explicitly asks and supplies the exact name. Compiler, flag, hardware, protocol, input or sampling changes do not authorize a Testbed change; assess their measurement compatibility separately.

## Reports

The `Benchmarks (rust)` report states the baseline, compared-case count and overall run-time change (with an explicit result when nothing alerts), then changed-case summaries and tables, a focused comparison figure, a compact overview when nothing alerts, a PR trend figure and the complete case table. Ratios display as ×, never nanoseconds. A missing measurement fails its check on its own; image or Cloud failures do not erase valid numbers. The report job maintains the `benchmark-images` branch for figures.

Each PR submits to its own `pr-N` branch with no Start Point or Head reset, so merging main or rebasing does not restart its trend. The trend reads every run on that branch with the current Testbed and hardware, plots each ratio against one over a fixed case cohort and names the latest baseline.

## Setup

Keep `BENCHER_PROJECT`, `BENCHER_API_KEY` and the `benchmark` environment, whose deployment branch policy allows `refs/pull/*/merge`. Only the report job gets credentials and write permissions, which trusts same-repository PR code with publication; fork PRs get neither. Artifact provenance is self-reported and cannot prove honest measurement by malicious PR code.

## Local use

`bun run tools compare commit BASE --plan continuous` runs the same protocol, optionally on a smaller selection. To reuse builds, prepare and shard separately:

```sh
# Build once; every result directory must be new. Use --plan local for a tiny selection.
bun run tools continuous prepare main --out bench/results/prepared
bun run tools continuous measure bench/results/prepared --shard 1/4 --out bench/results/paired-1
bun run tools report bench/results/paired-1    # regenerate offline
```

A prepared pair holds plan, provenance and executable hashes but no samples, so it is not evidence. `continuous measure` requires the original harness and catalog, rejects modified or escaping executable paths, verifies both listings and keeps the executed copies. `--shard INDEX/COUNT` partitions the listing deterministically with each case's full budget; omit it to run everything on one host. Four-thread cases need Linux and `taskset` with four allowed CPUs; serial and parallel cases never overlap. Interrupted measurement is not resumable. `TESSERA_BENCH_TARGET_DIR` selects a caller-owned Cargo cache; never share it across trust boundaries. Older protocols use their original tooling revision.

Upload is explicit and never executes artifact code. With the pinned Bencher CLI, the `prepared-revisions` artifact in `PREPARED`, each `paired-*` artifact in its own directory under `ARTIFACTS`, and `BENCHER_API_KEY` set:

```sh
bun run tools continuous submit --project PROJECT --hash COMMIT --base-hash BASE_COMMIT --pr 123 \
  --prepared PREPARED --artifacts ARTIFACTS --out bench/results/submission
```

It validates the prepared plan and inventory, disjoint shard coverage, the revision pair, common settings and complete positive sample pairs, uploads, then renders. It keeps `bmf.json` and the Cloud response `bencher.json` and exits nonzero on validation or upload failure; raw-sample verdicts are unaffected. Without `--pr` it performs the legacy manual main upload.

## Main long-term tracking

The [performance page](../benchmarks/index.md) tracks absolute nanoseconds per sort on main. The [main workflow](../../.github/workflows/main-benchmarks.yml) reuses the diff router: a relevant push measures its exact head; documentation-only changes do not. Push execution is opt-in through `MAIN_BENCHMARKS_ENABLED=true`; manual dispatch is available on main. Pending pushes are not coalesced; Bencher owns the single remote execution slot. Main tracking is not a merge requirement and sets no alerts.

[The plan](../../bench/plans/main.toml) owns fixed-length serial and four-thread coverage on the smoke Criterion harness and catalog budgets; preview it with `bun run --silent tools plan main`. CI builds one static glibc executable with the shared builder; the Alpine runtime image contains no Rust, Bun, sources or credentials, and remote execution never compiles. Serial and parallel phases require CPU pinning and disabled ASLR and fail without them. One remote job runs with `--job-timeout 300`, excluding compilation and queueing; the sampling budget does not guarantee end-to-end duration. Cancelling Actions need not cancel a submitted job.

### Data and site publication

Main tracking uses Bencher branch `main`, Spec `intel-v1`, Measure `latency` and the `main` [Testbed](#testbed-identity). The workflow accepts only complete matching reports, then updates the orphan `benchmark-data` branch with ordinary Git:

- `main/YYYY-MM.jsonl` holds one month of numeric summaries, partitioned by UTC benchmark start time (`time`), with source commit, compiler and flags, image digest and Actions links; no raw samples, API responses or binaries. The commit identifies the lockfile and the digest the executable and plan. New records add `commit_time`, the source commit's UTC committer time (`git show -s --format=%cI`); the field is optional, so old journals and readers stay compatible.
- `main/index.json` lists months, the cases in display order with their catalog mode group, labels, record type and length, and each filter's values in catalog order, since the browser cannot read the catalog. A catalog change reaches the site at the next publication. A format change must keep the deployed reader working: publish the new index first.
- `main/preview.svg` plots the group means of the main trend; the README embeds it (GitHub may cache it).

Re-publishing a report leaves the journals unchanged and regenerates the index and preview; a conflicting identity fails. Non-force pushes retry ref races without remeasuring. Full clones include the data branch's history.

The page reads the index from GitHub Raw, downloads each month overlapping the selected range once and renders locally; the [performance page](../benchmarks/index.md#main-performance-history) owns axis, baseline, rerun and zoom semantics. Requests revalidate cached files; observations are append-only, and a cached index may briefly omit a new month. **Publishing measurements does not build or deploy the website**, and site builds need no data or network. Agents can inspect the data with `git fetch origin benchmark-data` and `git ls-tree -r FETCH_HEAD main/`.

Compiler and dependency upgrades stay in the trend; protocol or input changes need a compatibility decision. Hardware specs and protocols are separate series, not one guaranteed machine. Criterion intervals are within-run estimates, not cross-run regression confidence.

### Setup and recovery

Main tracking reuses the `benchmark` environment, whose deployment branch policy must also allow `main`. Keep automatic execution disabled until a manually dispatched Bare Metal pilot confirms execution controls, complete ingestion, timeout headroom, Git publication and browser loading.

Rerunning the workflow measures again. If only publication failed, download the retained summary, check out the data branch, run `bun /path/to/source/bench/tracking/publish.ts /path/to/summary.jsonl`, and commit and push normally; running it with an already published summary regenerates the index and preview. Fourteen-day Actions evidence is diagnostic, not the long-term store.
