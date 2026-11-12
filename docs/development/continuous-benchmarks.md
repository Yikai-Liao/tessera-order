# Continuous benchmarks

Affected pull requests compare the required SIMD backend against an exact source baseline on GitHub-hosted runners. Each case alternates adjacent A/B and B/A samples; local `compare commit` uses the same protocol. Bencher Cloud archives the PRs’ relative results, not historical regression thresholds. Independent [main tracking](#main-long-term-tracking) records absolute time. These screens flag changes to investigate, not source-change causality independent of code placement.

## Read feedback from a PR

Run from the repository root with the pinned Bun toolchain, installed dependencies and authenticated `gh`:

```sh
bun run --silent tools status main
bun run --silent tools status --pr 123 [--details]
```

1. Match `target.head` to the requested commit. Stale-head feedback is not current evidence.
2. Read `Benchmarks (rust)` in `benchmark`. Missing or incomplete measurements cannot pass; benchmark verdicts (`Measurement failed`, `no comparable baseline`, a slowdown alert) are independent of workflow success.
3. Read `workflows[].state`. The normal `CI` run builds, measures, attempts Cloud archival and publishes the report. An upload failure fails CI while the report keeps its independently computed verdict.
4. Use `--details` for the exact baseline and affected cases. Negative changes mean less time. Stacked PRs compare with their parent branch, not automatically with main.

A read-only feedback request does not authorize new measurements.

## Read main and history

The PR series on the [dashboard](https://bencher.dev/perf/yikai-liao-s-project) store current/baseline ratios, not absolute nanoseconds: 1.10 means 10% more time than that run's exact baseline. A PR's points form one trend even when merging main or a retarget changes the baseline, so a step can come from the baseline moving. Do not compare ratios across PRs.

Paired measurement and ratio archival run on PRs only; main runs correctness and site checks selected by its diff, plus the opt-in independent main tracking workflow. With no attached benchmark check, `status main` returns `benchmark.state: "not_applicable"`; an unmeasured PR or another ref still returns `"missing"`. Retained older main ratios describe parent-to-commit effects, not cumulative latency, and no historical threshold decides the performance check.

## Pipeline

- The [CI workflow](../../.github/workflows/ci.yml) runs `prepare → measure → report` with the PR branch's code; the [diff router](ci.md) selects measurement for engine, harness, plan and execution-tooling changes. Main tracking does not publish PR verdicts or interpret the continuous plan.
- **Prepare** calls `continuous prepare` once. Both revisions use the PR's harness, lockfile and toolchain; the baseline library comes from the merge base with the target SHA from the triggering event. It retains the plan, resolved inventory, provenance, hashes and executables as `prepared-revisions`.
- **Measure**: four complete-case shards call `continuous measure --shard INDEX/COUNT` without Rust or compiling, checking hashes, harness/catalog compatibility and both inventories first. Fourteen-day `paired-*` artifacts keep JSON, reports and logs; combine a shard's `local.json` binary paths with `prepared-revisions` to inspect binaries.
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

The former staging-specific 4M cases were removed after final-band completion replaced that eligibility-gated route. Larger lengths remain available as explicit local plan overrides; historical reports and figures retain their original case inventories.

The [catalog](../../bench/catalog.toml) owns the per-revision budgets, divided across fresh workers; startup, input preparation and at least one complete batch add overhead beyond them. There is no outer repetition.

| Length | Warm-up | Paired samples | Sampling budget per revision |
|---|---|---|---|
| Below 1,000,000 | 0.5 s | 30 | 1.5 s |
| 1,000,000 or more | 1 s | 40 | 4 s |

Preview coverage with `bun run --silent tools plan continuous`; preparation resolves the full inventory through Rust and report uses that retained inventory.

## Alerts

For each sample pair compute B/A. Report the median ratio and a distribution-free interval for the population median (exact binomial tails, at least 99% coverage under independent samples). A slowdown alert needs the whole interval above 1.05, an improvement below 0.95; anything else means no alert, not equivalence. `bench/statistics.ts` owns the statistic and no A/A pass is required. Adjacent pairs reduce drift and AB/BA alternation balances order effects, but neither removes preemption, correlated samples, cache effects or code-placement bias, and coverage is per case, not family-wide. Unexpected results need investigation, not automatic source attribution ([placement evidence](../design/evidence.md#reading-small-case-results)).

Cloud stores PR ratios under Measure `paired-time-ratio`. Submissions use Bencher branches `pr-N` with hashes identifying heads; BMF carries the median and interval endpoints, and raw pairs stay in the Actions evidence. Configure no history-based thresholds.

### Testbed identity

Use the existing, user-named Testbeds: `ci` for PR comparisons and `main` for main tracking. Preserve them by default. Testbed names must come from the user; agents must not invent names or create, rename, replace or migrate Testbeds, including changing their references in code, configuration or documentation, unless the user explicitly requests the change and supplies the exact name. Changes to the compiler, flags, hardware, protocol, inputs or sampling do not authorize a Testbed change. Assess measurement compatibility separately; do not encode each change in a new Testbed name.

## Reports

The `Benchmarks (rust)` report states the baseline, compared-case count and overall run-time change (with an explicit result when there are no alerts), then changed-case summaries and tables, a focused comparison figure, a compact overview when nothing alerts, a PR trend figure and the complete case table. Ratios display as ×, never nanoseconds. A missing measurement fails its performance check independently; image or Cloud failures do not erase valid numbers. The report job maintains the `benchmark-images` branch for figures.

Each PR submits to its own `pr-N` branch with no Start Point or Head reset, so merging main or rebasing does not restart a trend. The report reads every run on that branch with the current Testbed and hardware, plots each ratio against one over a fixed case cohort and names the latest baseline. Cloud links select the current Head; Cloud failure leaves the measured point visible.

## Setup

Keep `BENCHER_PROJECT`, `BENCHER_API_KEY` and the `benchmark` environment, whose deployment branch policy allows `refs/pull/*/merge`. Only the report job gets credentials and write permissions, which explicitly trusts same-repository PR code with publication; fork PRs get neither. Artifact provenance is self-reported and cannot prove honest measurement by malicious PR code.

## Local use

`bun run tools compare commit BASE --plan continuous` runs the same protocol (optionally a smaller selection); there is no default nine-round local run. To reuse builds, separate preparation and sharding under `continuous`:

```sh
# Build once; every result directory must be new. Use --plan local for a tiny selection.
bun run tools continuous prepare main --out bench/results/prepared
bun run tools continuous measure bench/results/prepared --shard 1/4 --out bench/results/paired-1
bun run tools report bench/results/paired-1    # regenerate offline
```

A prepared pair holds the plan, provenance and executable hashes but no samples, so it is not evidence. `continuous measure` requires the original harness and catalog, rejects modified or escaping executable paths, verifies both listings and retains the executed copies. `--shard INDEX/COUNT` partitions the resolved listing deterministically, keeping each case's full sampling budget; omit it to run everything on one host. The four-thread plan needs Linux and `taskset` with four allowed CPUs, and serial and parallel cases never overlap. Interrupted measurement is not resumable. Builds reuse the checkout-local cache by default; `TESSERA_BENCH_TARGET_DIR` selects another caller-owned Cargo cache. Never share it across trust boundaries. Older protocols and absolute-time histories use their original tooling revision.

Upload is explicit and never executes artifact code. With the pinned Bencher CLI, the `prepared-revisions` artifact in `PREPARED`, every `paired-*` artifact in separate directories under `ARTIFACTS` and `BENCHER_API_KEY` in the environment:

```sh
bun run tools continuous submit --project PROJECT --hash COMMIT --base-hash BASE_COMMIT --pr 123 \
  --prepared PREPARED --artifacts ARTIFACTS --out bench/results/submission
```

It validates the prepared plan and inventory, disjoint shard coverage, the revision pair, common settings and complete positive sample pairs, then uploads before rendering so reports carry links. It retains `bmf.json` and the Cloud response `bencher.json` and exits nonzero on validation or upload failure; raw-sample verdicts are unaffected. Omitting `--pr` keeps legacy manual main upload. The evidence limits are in [benchmarking](benchmarking.md#working-tree-comparisons).

## Main long-term tracking

The [performance page](../benchmarks/index.md) tracks absolute nanoseconds per sort, independently of PR ratios. The [main workflow](../../.github/workflows/main-benchmarks.yml) reuses the diff router: relevant main pushes measure their exact head; documentation-only changes and unchanged main do not measure. Manual dispatch is available on main. Push execution is opt-in through `MAIN_BENCHMARKS_ENABLED=true`. Pending pushes are not coalesced; Bencher owns the single remote execution slot. This is not a merge requirement and sets no historical alerts.

[The plan](../../bench/plans/main.toml) owns fixed-length serial and four-thread coverage, reusing the smoke Criterion harness and catalog budgets. Preview with `bun run --silent tools plan main`. CI reuses the shared workspace/builder to create one static glibc executable; the Alpine runtime contains no Rust, Bun, sources or credentials. Remote execution never compiles. Serial and parallel phases require CPU pinning and disabled ASLR, failing if unavailable. There is one remote job, no shards, with `--job-timeout 300`; compilation and queue wait are outside that budget. The nominal sampling budget is not an end-to-end duration guarantee. Canceling Actions need not cancel an already submitted job.

### Data and site publication

Main tracking uses Bencher branch `main`, Spec `intel-v1` and Measure `latency`, with the existing [Testbed](#testbed-identity). The workflow accepts only complete matching reports, then uses ordinary Git to update the orphan `benchmark-data` branch. Each `YYYY-MM.jsonl` is partitioned by UTC benchmark start time and contains numeric summaries, source commit, compiler/flags, image digest and Actions links—not raw samples, full API responses or binaries. New reports also record `commit_time`, the exact source commit's Git committer timestamp (`git show -s --format=%cI`), normalized to UTC; existing `time` remains the benchmark start time. This optional field keeps old journals and deployed readers compatible. Chart ordering uses commit time, with measurement time as a visibly identified fallback for older records; measurement-date filtering and monthly storage still use `time`. The [performance page](../benchmarks/index.md#main-performance-history) owns axis, baseline, rerun and zoom semantics. The source commit identifies the lockfile; the image digest identifies the executable and plan, so separate content hashes are unnecessary. A small `main/index.json` lists months, the cases in display order with their catalog-generated mode group, labels, record type and length, and each filterable field's values in catalog order, because the browser cannot read the catalog; a catalog change takes effect on the site at the next publication. Each entry keeps the first format's `id` and `label`; a format change must keep the deployed reader working until the site deploys, so publish the new index first. Publication also regenerates `main/preview.svg`, the group means of the main trend; GitHub README embeds this image and links to the interactive page. GitHub image caching may delay updates. Re-publishing the same report leaves the journals unchanged and regenerates the index and preview; a conflicting identity fails. Non-force pushes retry ref races without remeasurement. Full clones include data-branch history; Git growth and registry retention remain operational costs.

The browser reads the data index from GitHub Raw when opened, downloads each monthly JSONL file overlapping the selected time range once, merges them, and renders the filtered all-case trend and the compared cases locally. Month boundaries never split the chart. Requests revalidate cached files; observations are append-only and readers do not require a transactionally consistent snapshot of all months. A cached index may temporarily omit a new month. Unavailable files report an error. **Publishing measurements does not build or deploy the website.** The Docs workflow has no data-fetch step; local builds need no data or network. Before the first publication the page reports that data is unavailable. Agents can inspect the same files with `git fetch origin benchmark-data` and `git ls-tree -r FETCH_HEAD main/`.

Compiler and dependency upgrades stay in the trend; protocol/input changes require a compatibility decision. Hardware specifications/protocols are separate plotted series, not a guarantee of one physical machine. Criterion intervals describe within-run estimates, not cross-run regression confidence or source-change causality.

### Setup and recovery

Reuse the `benchmark` environment's `BENCHER_API_KEY` and `BENCHER_PROJECT`. Its deployment branch policy must allow `main` as well as `refs/pull/*/merge`. Keep automatic execution disabled until a manually dispatched Bare Metal pilot confirms execution controls, complete ingestion, timeout headroom, Git publication and browser loading.

Rerunning the workflow measures again. If only publication failed, download the retained summary, check out the data branch, run `bun /path/to/source/bench/tracking/publish.ts /path/to/summary.jsonl`, then commit and push normally. Running the same command with any already published run regenerates the index and preview without remeasuring. Fourteen-day Actions evidence is diagnostic, not the long-term store. There is no site-refresh recovery step.
