# Continuous benchmarks

Rust changes and pushes to `main` run Default and Portable SIMD Criterion suites on [Bencher Bare Metal](https://bencher.dev/docs/explanation/images/). These measurements flag changes to investigate; they do not isolate source changes from code-placement effects. The [local measurement protocol](benchmarking.md#command-capabilities) provides paired experiments and defines their evidence limits.

## Read feedback from a PR

Run from the repository root with Node.js 24, the pinned pnpm dependencies and authenticated `gh`:

```sh
pnpm -s tools status main
pnpm -s tools status --pr 123
pnpm -s tools status --pr 123 --details
```

The command uses the authenticated GitHub CLI (`gh auth login`) and reads an exact remote commit's workflows and benchmark verdicts. Output is compact JSON by default: inspect `target.head`, `workflows[].state`, `benchmarks[].state` and `benchmarks[].title`. A current-head report URL appears in `report`. Add `--details` only when full `details.checks[].output.summary` or `details.comment.body` is needed. It does not submit jobs or change the PR. See [the tooling guide](tooling.md) for local reproduction, logging and command groups.

1. Match the returned `target.head` to the PR's current commit. Read `benchmarks[].state` and `benchmarks[].title`; use `--details` for `details.checks[].output.summary` and the matching `details.comment.body`; stale-head feedback is not the current result.
2. Check **both** `Benchmarks (rust, Default)` and `Benchmarks (rust, Portable SIMD)`. A green `Track benchmarks` workflow does not imply those checks passed. Distinguish a measured alert, a failed measurement and a missing comparable baseline.
3. A missing comment or pending check means feedback is not ready, not clean. If checks are absent, inspect the linked `Build benchmarks` run, then `Track benchmarks`. For baseline or queueing problems, use [Pipeline](#pipeline), [Testbeds](#within-job-scheduling) and [Alerts](#alerts) below.
4. For stacked PRs, verify the comparison uses the parent at the resolved base commit, not the whole stack against `main`.
5. Read each build summary and its alerted cases. Negative latency changes mean less run time. A failed or unmeasured build is not the clean opposite of a measured alert.

Report the head, each build verdict, relevant cases and baseline, and any pending, missing or failed evidence. Stop here for a feedback request. An alert that needs a new experiment follows the [measurement procedure](benchmarking.md#run-an-experiment) after authorization.

## Read main and history

Use the [public dashboard](https://bencher.dev/perf/yikai-liao-s-project) and the corresponding `Track benchmarks` job summaries to inspect existing `main` results. Check the commit, build/Testbed and baseline: `main` compares with its previous sample; its first sample has no comparison. [Alerts](#alerts) defines missing-baseline and PR start-point rules.

For PR trends, use the report's [history](#pr-history): all points compare against the latest report's baseline over a fixed common case set, separately for each build. Coverage changes and aggregate means are not source-change attribution. `pnpm -s tools status main` reads current `main`; `--pr NUMBER` reads that PR’s current head. The commands do not replace dashboard history.

Read the sections below when changing tracking, investigating missing feedback or interpreting report details. Tracker changes need the [tooling source map](../design/benchmark-tooling.md); deployment and baseline seeding in [Setup](#setup) are remote changes, not part of reading existing evidence.

## Pipeline

- [Build](../../.github/workflows/bench-build.yml): unprivileged, including forks; calls `pnpm tools continuous build` to compile Default and Portable SIMD sequentially in one job, both with Rayon enabled. The build verifies selected serial and parallel outputs with Criterion's untimed test mode. Static glibc preserves allocator behavior in `api` mode. Artifacts are `bench-rust-default` and `bench-rust`.
- [Track](../../.github/workflows/bench-track.yml): trusted default-branch code resolves PR identity via the API, skips closed/superseded heads, wraps executables with the trusted [supervisor](../../bench/continuous/worker.ts), and uses `pnpm tools continuous submit` to submit image digests. Executables run only on Bencher without credentials. Each configuration has a separate timeout; failure of one does not skip the other.
- Builds share compiler and flags; Portable SIMD adds runtime dispatch, not forced ISA or `target-cpu=native` (the build and measurement hosts differ). Both use `-C llvm-args=-x86-branches-within-32B-boundaries` for Intel JCC boundary hazards. This benchmark-only flag does not eliminate all placement effects.
- Bencher Free has one remote slot and a five-minute job limit. Each run times out at 240 seconds; target at most 200–210 seconds including preparation, validation and statistics. The two configurations remain two sequential remote jobs, not four serial/parallel jobs. Do not overlap parallel cases to meet the budget; reduce coverage rather than weaken sampling or increase the timeout. The shared `bencher-bare-metal` concurrency group queues up to 100 tracking runs and holds its slot across both measurements. It does not coordinate other repositories in the organization.

Tracker changes in a PR take effect only after merging: `workflow_run` uses default-branch code. During this rollout, the executable lists only serial IDs unless the supervisor explicitly sets `TESSERA_BENCH_PARALLEL=4`. An old tracker can therefore measure the baseline PR without rejecting new IDs. After merge the trusted supervisor enables both APIs and the first real `main` run seeds the new parallel histories; the bootstrap PR has no cloud parallel comparison. Never label that missing comparison as a demonstrated parallel gain.

## Within-job scheduling

The supervisor runs the plan's serial API timed IDs: small cases below 1,000,000 records balance across two single-threaded workers pinned to separate available CPUs; both exit before one pinned worker prepares or measures larger serial inputs. After every serial worker has exited, four-thread cases run strictly one at a time on a four-CPU mask. No other case's preparation, warm-up, validation, timing or pool teardown overlaps a parallel case. Only the selected sort's four workers run concurrently. Each pool is warmed before verification/timing and joined before the next case prepares. Selection precedes generation; Criterion directories are independent. Failed workers and missing, extra or duplicate results fail the run.

The `intel-v1` spec declares four vCPUs, not four proven exclusive physical cores. The supervisor selects available CPU IDs, fails if fewer than four are available for parallel cases, and logs masks and visible SMT topology. It preserves the serial phase's existing CPU selection. Exact `TESSERA_BENCH_IDS` selection distinguishes `/api` from `/par-4` for the same input; legacy group selection remains available to old artifacts. Direct binary invocation remains serial-only unless explicitly opted into the four-thread contract.

The [catalog](../../bench/catalog.toml) is the authority for configuration identities:

| Configuration | Features | Testbed | Hardware spec |
|---|---|---|---|
| Default | `std`, `rayon` | `intel-v1-hybrid-v1` | `intel-v1` |
| Portable SIMD | `std`, `portable-simd`, `rayon` | `intel-v1-portable-simd-hybrid-v1` | `intel-v1` |

These Testbeds separate concurrent measurements from the older serial histories. A first `main` sample establishes each baseline; older-base PRs may need rebasing after that sample exists. Artifacts predating the listing/selection protocol must be rebuilt. Never seed one configuration with another's measurements. Changed compiler, flags or hardware need a new Testbed unless the transition is explicitly justified here. Changed inputs/batches need new IDs or reset thresholds; adding cases or changing sample counts retains the Testbed.

## Cases

[`bench/plans/continuous.toml`](../../bench/plans/continuous.toml) defines coverage, compiled into each measured executable. Preview it with `pnpm -s tools plan continuous`; do not maintain a second case list here or in the supervisor. [Selection rules](benchmarking.md#experiment-plans) and the [case ID contract](benchmarking.md#cases-and-suites) apply. The serial `include` rules retain the serial timed IDs ending in `/api`, such as `rust/u64/random/65536/s1/api`. Separate `parallel` rules add IDs ending in `/par-4`: 10M seed-one random keys and rows, plus representative ordered, equal-key and skewed inputs. Parallel timing targets large inputs rather than short or threshold-adjacent cases; those boundaries retain correctness-test coverage. These rules do not multiply the whole serial suite by the parallel API. Both APIs include per-call scratch allocation; the parallel clock also includes dispatch into a pre-warmed caller-owned pool. There is no thread-count sweep or general mode axis.

Coverage must exercise insertion/tile boundaries, confirmed order, low-fanout tiles, each node route, key widths, floats, stable payloads and large working sets. Keep random controls beside targeted distributions. Small cases are placement-sensitive; their alerts need the same investigation as large ones. Insertion-threshold coverage includes random, sorted, reverse and equal inputs for direct `u64` keys and keyed rows. Ordered controls retain the root probe and reversal paths; only unsorted insertion-sized tiles take direct insertion finishing. Unit-stride order confirmation is covered by sorted and reverse keys and keyed rows at 1,000 records, with an equal-key input and the existing random controls. Added cases retain the same Testbeds and remain new measurements until comparative baselines exist.

Direct-key ascending-confirmation coverage also includes `u32` and `u64` at 65,536 records. To budget the exclusive parallel phase, specialized direct-key `d4096` and sparse-digit screens run at 65,536 rather than 1M records; Zipf2 remains at 65,536 rather than 1M/10M. The 1M near-ordered and 50%-hot-key screens are omitted, retaining their 65,536-record representatives and all random type, insertion-boundary and unit-stride controls. The long serial screens retain random keys at all measured widths, keyed rows, sparse wide keys, two clusters, and a 10M random-key control. Skewed and two-value direct-key screens stay at 65,536 instead of 1M records. This reduces large-scale distribution coverage; the local comparison plans remain available for deeper sweeps. Sampling windows and counts are unchanged.

| Length | Warm-up | Samples | Measurement window |
|---|---|---|---|
| Below 1,000,000 | 0.5 s | 30 | 1.5 s |
| 1,000,000 | 1 s | 40 | 4 s |
| 10,000,000 | 1 s | 20 | 4 s |

Every case is checked against a stable standard-library reference before timing, including original indices for rows. Batches prepare about 2^18 elements; copies are outside the timed region.

## Alerts

Each configuration has independent thresholds. `main` compares latency with its previous sample; changes beyond ±5% alert in either direction. PRs inherit thresholds at their base commit. For stacked PRs, resolve the parent's `pr-<number>` at the child's `base.sha`: GitHub feature-branch names are not Bencher branch names.

A promised comparison fails its check if there is no unique parent, no start point, the wrong base commit or no comparable cases. Measurement still runs so the failure remains inspectable. A first `main` sample intentionally has no comparison. The baseline is the start point's latest sample up to the base commit.

A rolling t-test was rejected because changing code is not one stationary population: after a step from about 190 µs to 124 µs, its window baseline stayed at 187 µs and called a 151 µs PR faster. The previous-sample rule follows code changes immediately. Cases noisier than 5% need their own boundary or a new ID; do not dismiss alerts as noise. Inspect the [public dashboard](https://bencher.dev/perf/yikai-liao-s-project), then use paired A/A and placement controls.

## Reports

[`bench/continuous/index.ts`](../../bench/continuous/index.ts) publishes one check per build, a job summary, and one combined PR comment per measured head. Same-head reruns update their comment; newer heads get a new comment and minimize older ones. Results for superseded heads go only to the summary. Missing supplemental history/images is logged in the footer without changing the measurement verdict.

Ownership: `report.ts` derives comparisons, `matrix.ts` aligns builds, `history.ts` normalizes history, `chart.ts`/`trend-chart.ts` render figures, `markdown.ts` formats, `images.ts` publishes, and `github.ts` resolves commits/checks/comments.

### Builds are columns, cases are rows

A separately compiled configuration has its own Testbed, check and column. Current runs have serial and four-thread API row groups within each executable; historical mode groups remain reportable, not separate builds or charts. Compute geometric means within each build and API group; a secondary all-case mean must not hide parallel rows among the larger serial suite. New groups explicitly show missing baselines. Combining builds can also hide a regression. Count a case once even if both builds alert. A failed build remains visible and must never be described as a clean opposite of a measured alert.

Minus means less run time. The summary gives per-build means and alerts; expanded tables give every changed case and baseline/limits/current values. Alert verdicts come only from thresholds, not chart emphasis.

### Comparison chart

Cases alerted in any build share one horizontal scale across columns, grouped by mode, slower first then faster. Orange/blue stems mean more/less time; hollow grey dots are within limits and missing baselines show `new`. Changes of at least 20% get stronger emphasis (`MAJOR` in `report.ts`). Only faster cases below 5% may fold (`FOLD_FASTER_BELOW`); slower cases never do, and folding cannot leave an empty chart.

### Report images

The trusted tracker alone has `contents: write` and writes SVGs to `benchmark-images`, under `pr-<number>/<language>/` or `main/<language>/`. Commit-pinned raw URLs keep earlier comments immutable; Actions ZIP artifacts cannot serve embedded SVGs. Writers serialize, retain other images and never force-push. Raw data stays in Bencher; image failure leaves numeric tables available.

### PR history

Load completed reports across reset heads and same-commit reruns, filtered by PR, Testbed and hardware spec, bounded by the current measurement time. Compare every point against the latest report's baseline over one fixed set of cases with a baseline and positive values in every run. Changes to coverage then affect the whole series, not one point.

A single-API history retains its overall geometric mean. Mixed-API histories show separate fixed-cohort serial and four-thread aggregate rows in the same two build columns; an API without comparable history shows an unavailable row, never a flat passing series. Failed builds remain unavailable columns in both figures. These are changes relative to the same API/thread-count baseline, not parallel speedups over serial. Include a case when its largest/smallest PR value minus one reaches 5% (`MIN_SWING`) or it still has an upper alert; show it in every build without a row cap. Rows share a vertical scale spanning at least 5%, with the base as the dashed axis. This preserves fixed regressions in history without changing today's alert verdict.

## Adding a language

Add an unprivileged build artifact per configuration and a tracker using the same queue, its own language ID, standard harness and Bencher adapter. Retain shared case IDs. Each configuration needs its own remote job, timeout and baseline; cross-language continuous timings remain indicative because their harnesses differ.

## Setup

1. Claim a public Free-plan project (unclaimed jobs are limited to one minute).
2. Configure the `benchmark` environment for `main` only, without reviewers; set `BENCHER_PROJECT` and secret `BENCHER_API_KEY`. Registry login uses the project slug.
3. After merging tracker changes, run `Build benchmarks` on `main` to seed both baselines.
4. Optionally require both named benchmark checks if alerts should block merging pending review; do not require only the green tracking workflow.

## Local use

```sh
cargo bench -p tessera-bench --bench smoke -- 'rust/u64/random/65536/s1/api' --exact
cargo bench -p tessera-bench --bench smoke --features tessera-sort/portable-simd -- 'rust/u64/random/65536/s1/api' --exact
```
