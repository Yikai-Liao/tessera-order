# Continuous benchmarks

Rust changes and pushes to `main` run paired Default and Portable SIMD Criterion suites on GitHub-hosted `ubuntu-24.04` runners. Each runner measures both revisions; Bencher Cloud stores the resulting ratios. Shared-runner measurements flag changes to investigate, not source-change causality independent of code placement. The [measurement protocol](benchmarking.md#command-capabilities) defines those evidence limits.

## Read feedback from a PR

Run from the repository root with Node.js 24, pinned pnpm dependencies and authenticated `gh`:

```sh
pnpm -s tools status main
pnpm -s tools status --pr 123
pnpm -s tools status --pr 123 --details
```

1. Match `target.head` to the requested commit. Stale-head feedback is not the current result.
2. Read both `Benchmarks (rust, Default)` and `Benchmarks (rust, Portable SIMD)` in `benchmarks[]`. A successful tracking workflow is not a passing benchmark. Missing results and failed uploads are not clean measurements.
3. Read `workflows[].state`. If checks are absent, inspect `Measure benchmarks` and then `Track benchmarks`; the publisher creates head-attached checks after the measurement run completes.
4. Use `--details` for the matching check summaries and PR comment. Verify the exact baseline commit, including the parent branch for stacked PRs. Negative changes mean less run time.

Report the head, both build verdicts, baseline and relevant cases, including pending or failed evidence. A read-only feedback request does not authorize new measurements. See the [CLI guide](tooling.md#agent-output) for output and authentication.

## Read main and history

The [public dashboard](https://bencher.dev/perf/yikai-liao-s-project) stores one ratio per measured case and configuration. Each PR point is relative to its own resolved merge base; each main point is relative to that commit’s first parent. These are not absolute nanoseconds. A ratio of 1.10 means 10% more time than that run’s baseline. Consecutive ratios with different baselines are not directly comparable changes. The Actions artifacts preserve the exact baseline and raw blocks.

Old absolute-time Testbeds remain available. `pnpm tools continuous history --project yikai-liao-s-project --out bench/results/main-history` reads those legacy Testbeds, restricted to the latest report’s hardware spec, without new measurements. Retained `history.json` regenerates with `pnpm tools report DIR`. That adjacent-run analysis mixes source, placement and host variation; it cannot estimate layout-only noise.

## Pipeline

- [Measure](../../.github/workflows/bench-build.yml): unprivileged, including forks. The catalog supplies both configurations; `continuous matrix` supplies four shards per configuration. Eight independent jobs call `compare commit --plan continuous --shard INDEX/COUNT`. Each job builds both revisions with the current harness, lockfile and toolchain. Only the baseline library is substituted. Selected timed IDs must match before measurement.
- PR baselines are the merge base with the target SHA captured in the triggering event, not a historical Cloud sample or a target fetched at publication time. The native run title snapshots protocol version, PR number, base and head SHAs; the trusted resolver checks that identity and uses live PR state only for eligibility. Parent rewrites, retargeting and native reruns retain the captured baseline. Stacked PRs need no parent Cloud history. Main and manual main runs use the first parent. Missing metadata or an unavailable/incompatible baseline fails rather than substituting another revision.
- Each shard retains its plan, complete ID inventory, selected IDs, source/executable hashes, build/host provenance, nine raw blocks, table, figure and log as a 14-day `paired-*` artifact. Partial runs remain inspectable but cannot pass publication.
- [Track](../../.github/workflows/bench-track.yml): trusted default-branch code independently resolves source identity, skips closed/superseded heads, builds and lists its own trusted continuous harness once, then downloads JSON artifacts and validates complete disjoint shards against that independent inventory. It never builds or runs artifact code. `continuous submit` uploads BMF JSON with the existing Bencher credential; no OCI image, registry push, remote execution or Bare Metal slot remains.
- Publication derives verdicts from raw blocks, not uploaded summaries or Cloud historical thresholds. Each configuration has its own check; a failed configuration cannot hide the other’s evidence. Upload failure preserves measured tables but fails the corresponding check. Image failure is supplemental and leaves numeric tables available.

Both builds enable Rayon; Portable SIMD additionally enables runtime dispatch, without forced ISA or `target-cpu=native`. They use the common CLI’s alignment defaults and Cargo-resolved flags. The old static-linking/JCC configuration is not reproduced: the hosted protocol has new Testbeds and does not compare its ratios with old absolute-time samples. No cross-machine absolute baseline equivalence is assumed.

### Capacity and recovery

There is no measurement queue or baseline-readiness scheduler. PR head updates cancel obsolete measurements. Main runs retain their own independent parent comparisons, without requiring ordered Cloud completion. Only the short publication phase serializes writes to `benchmark-images`; it does not hold a measurement lock.

Ordinary Rust CI limits platform tests to two concurrent cells and Miri to one. Extra main platform cells follow the initial matrix, including after its test failures (but not cancellation); comparison, Miri and sanitizer jobs follow Rust lint and still run after lint failure. This leaves headroom for eight measurement jobs within the documented 20-slot account budget for a single run. Tests and benchmarks otherwise run together. GitHub provides no workflow priority/preemption guarantee: multiple PRs, other repositories and already-running jobs can still exhaust the account. We do not recreate a scheduler to enforce global ordering.

Rerun failed measurement jobs in their original Actions run, then rerun its tracker if needed. Incomplete artifacts fail closed. If artifacts expired, rerun the measurement workflow; no historical absolute sample can substitute for same-run controls.

## Within-job scheduling

The shard owner balances complete cases using nominal Criterion windows: weight 2 below one million records and 5 at or above it. This is a deterministic partition heuristic, not a measured runtime estimate or another case inventory. Each selected case retains its full sample budget.

Nine blocks each run the baseline executable twice and current executable once, rotating the six possible process orders. Each process runs selected serial cases followed by selected four-thread cases, without overlap. Serial calls use one CPU; parallel calls use exactly four CPUs selected from the allowed affinity mask. Case preparation, warmed caller-pool measurement and teardown complete before the next case starts. Fewer than four available CPUs fail before building. Host provenance records the selected mask, not an assumption of four exclusive physical cores.

Each observation is Criterion’s mean nanoseconds per sort. Per-block ratios divide current time by the mean of the two baseline observations. The reported ratio is the median of those nine ratios, not the ratio of two separately pooled medians. A direction requires at least eight agreeing blocks and a median outside the paired A/A range. Block and A/A ranges are observed ranges, not confidence intervals. Shared-runner interference and code placement can remain; investigate unexpected controls rather than calling them proven source effects.

## Cases

[`bench/plans/continuous.toml`](../../bench/plans/continuous.toml) retains the complete existing serial and four-thread coverage. Preview it with `pnpm -s tools plan continuous`; the executable resolves boundary names and lists timed IDs. No workflow maintains its own case list. Trusted publication resolves the same preset through its own Rust harness; a changed plan or resolved boundary inventory needs the trusted default branch updated before publication accepts it. The [selection](benchmarking.md#experiment-plans) and [ID](benchmarking.md#cases-and-suites) contracts apply.

| Length | Warm-up | Samples | Measurement window |
|---|---|---|---|
| Below 1,000,000 | 0.5 s | 30 | 1.5 s |
| 1,000,000 | 1 s | 40 | 4 s |
| 10,000,000 | 1 s | 20 | 4 s |

Every case checks against the stable standard-library reference before timing, including row identities. Per-call allocation remains inside the sort call, input copies outside. Four-thread calls include dispatch into a warmed caller-owned pool. Coverage, inputs, windows and sample counts are unchanged; repeating them in nine process blocks increases total measurement work. The hosted job timeout is 45 minutes, not a measured completion-time claim. Full-suite wall time, A/A ranges and false-alert behavior need hosted validation before changing the protocol or claiming faster feedback.

## Alerts

The maintained paired statistic owns the verdict, separately for each build. A slowdown outside the A/A controls fails its check; improvements are reported but do not fail. There is no previous-Cloud-sample or rolling-window alert on ratios. Missing shards, mismatched source/configuration, incomplete blocks or upload failure fail rather than showing “no change.”

The catalog owns separate hosted identities:

| Configuration | Features | Relative Testbed | Measure |
|---|---|---|---|
| Default | `std`, `rayon` | `actions-paired-default-v1` | `paired-time-ratio` |
| Portable SIMD | `std`, `portable-simd`, `rayon` | `actions-paired-portable-simd-v1` | `paired-time-ratio` |

BMF stores median ratio and observed block bounds. Raw A/A observations and the exact comparison baseline stay in the Actions artifacts and check summary. Old `intel-v1-*` absolute-time histories remain untouched. Do not configure historical percentage thresholds on the new ratio Testbeds: a prior PR may have a different denominator.

## Reports

The existing comparison matrix, Markdown tables, image branch and same-head PR comment lifecycle are reused. Builds are columns, cases are rows; serial and four-thread aggregate rows remain separate. A failed build stays visible. Ratios display as `×`, never nanoseconds; the table includes the A/A range and exact baseline commit.

### Report images

Only the trusted tracker has write permissions. It writes comparison SVGs to `benchmark-images`, using commit-pinned public URLs. Publication is serialized; writers retain other images and never force-push. Numeric tables survive image failure. Raw measurements stay in Actions artifacts, not in the image branch.

### PR history

The current report does not draw the legacy absolute-time PR trend from hosted ratios. Re-normalizing different merge-base ratios as though they shared a fixed baseline would misrepresent the data. Cloud retains the relative series; compare individual points with their exact Actions baseline, not as an absolute performance trend. Historical report regeneration remains supported.

## Setup

1. Keep the existing public Bencher project, `BENCHER_PROJECT` repository variable and `BENCHER_API_KEY` environment secret. The `benchmark` environment permits only `main`; it is used only by the trusted publisher.
2. Merge both workflows together. Tracker changes execute from the default branch; a PR cannot deploy its own publisher. The migration PR may therefore retain artifacts without new Cloud feedback until merge.
3. Run `Measure benchmarks` on main after merge and inspect both head-attached checks, the submission artifact and the new Testbeds. No baseline seeding or queue activation variable is required. `BENCHMARK_QUEUE_SINCE` is no longer consumed.
4. Optionally require both named benchmark checks, not merely `Track benchmarks`. Cloud metrics quotas still apply; registry bandwidth and Bare Metal limits do not.

## Local use

Use the same CLI path as CI; see [local reproduction](tooling.md#reproduce-continuous-work-locally):

```sh
pnpm tools compare commit main --plan continuous --shard 1/4
pnpm tools compare commit main --plan continuous --simd --shard 1/4
pnpm tools report bench/results/SESSION
```

A separate [diagnostics workflow](../../.github/workflows/bench-diagnostics.yml) retains counters and exploratory layout evidence without credentials or publication. It supplements rather than replaces paired benchmark feedback.
