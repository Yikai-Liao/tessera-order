# Continuous benchmarks

Every push to `main` and every PR that touches Rust code runs the same small Criterion suite in two configurations, Default and Portable SIMD, on [Bencher Bare Metal](https://bencher.dev/docs/explanation/images/). It flags changes worth investigating. It is not the [release and A/B protocol](benchmarking.md): there is no in-process A/A or placement control, so an alert is not evidence for a performance claim.

## Pipeline

1. `bench-build.yml` runs without secrets, also for forks. A build matrix compiles `bench/rust/benches/smoke.rs` twice: with default features, and with `tessera-sort/portable-simd` added. Each static glibc executable has its own artifact, `bench-rust-default` or `bench-rust`. Static glibc keeps glibc's allocator, which `api` mode measures. The builds use the same compiler and flags; only the feature differs. Portable SIMD retains the artifact name and executable path consumed by the default-branch tracker, so a PR introducing this matrix can still be measured before its tracker changes merge.
2. `bench-track.yml` runs trusted default-branch code through `workflow_run`. It looks up the source PR through the GitHub API ([`github.ts`](../../.github/scripts/bencher/github.ts)) and skips closed or superseded PRs. It then wraps both executables in a `FROM scratch` image that the workflow defines itself, pushes the image and submits its digest to Bencher. It measures each executable in a separate remote job, one after the other, so each gets its own timeout. A failed measurement does not skip the other configuration. The executables only ever run on Bencher's runner, which receives no credentials.

Default keeps the default-on `std` feature. It may still use the compiler's automatic vectorization. Portable SIMD adds the optional `fearless_simd` backend, using stable Rust and runtime detection on x86. No SIMD level is forced or disabled: the remote CPU's available features determine the backend. `+crt-static` controls linking, and does not cap SIMD dispatch. The build does not use `target-cpu=native`, because the GitHub build host differs from the measurement host.

Bencher Free allows one job per organization, at most 5 minutes each. Tracking jobs share the `bencher-bare-metal` concurrency group with [`queue: max`](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#example-queueing-multiple-pending-runs), which keeps up to 100 pending runs. The tracking job waits for both remote runs while holding the concurrency slot. Each remote run has a 240-second timeout. The suite takes about 130 seconds locally per configuration; keep each under 3 minutes when adding cases.

## Cases

Benchmark IDs are `<language>/<case_id>/<mode>`, with `case_id` as in the [protocol](benchmarking.md#runner), for example `rust/u64/random/65536/s1/api`. The mode names the API being timed: `api` is the stable sort with its own scratch allocation, `workspace` the same sort with a prepared workspace. Further APIs, such as an unstable or a parallel sort, get their own mode in the same executable rather than a new build; reports group cases by mode ([Reports](#builds-are-columns-cases-are-rows)). Measure a new API on a representative subset of inputs: each build's run must stay under the remote job limit below. Each case is there for an engine path ([routing](../explain/routing.md)) or a key type. The 38 benchmarks cover:

- the insertion limit (`u64` only), one past it and one past the tile capacity, for `u64` keys and `{u32, u32}` rows, and one past the tile capacity for `{u64, u32}` rows. Cases this short move with code placement, so only the boundaries are kept;
- at 65,536 `u64` keys, random and one case per route: sorted (confirmed order), reverse (stable reversal), 99% sorted (budgeted insertion), all equal (constant keys), 16 values (one block partition), Zipf (s = 2) and `skew99` (equality partition), `sorted_tail` (outlier run), and `runs`, mixed ascending and descending runs on which the insertion probe must give up;
- `u32`, `u128` and `f64` random keys, and `{u64, u32}` rows (random, and 16 values), at 65,536;
- 99% sorted `u32` and `f64` keys at 65,536, reaching long integer and float inversion scans;
- sorted `f64` keys at 65,536, reaching vector ascending-order confirmation;
- 1,000,000 records for `u64` (random, Zipf, `skew99`, `two_values`, 99% sorted), `u32`, `u128` (random, and `sparse8`, which widens the top digit) and `{u64, u32}` rows;
- 10,000,000 `u64` records, random and Zipf, where every level goes through the block partition and the scratch working set leaves the caches;
- `workspace` mode beside `api` only where scratch allocation is a visible share of the time.

Cases at 1,000,000 records warm up for 1 s and take 40 samples over 4 s; a 1.5 s window gives them only a few hundred sorts, and their Bencher limits were the widest, ±5% to ±18%. Cases at 10,000,000 records take the same window with 20 samples, because 40 sorts of that length would need eight seconds. The other cases warm up for 0.5 s and take 30 samples over 1.5 s.

Every case is checked against a stable standard-library sort before timing; for rows the check includes the original index. Each Criterion batch prepares about 2^18 elements, so the cache footprint does not depend on measured times. Input copies are made outside the timed region.

## Alerts

Each configuration uses its own Testbed and thresholds. `main` uses a percentage test on latency against its previous sample: a value more than 5% above or below it is an alert, flagging potential regressions and improvements. PRs start from their base commit on `main` and inherit its thresholds. The limits are the same ±5% for every case, so a case whose noise exceeds 5% alerts on noise and needs its own boundary or a new ID. Why not a statistical test over a window of samples: [decision 0013](../decisions/0013-compare-with-the-previous-main-sample.md).

History and alerts are available on the [public Bencher dashboard](https://bencher.dev/perf/yikai-liao-s-project). Investigate an alert with the A/A and placement controls of the full protocol.

## Reports

[`bencher/index.ts`](../../.github/scripts/bencher/index.ts) turns the JSON report from `bencher run` into three outputs:

- **Checks `Benchmarks (rust, Default)` and `Benchmarks (rust, Portable SIMD)`** on the measured commit, one per build, so each can be required. Both start as in progress; each completes with its own verdict and holds that build's report on its own: counts in the title, caution and tip boxes, both charts, the overall line and expanded `Slower` and `Faster` tables. A check fails for possible regressions or a failed measurement, and passes when there are no regression alerts. The workflow itself does not fail on alerts or a remote measurement failure; the checks carry those failures.
- **Job summary** of the tracking run, for `main` and PRs.
- **PR comment** for each measured head, combining both builds after both measurements finish. A rerun of the same head updates its comment. A new head gets a new comment at the end of the timeline, and earlier reports (including comments from `bencher run --github-actions`) are minimized as outdated. That way the newest result is the visible one, and the PR shows one expanded report however often it is pushed. If the PR moves on while a run is queued, the stale result goes to the job summary only.

The scripts run directly on Node.js 24. `report.ts` owns the result model and current comparison; `matrix.ts` lines the builds' results up by case; `history.ts` loads and normalizes PR history. `chart.ts` and `trend-chart.ts` share the column layout, SVG styles and text metrics. `markdown.ts` formats the report; `images.ts` publishes generated SVGs; `github.ts` resolves commits and manages checks and comments. Run `npm ci`, `npm run scripts:typecheck` and `npm run scripts:test` to check them.

### Builds are columns, cases are rows

The number of measured combinations grows in two directions, and the report treats them differently so that it grows by rows rather than by sections or charts:

- A **build** is a separately compiled executable with its own Testbed and baseline, such as Default and Portable SIMD. Builds are **columns** everywhere in the comment: the overview, both charts and the changed-case table.
- An **API variant** of one executable, such as `api` (the stable sort), `workspace`, and later an unstable or parallel sort, is the `mode` segment of the case ID and is measured within each build's run. It is a **row group**: the overview gets one row per mode, and the charts group cases under a mode title. Adding an API adds rows, not a Bencher job, a check or a chart.

Each case is named once however many builds measured it, and each alert says whether every build shares it or which builds it is confined to. That is the first clue to a cause: a slowdown in both builds points to shared code, one confined to Portable SIMD points to the SIMD backend. A build that failed to measure stays in the overview and gets its own line; the case lines, tables and charts cover only measured builds, so "only" never means "the other build failed".

A comment reads top-down:

1. The title, `Benchmarks · rust · 3 slower, 12 faster`, counts distinct cases: a case slower in two builds is one slower case. A failed build is appended, such as `, Portable SIMD failed`.
2. The overview table has one column per build and the rows `All cases` and, when the suite has more than one mode, each mode (`stable`, `stable, workspace`, `unstable`, …). A cell is the geometric mean of current/baseline over that build's compared cases in that row, followed by its alert counts, such as `−4.7% · **2 slower** · 12 faster`. Bold marks alerts. Each build is compared with its own baseline on `main`. A build with no compared case shows `—`, and its headline says "no baseline" rather than "no significant change". There is no column across builds: their builds differ, so a combined mean could hide one build's regression behind the other's speedup.
3. A caution box counts the slower cases by where they occur, such as "1 in Portable SIMD only, 2 in both", and lists the three worst with every build's change, such as `u64 · Zipf 2 · 1,000,000: Default +0.6%, Portable SIMD +13.0%`; further cases are counted and left to the table.
4. A tip box does the same for faster cases, names the best and ends with "Nice work."
5. The two embedded charts follow: the comparison with main and, on a PR, the PR's history.
6. A collapsed block, "15 changed cases", holds one table of the cases alerted in any build: input, key type, record count and mode, then one column per build with the change and the current time, bold where that build alerted.
7. A small line counts new cases without a baseline in each build.
8. A collapsed "All cases, by build" block holds each build's full table with baseline, limits, current value, alert and benchmark ID.
9. A compact footer holds the measured and base commits, the workflow run, and per build its Testbed and links to the Bencher report, its JSON and the alerted cases' history.

Every change is the signed change in run time, such as −37.4%: minus means less time. The threshold model alone decides alerts. The baseline is the latest `main` sample up to the base commit, so one run's noise is part of every limit.

### Comparison chart

The comparison chart is a dot plot of the cases alerted in any build, with a name column and one column per build on one shared horizontal scale, so equal changes are equally long in every column. Each column's header has that build's geometric mean as a large number and its counts; its values sit right-aligned at the column's edge so they read down like a table. Cases are grouped under their mode, slower cases first from the worst, then faster ones from the best.

- An alert is a stem from zero ending in a dot, orange for more time and blue for less, with its value in bold. Changes of 20% or more (`MAJOR` in `report.ts`) get a larger dot and a bold case name.
- The same case within its limits in another build is a hollow grey dot with a grey value, so a change confined to one build is visible at a glance. A case without a baseline shows "new".
- Faster cases that changed by less than 5% in every build that alerted them (`FOLD_FASTER_BELOW` in `chart.ts`) share one line, such as "4 more faster, each by less than 5%", so the chart grows with notable changes rather than with the suite. Slower cases are never folded. If folding would leave the chart empty, the small speedups are drawn instead.
- A legend explains the marks and which way is less time. Two builds keep the 720 px width of a comment image; each further build adds a 180 px column.

### Report images

Comments need image URLs, and Actions artifacts are ZIP downloads, so the tracking job publishes each SVG itself. It has `contents: write` and commits the generated SVGs to the separate `benchmark-images` branch, under `pr-<number>/<language>/` or `main/<language>/`. One comparison and one trend SVG hold every build as a column; checks embed the same images. The comment embeds them from `raw.githubusercontent.com` at that image commit, which GitHub serves as `image/svg+xml` and embeds without a redirect. Pinning the commit keeps the images of earlier comments unchanged. The workflow's shared concurrency group serializes image writers, and branch updates never force-push. Raw report data stays in Bencher. See [decision 0009](../decisions/0009-benchmark-report-images-and-history.md).

### PR history

Each configuration's history comes from Bencher's report-list API, filtered by PR branch and its own Testbed, across every reset branch head. Only completed reports with the current hardware spec are compared, up to the current report's measurement time. Report UUIDs distinguish same-commit reruns.

Every run is compared with main's baseline from the latest report: the same reference as the comparison chart, fixed for the whole history. An earlier run on an older base therefore shows how far that code was from the main the PR would merge into, and rebases or force pushes need no special handling. Cases without a main baseline are left to the table. Every point uses the same set of cases, those with a baseline and a positive value in every run, so a case added to or removed from the suite changes the whole chart, never one point.

The history chart uses the comparison chart's columns: one per build with history, each with that build's change from its first to its latest PR run (the geometric mean over the set above) as the large number and its run count.

- The first row is Overall, each build's geometric mean against main per run.
- A case gets a row when, in any build, its swing within the PR, the ratio of its largest to its smallest PR value minus one, reaches 5% (`MIN_SWING` in `history.ts`), or when it still has an upper alert. The row then shows the case in every build, so a swing confined to one build sits beside the others' flat lines. A regression fixed later in the PR therefore stays visible here though the comparison no longer shows it. The swing is a ratio of PR values, so a case far from main does not magnify its rerun noise. There is no row cap; rows run from the largest swing down. A pill names a non-default mode or seed.
- Each cell is a line over the PR's runs with main as its dashed axis. The line is blue while the run takes less time than main and orange while it takes more, so it changes colour only where it crosses main. A small dot marks the first run and a larger one the latest; the value at the cell's edge is the latest run against main, bold for Overall and for changes of 20% or more.
- A row's cells share one vertical scale, spanning at least 5% and including main, so the same swing has the same height in every build and a small swing does not fill the row.
- The legend names main's commit and the range of PR runs.

History loading and image publication are supplemental: failures are logged and linked in the footer without changing the current measurement's verdict. Neither this descriptive history nor the continuous comparison replaces the A/A and placement controls of the full benchmark protocol.

The tracking workflow runs from `main`, so a PR that changes `bench-track.yml` or this script is still reported by the version on `main`. Check the Testbed in the comment when a PR changes the build.

Pushes to `main` that do not change code still run, adding samples of unchanged code to the history. When a case's input or batch changes, give it a new ID or reset its thresholds. When the compiler, flags or machine change, use a new Testbed. Adding or removing cases and changing sample counts retain the existing Testbed.

| Configuration | Features | Testbed | Hardware spec |
|---|---|---|---|
| Default | default (`std`) | `intel-v1` | `intel-v1` |
| Portable SIMD | default + `portable-simd` | `intel-v1-portable-simd` | `intel-v1` |

These retain the existing default-feature and SIMD histories because their build flags and case IDs are unchanged. After merging, run **Build benchmarks** on `main` once to ensure both configurations have a baseline sample. PR comparisons need a base sample in each Testbed. Do not use one configuration's results to seed the other. See [decision 0011](../decisions/0011-benchmark-feature-configurations.md).

## Adding a language

Each language adds its own build job (unprivileged, uploading one artifact per configuration) and track job (same concurrency group, publishing with its own `language`, which names its check and comment). It uses its standard harness with the matching Bencher adapter, such as Google Benchmark with `cpp_google` or pytest-benchmark with `python_pytest`. Benchmark names keep the shared `case_id` under the language prefix. Each language configuration is a separate Bencher job with its own 5-minute limit; the queue runs them one after another.

## Setup

1. Claim a public Bencher project on the Free plan. Unclaimed projects only allow 1-minute jobs.
2. In Settings → Environments → `benchmark` (deployments from `main` only, no reviewers), set the variable `BENCHER_PROJECT` to the project slug and the secret `BENCHER_API_KEY` to a project API key. The registry login uses the slug as its username.
3. The tracking workflow only runs once it is on the default branch. After merging, run **Build benchmarks** on `main` once to seed the baseline; alerts start with the second sample.
4. Once reports appear on PRs, **Benchmarks (rust, Default)** and **Benchmarks (rust, Portable SIMD)** can be made required checks. Replace any requirement for the former **Benchmarks (rust)** check. They fail on possible regressions, so require them only if an alert should block merging until someone reviews it.

## Local use

```sh
# Default
cargo test -p tessera-bench --all-targets
cargo bench -p tessera-bench --bench smoke -- 'rust/u64/random/65536/s1/api' --exact

# Portable SIMD
cargo test -p tessera-bench --all-targets --features tessera-sort/portable-simd
cargo bench -p tessera-bench --bench smoke --features tessera-sort/portable-simd -- 'rust/u64/random/65536/s1/api' --exact
```
