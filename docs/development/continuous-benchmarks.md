# Continuous benchmarks

Every push to `main` and every PR that touches Rust code runs a small Criterion suite on [Bencher Bare Metal](https://bencher.dev/docs/explanation/images/). It flags changes worth investigating. It is not the [release and A/B protocol](benchmarking.md): there is no in-process A/A or placement control, so an alert is not evidence for a performance claim.

## Pipeline

1. `bench-build.yml` runs without secrets, also for forks. It builds `bench/rust/benches/smoke.rs` as a static glibc executable and uploads it as an artifact. Static glibc keeps glibc's allocator, which `api` mode measures.
2. `bench-track.yml` runs trusted default-branch code through `workflow_run`. It looks up the source PR through the GitHub API ([`github.ts`](../../.github/scripts/bencher/github.ts)) and skips closed or superseded PRs. It then wraps the executable in a `FROM scratch` image that the workflow defines itself, pushes the image and submits its digest to Bencher. The executable only ever runs on Bencher's runner, which receives no credentials.

Bencher Free allows one job per organization, at most 5 minutes each. Tracking jobs share the `bencher-bare-metal` concurrency group with [`queue: max`](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#example-queueing-multiple-pending-runs), which keeps up to 100 pending runs. Each job waits for its remote run. The remote timeout is 240 seconds. The suite takes about 130 seconds locally; keep it under 3 minutes when adding cases.

## Cases

Benchmark IDs are `<language>/<case_id>/<alloc_mode>`, with `case_id` as in the [protocol](benchmarking.md#runner), for example `rust/u64/random/65536/s1/api`. Each case is there for an engine path ([routing](../explain/routing.md)) or a key type. The 35 benchmarks cover:

- the insertion limit (`u64` only), one past it and one past the tile capacity, for `u64` keys and `{u32, u32}` rows, and one past the tile capacity for `{u64, u32}` rows. Cases this short move with code placement, so only the boundaries are kept;
- at 65,536 `u64` keys, random and one case per route: sorted (confirmed order), reverse (stable reversal), 99% sorted (budgeted insertion), all equal (constant keys), 16 values (one block partition), Zipf (s = 2) and `skew99` (equality partition), `sorted_tail` (outlier run), and `runs`, mixed ascending and descending runs on which the insertion probe must give up;
- `u32`, `u128` and `f64` random keys, and `{u64, u32}` rows (random, and 16 values), at 65,536;
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

The scripts run directly on Node.js 24. `report.ts` owns the result model and current comparison; `history.ts` loads and normalizes PR history. `chart.ts` and `trend-chart.ts` share the SVG styles and text metrics in `svg.ts`. `markdown.ts` formats the report; `images.ts` publishes generated SVGs; `github.ts` resolves commits and manages checks and comments. Run `npm ci`, `npm run scripts:typecheck` and `npm run scripts:test` to check them.

A comment is written to be read top-down:

1. The title counts slower, faster and new cases. It is also the check title.
2. A caution box names the worst slowdown. A tip names the best speedup and counts changes of 20% or more.
3. Two embedded charts follow: the comparison of the current run with main, and on a PR, how the PR's own runs moved against main.
4. An overall line gives the change against main as the geometric mean of current/baseline over all compared cases.
5. Slower and faster tables separate input, key type, record count and allocation mode into columns. Changes of 20% or more are bold. Each input links to its public history chart; its link title holds the benchmark ID.
6. A small line counts new cases without a baseline; their details remain in the collapsed All Cases table.
7. A compact footer holds the measured and base commits, Testbed and links to the workflow run, the Bencher report, its JSON and the alerted cases' history.

Every change is the signed change in run time, such as −37.4%: minus means less time. The 20% tier only changes emphasis; the threshold model alone decides alerts. The baseline is the threshold model's mean of `main` samples up to the base commit, not a single run of the base commit.

The comparison chart draws every slower case. Faster cases that changed by less than 5% (`FOLD_FASTER_BELOW` in `chart.ts`) share one line, such as "2 more faster, each by less than 5%", so the chart grows with notable changes rather than with the suite; the headline counts and the All Cases table still include them. If folding would leave the chart empty, the small speedups are drawn instead.

### Report images

Comments need image URLs, and Actions artifacts are ZIP downloads, so the tracking job publishes each SVG itself. It has `contents: write` and commits the generated SVGs to the separate `benchmark-images` branch, under `pr-<number>/<language>/` or `main/<language>/`. The comment embeds them from `raw.githubusercontent.com` at that image commit, which GitHub serves as `image/svg+xml` and embeds without a redirect. Pinning the commit keeps the images of earlier comments unchanged. The workflow's shared concurrency group serializes image writers, and branch updates never force-push. Raw report data stays in Bencher. See [decision 0009](../decisions/0009-benchmark-report-images-and-history.md).

### PR history

History comes from Bencher's report-list API, filtered by PR branch and Testbed, across every reset branch head. Only completed reports with the current hardware spec are compared, up to the current report's measurement time. Report UUIDs distinguish same-commit reruns.

Every run is compared with main's baseline from the latest report: the same reference as the comparison chart, fixed for the whole history. An earlier run on an older base therefore shows how far that code was from the main the PR would merge into, and rebases or force pushes need no special handling. Cases without a main baseline are left to the table. Every point uses the same set of cases, those with a baseline and a positive value in every run, so a case added to or removed from the suite changes the whole chart, never one point.

- The headline is the change from the first to the latest PR run, as a geometric mean over that set.
- The Overall row shows the geometric mean against main at its first and latest run; its right end matches the comparison chart's headline when both cover the same cases.
- A case is drawn when its swing within the PR, the ratio of its largest to its smallest PR value minus one, reaches 5% (`MIN_SWING` in `history.ts`), or when it still has an upper alert. A regression fixed later in the PR therefore stays visible here though the comparison no longer shows it. The swing is a ratio of PR values, so a case far from main does not magnify its rerun noise. There is no row cap; rows run from the largest swing down.
- Each row has its own vertical scale, spanning at least 5% and including main, so a small swing does not fill the row. Main is the row's dashed axis. The line is blue while the run takes less time than main and orange while it takes more, so it changes colour only where it crosses main. The ends carry measured times, or percentages for Overall.
- Only the first and latest runs have visible dots and commit labels. Every other run is a vertex of the line with a hover target linking to its Bencher report.

History loading and image publication are supplemental: failures are logged and linked in the footer without changing the current measurement's verdict. Neither this descriptive history nor the continuous comparison replaces the A/A and placement controls of the full benchmark protocol.

The tracking workflow runs from `main`, so a PR that changes `bench-track.yml` or this script is still reported by the version on `main`. Check the Testbed in the comment when a PR changes the build.

Pushes to `main` that do not change code still run, adding samples of unchanged code to the history. When a case's input or batch changes, give it a new ID or reset its thresholds. When the compiler, flags or machine change, use a new Testbed. Adding or removing cases and changing sample counts retain the existing Testbed.

## Adding a language

Each language adds its own build job (unprivileged, uploading `bench-<language>`) and track job (same concurrency group, publishing with its own `language`, which names its check and comment). It uses its standard harness with the matching Bencher adapter, such as Google Benchmark with `cpp_google` or pytest-benchmark with `python_pytest`. Benchmark names keep the shared `case_id` under the language prefix. Each language is a separate Bencher job with its own 5-minute limit; the queue runs them one after another.

## Setup

1. Claim a public Bencher project on the Free plan. Unclaimed projects only allow 1-minute jobs.
2. In Settings → Environments → `benchmark` (deployments from `main` only, no reviewers), set the variable `BENCHER_PROJECT` to the project slug and the secret `BENCHER_API_KEY` to a project API key. The registry login uses the slug as its username.
3. The tracking workflow only runs once it is on the default branch. After merging, run **Build benchmarks** on `main` five times to seed the baseline; alerts start after five samples.
4. Once reports appear on PRs, **Benchmarks (rust)** can be made a required check. It fails on possible regressions, so require it only if an alert should block merging until someone reviews it.

## Local use

```sh
cargo test -p tessera-bench --all-targets     # validate every case
cargo bench -p tessera-bench --bench smoke -- 'rust/u64/random/65536/s1/api' --exact
```
