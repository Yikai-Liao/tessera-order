# Continuous benchmarks

Affected pull requests compare the required SIMD backend against an exact source baseline on GitHub-hosted runners. Each case alternates adjacent A/B and B/A samples; local `compare commit` uses the same protocol. Bencher Cloud archives relative results, not historical regression thresholds. These screens flag changes to investigate, not source-change causality independent of code placement.

## Read feedback from a PR

Run from the repository root with Node.js 24, pinned dependencies and authenticated `gh`:

```
pnpm -s tools status main
pnpm -s tools status --pr 123
pnpm -s tools status --pr 123 --details
```

1. Match `target.head` to the requested commit. Stale-head feedback is not current evidence.
2. Read `Benchmarks (rust)` in `benchmarks[]`. Missing or incomplete measurements cannot pass.
3. Read `workflows[].state`. The normal `CI` run builds, measures, attempts Cloud archival and publishes the report. An upload failure fails CI while the report retains its independently computed performance verdict.
4. Use `--details` for the exact baseline and affected cases. Negative changes mean less time. Stacked PRs compare with their parent branch, not automatically with main.

A read-only feedback request does not authorize new measurements. The [CLI guide](tooling.md#agent-output) defines output and authentication.

## Read main and history

The [dashboard](https://bencher.dev/perf/yikai-liao-s-project) stores current/baseline ratios, not absolute nanoseconds. A ratio of 1.10 means 10% more time than that run’s exact baseline. A PR's points form one trend even when merging main or a retarget changes the baseline: each point is the PR at that run against the baseline it was measured with, so a step in the trend can come from the baseline moving. Do not compare these ratios with another PR's.

Automatic measurement and Cloud archival run on PRs only. Merging does not repeat the benchmark on main; main still runs correctness and site checks selected by its diff. Retained older main ratios describe individual parent-to-commit effects, not absolute or cumulative latency. No upload depends on an earlier Cloud sample, and no historical threshold decides the performance check.


## Pipeline

- The ordinary [CI workflow](../../.github/workflows/ci.yml) uses the PR branch’s code for `prepare → measure → report`. The shared diff router selects measurement for engine, harness, plan and execution-tooling changes; presentation-only changes run tooling checks. There is no `workflow_run` publisher or second plan authority on main.
- Preparation calls `compare commit --plan continuous --prepare-only` once. Both revisions use the PR’s current harness, lockfile and toolchain; the baseline library comes from the merge base with the target SHA captured in the triggering event. Preparation retains the complete plan, resolved inventory, build provenance, executable hashes and exact executables in `prepared-revisions`.
- Four complete-case shards call `compare prepared --shard INDEX/COUNT` without installing Rust or compiling. Each checks executable hashes, harness/catalog compatibility and both inventories before timing. Fourteen-day `paired-*` artifacts retain JSON, reports and logs; the executable pair is stored once in `prepared-revisions`. Combine a shard’s `local.json` binary paths with that preparation artifact when inspecting retained binaries.
- Report reads this run’s prepared plan and inventory, verifies complete disjoint shards and matching source/build/executable identity, and recomputes verdicts from raw samples. It does not build a worker or execute artifact binaries. Cloud submission is attempted before rendering so successful reports include Report and JSON links; archival failure still leaves the raw-sample verdict, figures and summary available.
- Same-repository PR report jobs use the `benchmark` environment and publish GitHub results and Bencher Cloud ratios. Forks and Dependabot-triggered runs use measurement artifacts and job summaries without Cloud publication. A regression still fails their report job. `continuous submit` validates the returned Cloud identity and values. Failures remain explicit.

Both revisions enable Rayon and use the runtime-dispatched SIMD backend. Historical libraries enable their opt-in feature; current libraries require that backend without a selector. There is no forced ISA or `target-cpu=native`. Linux revision workers run under `setarch -R`, disabling process address randomization without changing host-wide settings. Same-binary hosted calibration showed stable process-specific offsets even after fixing only the executable base. ASLR control covers executable, heap, stack and library address randomization, not source-dependent code layout or physical page placement. The runner probes support before building and before executing a prepared pair, records the control, and fails rather than silently measuring an uncontrolled Linux pair. New relative Testbeds do not compare against the old static-linking/JCC absolute-time samples.

### Capacity and recovery

One preparation job and four measurement jobs cover the single required backend. Preparation retains both frozen executables and their full inventory; each measurement job checks their hashes and records its own host before sampling. Ordinary Rust CI retains its original parallelism; no test job waits for benchmarks. PR updates cancel obsolete CI runs. GitHub still provides no global priority or preemption guarantee. Image publication has no global queue; a conflicting ref update rebuilds against the latest image tree inside the same report job.

Only preparation compiles. CI supplies `TESSERA_BENCH_TARGET_DIR` and uses `Swatinem/rust-cache` for the actual Cargo target, separated by build configuration. CLI alignment flags are set before compiling, avoiding the preliminary unaligned build. Temporary source snapshots still disappear; cached dependencies survive. Cargo validates source changes and rebuilds the measured crate. PR caches are scoped by GitHub. Local caching is explicit and must not cross trust boundaries.

After deploying this workflow, update active branches such as #97 with the new CI source and trigger a fresh run. Former measurement-only workflows do not gain a report job when rerun; their retained artifacts can be inspected offline or archived with the source version and complete layout that produced them. Do not submit old two-shard evidence through the new four-shard validator.

Rerun failed jobs in their original CI run. Missing or expired artifacts require a new complete run; an archival retry can reuse complete retained artifacts without measuring again. A plan change is handled by that branch’s preparation and report code in the same run. The feedback target is four minutes from triggering CI through all selected checks and archival. Measure the complete critical path, including queues and publication; sampling budgets, cache hits and job timeouts are not completion evidence.

## Within-job scheduling

The partitioner balances complete cases by nominal sampling budgets. It does not shrink coverage or create a second inventory. Each job handles one case at a time and pins serial measurements to one allowed CPU, parallel measurements to exactly four. Fewer than four available CPUs fail before a parallel-plan build or prepared measurement.

Each sample pair uses two fresh processes, one per revision, with their own original input and warmed pool. The coordinator alternates AB/BA order for startup, warm-up and measurement. Only one process prepares, verifies or sorts at a time, and both stop before the next pair starts. Identical executable basenames keep the launch argument shape constant. Fresh workers make process-specific state vary across samples instead of treating repeated observations of one biased process pair as independent evidence. Cancellation kills and reaps workers; EOF or malformed/incomplete responses fail the comparison.

Each sample restores bounded batches, times only sort calls, then checks their outputs against the stable reference. Copies, validation and IPC are outside the clock; per-sort scratch allocation remains inside. Parallel calls include entry into the warmed caller-owned pool. The prepared batch cap is the existing 2^18 elements, with at least one complete input. The sample wall-time budget includes restoration and verification; observed nanoseconds per sort divide timed duration by the executed sort count. This is an explicit change from Criterion’s calibration, not a claim that the old and new measurements are interchangeable.

## Cases

The [continuous plan](../../bench/plans/continuous.toml) caps inputs at 1,000,000 records and selects 69 cases: 61 serial and eight four-thread. The former 10M serial random case is covered by the existing 1M case; all eight parallel shapes now use 1M records. Release and dedicated large-input experiments retain their own coverage. The [catalog](../../bench/catalog.toml) owns the per-revision warm-up, sample count and sampling budget, shared by Rust workers and completeness checks. Both time budgets are divided by the sample count for each fresh worker; startup/input preparation and at least one complete measured batch add overhead, not another full-case budget:

| Length | Warm-up | Paired samples | Sampling budget per revision |
|---|---|---|---|
| Below 1,000,000 | 0.5 s | 30 | 1.5 s |
| 1,000,000 | 1 s | 40 | 4 s |

There is no outer repetition. Each sample is a bounded batch measurement, not one sort. Preview coverage with `pnpm -s tools plan continuous`. Preparation resolves the full inventory through Rust; report uses that exact retained inventory.

## Alerts

For each sample pair, compute B/A. Report the median ratio and a distribution-free interval for the population median, using exact binomial tails with at least 99% coverage under independent samples. A slowdown alert requires the entire interval above 1.05; an improvement requires it below 0.95. Other results mean no alert, not proof of equivalence. The statistical owner is `bench/statistics.ts`. No extra A/A pass is required.

Adjacent pairs reduce slow drift; AB/BA alternation balances order effects. Neither eliminates preemption, correlated samples, cache effects or code-placement bias. Nominal coverage is per case, not a family-wide guarantee. Same-source hosted calibration must check practical false alerts; unexpected results need investigation rather than automatic source attribution.

| Configuration | Testbed | Measure |
|---|---|---|
| Tessera | `ci` | `paired-time-ratio` |

Testbeds identify build configuration and protocol, not individual PRs or stable runner hardware. New submissions use Bencher branches `pr-N`; historical `main` points remain readable; hashes identify measured heads. BMF stores the median and interval endpoints. Exact baselines and raw pairs remain in the Actions evidence and report. Do not configure history-based thresholds on these Testbeds.

## Reports

The single-backend report uses `Benchmarks (rust)`. A short paragraph states the baseline, compared-case count and overall run-time change, including an explicit result when there are no regression or improvement alerts. It retains changed-case summaries and tables, a focused comparison figure, a compact overview when there are no alerts, a PR trend figure and the complete case table. Historical multi-build reports remain readable. Ratios display as ×, never nanoseconds. Missing measurements fail their performance check independently; image or Cloud failures do not erase valid numeric evidence.

### Report images

The report job maintains the existing `benchmark-images` branch. Image failure leaves tables available; it does not alter performance verdicts.

### PR history

Each PR submits to its `pr-N` Bencher branch without a Start Point or Head reset. No main measurement is needed. The report reads every run on that branch with the current Testbed and hardware specification, including Heads that earlier submissions created per baseline, so merging main or rebasing does not start a new trend. Each ratio is plotted directly against one over a fixed shared case cohort, and the figure names the latest baseline. The existing swing/current-regression focus rules and trend figure are retained. Cloud links select the current Head, which holds only the runs since the last reset. Cloud failure leaves the current measured point visible.

## Setup

Keep `BENCHER_PROJECT`, `BENCHER_API_KEY` and the `benchmark` environment. Its deployment branch policy allows `refs/pull/*/merge`, so same-repository PR report jobs can archive directly. Only the report job receives credentials and write permissions; prepare and measure have read-only access. This explicitly trusts same-repository PR code with publication credentials. Fork PRs receive neither secrets nor write permissions. No tracker deployment, dedicated runner or registry is needed.

## Local use

Use the same `pnpm tools compare commit BASE --plan continuous` command, optionally selecting one shard or a smaller plan. There is no default nine-round local run. `pnpm tools report DIR` regenerates retained results offline; older protocols and legacy absolute-time histories use their original tooling revision. See [tooling](tooling.md#reproduce-continuous-work-locally) for commands and [benchmarking](benchmarking.md#working-tree-comparisons) for evidence limits.

To reuse builds locally, run `compare commit BASE --plan continuous --prepare-only --out PREPARED`, then `compare prepared PREPARED --shard 1/4 --out RESULT`. Each result directory must be new. A prepared pair has no samples and is not passing evidence; it retains the complete plan, source/build provenance and executable hashes. Measurement requires the original harness and catalog, rejects modified or escaping executable paths, verifies both listings, and retains the exact executed copies. Preparation remains unchanged and can serve independent shards. Interrupted measurement is still not resumable.
