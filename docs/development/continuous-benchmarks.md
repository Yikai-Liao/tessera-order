# Continuous benchmarks

Every push to `main` and every PR that touches Rust code runs a small Criterion suite on [Bencher Bare Metal](https://bencher.dev/docs/explanation/images/). It flags changes worth investigating. It is not the [release and A/B protocol](benchmarking.md): there is no in-process A/A or placement control, so an alert is not evidence for a performance claim.

## Pipeline

1. `bench-build.yml` runs without secrets, also for forks. It builds `bench/rust/benches/smoke.rs` with `tessera-sort/portable-simd` as a static glibc executable and uploads it as an artifact. Static glibc keeps glibc's allocator, which `api` mode measures. The optional feature uses stable Rust and selects the SIMD backend on the benchmark machine.
2. `bench-track.yml` runs trusted default-branch code through `workflow_run`. It looks up the source PR through the GitHub API ([`bencher_context.cjs`](../../.github/scripts/bencher_context.cjs)) and skips closed or superseded PRs. It then wraps the executable in a `FROM scratch` image that the workflow defines itself, pushes the image and submits its digest to Bencher. The executable only ever runs on Bencher's runner, which receives no credentials.

Bencher Free allows one job per organization, at most 5 minutes each. Tracking jobs share the `bencher-bare-metal` concurrency group with [`queue: max`](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#example-queueing-multiple-pending-runs), which keeps up to 100 pending runs. Each job waits for its remote run. The remote timeout is 240 seconds; the suite takes about 45 seconds locally. Keep it under two minutes when adding cases.

## Cases

Benchmark IDs are `<language>/<case_id>/<alloc_mode>`, with `case_id` as in the [protocol](benchmarking.md#runner), for example `rust/u64/random/65536/s1/api`. The 33 benchmarks cover:

- the insertion limit, one past it and one past the tile capacity, for `u64` keys and `{u32, u32}` and `{u64, u32}` rows;
- at 65,536 `u64` keys: random, 16 values, Zipf (s = 2), sorted, reverse, 99% sorted and all equal. Together they reach the order probe, the near-sorted insertion pass, the equal-key split and the radix passes;
- `u32`, `u128` and `f64` keys, and `{u64, u32}` rows with 16 values, at 65,536;
- 99% sorted `u32` and `f64` keys at 65,536, reaching long integer and float inversion scans;
- 1,000,000 records for `u64` (random, Zipf, 99% sorted), `u32`, `u128` and `{u64, u32}` rows;
- `workspace` mode beside `api` only where scratch allocation is a visible share of the time.

Every case is checked against a stable standard-library sort before timing; for rows the check includes the original index. Each Criterion batch prepares about 2^18 elements, so the cache footprint does not depend on measured times. Input copies are made outside the timed region.

## Alerts

`main` uses a t-test on latency against up to its last 32 samples, requiring at least 5 samples. Both the upper and lower boundaries are 0.999, flagging potential regressions and improvements. PRs start from their base commit on `main` and inherit its thresholds. Cases with high variance, such as the placement-sensitive cases at n ≤ 49, get wider limits automatically.

History and alerts are available on the [public Bencher dashboard](https://bencher.dev/perf/yikai-liao-s-project). PR alerts also appear in the Bencher check and PR comment. Alerts do not fail the workflow, and `main` alerts currently have no notification. Investigate an alert with the A/A and placement controls of the full protocol.

Pushes to `main` that do not change code still run, adding samples of unchanged code to the history. Changing the compiler, flags or suite makes old samples incomparable: use a new Testbed name or reset the thresholds.

The SIMD build uses Testbed `intel-v1-portable-simd` with hardware spec
`intel-v1`, keeping its history separate from the earlier default-feature
builds on Testbed `intel-v1`. After merging the build change, seed the new
baseline with five runs on `main`. PR comparisons need a base sample in the
new Testbed.

## Adding a language

Each language adds its own build job (unprivileged, uploading `bench-<language>`) and track job (same concurrency group, `--ci-id <language>`). It uses its standard harness with the matching Bencher adapter, such as Google Benchmark with `cpp_google` or pytest-benchmark with `python_pytest`. Benchmark names keep the shared `case_id` under the language prefix. Each language is a separate Bencher job with its own 5-minute limit; the queue runs them one after another.

## Setup

1. Claim a public Bencher project on the Free plan. Unclaimed projects only allow 1-minute jobs.
2. In Settings → Environments → `benchmark` (deployments from `main` only, no reviewers), set the variable `BENCHER_PROJECT` to the project slug and the secret `BENCHER_API_KEY` to a project API key. The registry login uses the slug as its username.
3. The tracking workflow only runs once it is on the default branch. After merging, run **Build benchmarks** on `main` five times to seed the baseline; alerts start after five samples.
4. Once reports appear on PRs, **Bencher Report (rust)** can be made a required check.

## Local use

```sh
cargo test -p tessera-bench --all-targets --features tessera-sort/portable-simd
cargo bench -p tessera-bench --bench smoke --features tessera-sort/portable-simd -- 'rust/u64/random/65536/s1/api' --exact
```
