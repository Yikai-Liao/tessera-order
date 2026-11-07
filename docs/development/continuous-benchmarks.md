# Continuous benchmarks

Every push to `main` and every PR that touches Rust code runs a small Criterion suite on [Bencher Bare Metal](https://bencher.dev/docs/explanation/images/). It flags changes worth investigating. It is not the [release and A/B protocol](benchmarking.md): there is no in-process A/A or placement control, so an alert is not evidence for a performance claim.

## Pipeline

1. `bench-build.yml` runs without secrets, also for forks. It builds `bench/rust/benches/smoke.rs` with `tessera-sort/portable-simd` as a static glibc executable and uploads it as an artifact. Static glibc keeps glibc's allocator, which `api` mode measures. The optional feature uses stable Rust and selects the SIMD backend on the benchmark machine.
2. `bench-track.yml` runs trusted default-branch code through `workflow_run`. It looks up the source PR through the GitHub API ([`github.ts`](../../.github/scripts/bencher/github.ts)) and skips closed or superseded PRs. It then wraps the executable in a `FROM scratch` image that the workflow defines itself, pushes the image and submits its digest to Bencher. The executable only ever runs on Bencher's runner, which receives no credentials.

Bencher Free allows one job per organization, at most 5 minutes each. Tracking jobs share the `bencher-bare-metal` concurrency group with [`queue: max`](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#example-queueing-multiple-pending-runs), which keeps up to 100 pending runs. Each job waits for its remote run. The remote timeout is 240 seconds. The suite takes about 130 seconds locally; keep it under 3 minutes when adding cases.

## Cases

Benchmark IDs are `<language>/<case_id>/<alloc_mode>`, with `case_id` as in the [protocol](benchmarking.md#runner), for example `rust/u64/random/65536/s1/api`. Each case is there for an engine path ([routing](../explain/routing.md)) or a key type. The 37 benchmarks cover:

- the insertion limit (`u64` only), one past it and one past the tile capacity, for `u64` keys and `{u32, u32}` rows, and one past the tile capacity for `{u64, u32}` rows. Cases this short move with code placement, so only the boundaries are kept;
- at 65,536 `u64` keys, random and one case per route: sorted (confirmed order), reverse (stable reversal), 99% sorted (budgeted insertion), all equal (constant keys), 16 values (one block partition), Zipf (s = 2) and `skew99` (equality partition), `sorted_tail` (outlier run), and `runs`, mixed ascending and descending runs on which the insertion probe must give up;
- `u32`, `u128` and `f64` random keys, and `{u64, u32}` rows (random, and 16 values), at 65,536;
- 99% sorted `u32` and `f64` keys at 65,536, reaching long integer and float inversion scans;
- 1,000,000 records for `u64` (random, Zipf, `skew99`, `two_values`, 99% sorted), `u32`, `u128` (random, and `sparse8`, which widens the top digit) and `{u64, u32}` rows;
- `workspace` mode beside `api` only where scratch allocation is a visible share of the time.

Cases at 1,000,000 records warm up for 1 s and take 40 samples over 4 s; a 1.5 s window gives them only a few hundred sorts, and their Bencher limits were the widest, ±5% to ±18%. The other cases warm up for 0.5 s and take 30 samples over 1.5 s.

Every case is checked against a stable standard-library sort before timing; for rows the check includes the original index. Each Criterion batch prepares about 2^18 elements, so the cache footprint does not depend on measured times. Input copies are made outside the timed region.

## Alerts

`main` uses a t-test on latency against up to its last 32 samples, requiring at least 5 samples. Both the upper and lower boundaries are 0.999, flagging potential regressions and improvements. PRs start from their base commit on `main` and inherit its thresholds. Cases with high variance, such as the placement-sensitive cases at n ≤ 49, get wider limits automatically.

History and alerts are available on the [public Bencher dashboard](https://bencher.dev/perf/yikai-liao-s-project). Investigate an alert with the A/A and placement controls of the full protocol.

## Reports

[`bencher/index.ts`](../../.github/scripts/bencher/index.ts) turns the JSON report from `bencher run` into three outputs:

- **Check `Benchmarks (rust)`** on the measured commit. It shows as in progress while the Bencher job runs. It fails for possible regressions or a failed measurement, and passes when the only alerts are improvements. The workflow itself does not fail on alerts.
- **Job summary** of the tracking run, for `main` and PRs.
- **PR comment** for each measured head. A rerun of the same head updates its comment. A new head gets a new comment at the end of the timeline, and earlier reports (including comments from `bencher run --github-actions`) are minimized as outdated. That way the newest result is the visible one, and the PR shows one expanded report however often it is pushed. If the PR moves on while a run is queued, the stale result goes to the job summary only.

The scripts run directly on Node.js 24. `report.ts` defines the Bencher result model and computes the summary, `chart.ts` generates the SVG, `markdown.ts` renders the report text, and `github.ts` resolves commits and publishes checks and comments. Run `npm ci`, `npm run scripts:typecheck` and `npm run scripts:test` to check them.

A comment is written to be read top-down:

1. The title counts slower, faster and new cases. It is also the check title.
2. A red caution box names the worst slowdown and asks for a rerun or a controlled measurement before merging. A green tip box names the best speedup and counts the changes of 20% or more.
3. An overall line gives the change in run time as the geometric mean of current/baseline over all compared cases, so a halving counts the same on a short and a long case. The measured commit, the base commit and the Testbed follow.
4. Tables of the slower and the faster cases give the case, baseline, current time and change. Changes of 20% or more are bold. Each input links to its public history chart; the link title holds the benchmark ID.
5. New cases without a baseline, two data links, and a collapsed table of all cases with their limits and IDs come last.

Every change is the signed change in run time, such as −37.4%: minus means less time. Cases are named in words, `u64 · 99% sorted · 65,536`, rather than by benchmark ID. Seed 1 and the `api` mode are left out; other seeds and modes are named. The 20% tier only changes how a change is shown; the threshold model alone decides what is alerted. The baseline is the threshold model's mean of `main` samples up to the base commit, not a single run of the base commit.

For runs with alerts, the workflow uploads an SVG chart as the `benchmark-chart-rust` artifact. The chart shows the overall change and each alerted case on a shared scale.

The data links point at Bencher's public API and need no login: the report JSON holds every case with its baseline and limits, and the history JSON holds each sample of the alerted cases on `main` and the PR branch.

The tracking workflow runs from `main`, so a PR that changes `bench-track.yml` or this script is still reported by the version on `main`. Check the Testbed in the comment when a PR changes the build.

Pushes to `main` that do not change code still run, adding samples of unchanged code to the history. When a case's input or batch changes, give it a new ID or reset its thresholds. When the compiler, flags or machine change, use a new Testbed. Adding or removing cases and changing sample counts retain the existing Testbed.

The SIMD build uses Testbed `intel-v1-portable-simd` with hardware spec
`intel-v1`, keeping its history separate from the earlier default-feature
builds on Testbed `intel-v1`. After merging the build change, seed the new
baseline with five runs on `main`. PR comparisons need a base sample in the
new Testbed.

## Adding a language

Each language adds its own build job (unprivileged, uploading `bench-<language>`) and track job (same concurrency group, publishing with its own `language`, which names its check and comment). It uses its standard harness with the matching Bencher adapter, such as Google Benchmark with `cpp_google` or pytest-benchmark with `python_pytest`. Benchmark names keep the shared `case_id` under the language prefix. Each language is a separate Bencher job with its own 5-minute limit; the queue runs them one after another.

## Setup

1. Claim a public Bencher project on the Free plan. Unclaimed projects only allow 1-minute jobs.
2. In Settings → Environments → `benchmark` (deployments from `main` only, no reviewers), set the variable `BENCHER_PROJECT` to the project slug and the secret `BENCHER_API_KEY` to a project API key. The registry login uses the slug as its username.
3. The tracking workflow only runs once it is on the default branch. After merging, run **Build benchmarks** on `main` five times to seed the baseline; alerts start after five samples.
4. Once reports appear on PRs, **Benchmarks (rust)** can be made a required check. It fails on possible regressions, so require it only if an alert should block merging until someone reviews it.

## Local use

```sh
cargo test -p tessera-bench --all-targets --features tessera-sort/portable-simd
cargo bench -p tessera-bench --bench smoke --features tessera-sort/portable-simd -- 'rust/u64/random/65536/s1/api' --exact
```
