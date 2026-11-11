# Continuous benchmarks

Rust changes and pushes to `main` compare the required SIMD backend against an exact source baseline on GitHub-hosted runners. Each case alternates adjacent A/B and B/A samples; local `compare commit` uses the same protocol. Bencher Cloud archives relative results, not historical regression thresholds. These screens flag changes to investigate, not source-change causality independent of code placement.

## Read feedback from a PR

Run from the repository root with Node.js 24, pinned dependencies and authenticated `gh`:

```
pnpm -s tools status main
pnpm -s tools status --pr 123
pnpm -s tools status --pr 123 --details
```

1. Match `target.head` to the requested commit. Stale-head feedback is not current evidence.
2. Read `Benchmarks (rust, Portable SIMD)` and any other active verdicts in `benchmarks[]`. Missing or incomplete measurements cannot pass.
3. Read `workflows[].state`. `Measure benchmarks` produces evidence; `Track benchmarks` publishes performance checks before attempting Cloud archival. An upload failure is a failed archival workflow, not a performance regression.
4. Use `--details` for the exact baseline and affected cases. Negative changes mean less time. Stacked PRs compare with their parent branch, not automatically with main.

A read-only feedback request does not authorize new measurements. The [CLI guide](tooling.md#agent-output) defines output and authentication.

## Read main and history

The [dashboard](https://bencher.dev/perf/yikai-liao-s-project) stores current/baseline ratios, not absolute nanoseconds. A ratio of 1.10 means 10% more time than that run’s exact baseline. PR points with the same baseline can describe within-PR history; a rebase or retarget that changes the baseline starts a separate segment. Bencher’s native series does not enforce that segmentation: consult the exact baseline in the associated Actions report rather than reading an unqualified continuous trend.

Main compares each commit with its first parent. Its points describe individual commit effects, not main’s absolute latency or cumulative speed; do not multiply ratios from different runners into a cumulative trend. No upload depends on an earlier Cloud sample, and no historical threshold decides the performance check.

Legacy absolute-time Testbeds remain available. `pnpm tools continuous history --project yikai-liao-s-project --out bench/results/main-history` reads those histories without new measurements. Retained `history.json` regenerates with `pnpm tools report DIR`. That legacy analysis mixes source, placement and host variation.

## Pipeline

- [Measure](../../.github/workflows/bench-build.yml) is unprivileged, including forks. One SIMD configuration × eight complete-case shards call the maintained `compare commit --plan continuous --shard INDEX/COUNT` command. Both revisions use the current harness, lockfile and toolchain; only the baseline library is substituted. Timed IDs must match before measuring.
- PR baselines use the merge base with the target SHA captured in the triggering event. The versioned native run title records PR/base/head identity. Publication checks eligibility without replacing the recorded target after retargeting or parent rewrites. Main and manual main runs use the first parent.
- Each 14-day `paired-*` artifact retains the plan, complete inventory, shard selection, source and executable hashes, build/host provenance, raw sample pairs, the executed baseline/current binaries, report and log. `local.json` records their relative `binaryPaths` and SHA-256 hashes. Downloaded binaries are untrusted PR code: inspect provenance first and run only in an isolated environment without credentials. The publisher still reads JSON only. Partial evidence remains inspectable, not passable.
- [Track](../../.github/workflows/bench-track.yml) runs trusted default-branch code. It builds/lists its trusted harness and validates complete, disjoint shards against that independent inventory. It reads artifact JSON only, never artifact code. Performance checks and the same-head PR report are published before Cloud archival.
- `continuous submit` uploads JSON using the existing Cloud credential and verifies the returned identity and values. Failure is explicit, per configuration, and does not change the already published measurement verdict. There is no OCI image, remote execution slot, baseline-readiness queue or scheduler.

Both revisions enable Rayon and use the runtime-dispatched SIMD backend. Historical libraries enable their opt-in feature; current libraries require that backend without a selector. There is no forced ISA or `target-cpu=native`. Linux revision workers run under `setarch -R`, disabling process address randomization without changing host-wide settings. Same-binary hosted calibration showed stable process-specific offsets even after fixing only the executable base. ASLR control covers executable, heap, stack and library address randomization, not source-dependent code layout or physical page placement. The runner probes support before building, records the control, and fails rather than silently measuring an uncontrolled Linux pair. New relative Testbeds do not compare against the old static-linking/JCC absolute-time samples.

### Capacity and recovery

Eight measurement jobs cover the single required backend. Ordinary Rust CI retains its original parallelism; no test job waits for benchmarks. PR updates cancel obsolete measurements; main runs use distinct concurrency groups so a newer push cannot replace a pending first-parent comparison. GitHub still provides no global priority or preemption guarantee. Only publication serializes image-branch writes.

CI supplies `TESSERA_BENCH_TARGET_DIR` and uses `Swatinem/rust-cache` for the actual Cargo target, separated by build configuration. CLI alignment flags are set before compiling, avoiding the preliminary unaligned build. Temporary source snapshots still disappear; cached dependencies survive. Cargo validates source changes and rebuilds the measured crate. PR caches are scoped by GitHub; the privileged default-branch publisher cannot restore PR-scoped caches. Local caching is explicit and must not cross trust boundaries.

Rerun failed measurement jobs in their original Actions run, then the tracker if needed. Missing/expired artifacts cannot be replaced by historical samples. An archival retry can reuse complete retained artifacts without remeasurement. The feedback target is four minutes from triggering CI through publication. Eight shards distribute the unchanged warm-up and sampling work; compilation, process startup, input preparation and verification remain additional wall time. Measure the complete critical path, including publication, rather than treating the sampling budget or a cache hit as completion evidence. The 10-minute job timeout is a safety bound, not a completion-time measurement; cold/warm cache duration and queue wait are separate acceptance observations.

## Within-job scheduling

The partitioner balances complete cases by nominal sampling budgets. It does not shrink coverage or create a second inventory. Each job handles one case at a time and pins serial measurements to one allowed CPU, parallel measurements to exactly four. Fewer than four available CPUs fail before a parallel-plan build.

Each sample pair uses two fresh processes, one per revision, with their own original input and warmed pool. The coordinator alternates AB/BA order for startup, warm-up and measurement. Only one process prepares, verifies or sorts at a time, and both stop before the next pair starts. Identical executable basenames keep the launch argument shape constant. Fresh workers make process-specific state vary across samples instead of treating repeated observations of one biased process pair as independent evidence. Cancellation kills and reaps workers; EOF or malformed/incomplete responses fail the comparison.

Each sample restores bounded batches, times only sort calls, then checks their outputs against the stable reference. Copies, validation and IPC are outside the clock; per-sort scratch allocation remains inside. Parallel calls include entry into the warmed caller-owned pool. The prepared batch cap is the existing 2^18 elements, with at least one complete input. The sample wall-time budget includes restoration and verification; observed nanoseconds per sort divide timed duration by the executed sort count. This is an explicit change from Criterion’s calibration, not a claim that the old and new measurements are interchangeable.

## Cases

The [continuous plan](../../bench/plans/continuous.toml) retains all 70 cases: 62 serial and eight four-thread. The [catalog](../../bench/catalog.toml) owns the per-revision warm-up, sample count and sampling budget, shared by Rust workers and trusted completeness checks. Both time budgets are divided by the sample count for each fresh worker; startup/input preparation and at least one complete measured batch add overhead, not another full-case budget:

| Length | Warm-up | Paired samples | Sampling budget per revision |
|---|---|---|---|
| Below 1,000,000 | 0.5 s | 30 | 1.5 s |
| 1,000,000 to below 10,000,000 | 1 s | 40 | 4 s |
| 10,000,000 and above | 1 s | 20 | 4 s |

There is no outer repetition. Each sample is a bounded batch measurement, not one sort. Preview coverage with `pnpm -s tools plan continuous`. Trusted publication resolves its own preset through Rust; intentional inventory changes must also be accepted by trusted default-branch code.

## Alerts

For each sample pair, compute B/A. Report the median ratio and a distribution-free interval for the population median, using exact binomial tails with at least 99% coverage under independent samples. A slowdown alert requires the entire interval above 1.05; an improvement requires it below 0.95. Other results mean no alert, not proof of equivalence. The statistical owner is `bench/statistics.ts`. No extra A/A pass is required.

Adjacent pairs reduce slow drift; AB/BA alternation balances order effects. Neither eliminates preemption, correlated samples, cache effects or code-placement bias. Nominal coverage is per case, not a family-wide guarantee. Same-source hosted calibration must check practical false alerts; unexpected results need investigation rather than automatic source attribution.

| Configuration | Testbed | Measure |
|---|---|---|
| Portable SIMD | `actions-screen-portable-simd-v1` | `paired-time-ratio` |

Testbeds identify build configuration and protocol, not individual PRs or stable runner hardware. Bencher branches remain `pr-N` and `main`; hashes identify measured heads. BMF stores the median and interval endpoints. Exact baselines and raw pairs remain in the Actions evidence and report. Do not configure history-based thresholds on these Testbeds.

## Reports

Existing comparison figures, numeric tables, image publication and same-head comment handling are reused. Builds are columns; cases are rows, with separate serial and parallel aggregates. Ratios display as ×, never nanoseconds. Missing measurements fail their performance check independently; image or Cloud failures do not erase valid numeric evidence.

### Report images

The trusted publisher maintains the existing `benchmark-images` branch. Image failure leaves tables available; it does not alter performance verdicts.

### PR history

Relative results do not use the legacy absolute-time trend renderer. Within-PR history is meaningful only within a fixed baseline segment; the baseline is linked from the report. A protocol change gets a new Testbed, not a fabricated continuation of old history.

## Setup

Keep `BENCHER_PROJECT`, `BENCHER_API_KEY` and the main-only `benchmark` environment. No dedicated runner, registry or scheduler variable is needed. Deploy measurement and tracker changes together: `workflow_run` uses default-branch code. Until the trusted publisher accepts this single configuration and its current harness feature provenance, a PR may retain valid raw measurements while publication rejects the configuration or reports a missing legacy Default build. That is a deployment failure, not passing performance evidence. Verify the head-attached Portable SIMD check and separate archival outcome after the trusted code is updated.

## Local use

Use the same `pnpm tools compare commit BASE --plan continuous` command, optionally selecting one shard or a smaller plan. There is no default nine-round local run. `pnpm tools report DIR` regenerates retained results offline; old recorded three- and nine-block reports remain readable. See [tooling](tooling.md#reproduce-continuous-work-locally) for commands and [benchmarking](benchmarking.md#working-tree-comparisons) for evidence limits.
