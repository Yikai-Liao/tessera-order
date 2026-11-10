<!-- SPDX-License-Identifier: BSD-2-Clause -->

# Repository CLI

Install Node.js 24 and the pnpm version pinned in `package.json`, then run `pnpm install --frozen-lockfile`. The repository entry point is `pnpm tools`; use `pnpm -s tools` to suppress pnpm's script banner when an agent consumes stdout.

## Task groups

| Task | Command |
|---|---|
| Define or preview an experiment | `pnpm -s tools plan local` |
| Compare uncommitted sources with a commit | `pnpm -s tools compare commit HEAD` |
| Compare local 8-worker sorting | `pnpm tools compare algorithms --plan parallel --cpu 0,2,4,6,8,10,12,14` (choose the host's physical-core mask) |
| Compare fixed-work serial counters | `pnpm -s tools compare counters HEAD` |
| Probe current-host PMU access | `pnpm -s tools continuous probe-counters --out bench/results/pmu` |
| Sample local function-section layouts | `pnpm -s tools compare commit HEAD --layout-seeds 1,2,3,4,5` |
| Compare sorting algorithms | `pnpm -s tools compare algorithms --plan smoke` |
| Resume a prepared algorithm comparison | `pnpm -s tools compare resume bench/results/SESSION` |
| Reproduce the native comparison CI job | `pnpm -s tools check --native` |
| Build both continuous artifacts | `pnpm -s tools continuous build --out bench/results/artifacts` |
| Run the continuous scheduler locally | `pnpm -s tools continuous run bench/results/artifacts/default/bench` |
| Submit a prepared image to Bencher | `pnpm -s tools continuous submit --help` |
| Regenerate an existing result | `pnpm -s tools report bench/results/SESSION` |
| Read current main feedback | `pnpm -s tools status main` |
| Read current PR feedback | `pnpm -s tools status --pr 123` |
| Reproduce CI change routing | `pnpm -s tools ci changes --base main --head HEAD` |
| Validate a PR title | `pnpm -s tools ci title 'fix(bench): validate source snapshots'` |
| Check every commit's DCO sign-off | `pnpm -s tools ci dco main HEAD` |
| Fetch and verify pinned native sources | `pnpm -s tools native fetch` |
| Validate a release tag | `pnpm -s tools release verify --tag rust/v0.1.0` |

Use a task's `--help` for its options. Local comparisons accept a Linux CPU list or range with `--cpu`; the [parallel measurement contract](benchmarking.md#local-parallel-comparisons) explains pool timing, affinity and memory exclusions. `plan` and both comparison tasks share the [experiment selection model](benchmarking.md#experiment-plans). `check` is a functional smoke check with a one-millisecond sample target, not a performance experiment. It runs harness tests, validates the smoke suite, measures one `u64/random` group, verifies that native programs do not link parallel runtimes when `--native` is selected, and generates a partial report.

## Agent output

`status` prints one compact JSON object by default. It resolves a remote ref once, reports that exact `target.head`, keeps only the latest workflow run per workflow/event/branch, and lists only unfinished or failed jobs and active or failed steps. Benchmark verdicts are independent of workflow success: `Measurement failed`, `no comparable baseline` and a slowdown alert retain their published check titles rather than becoming one generic failure. A missing check has `state: "missing"`; an empty workflow list is not a passing verdict.

For an open PR, CI may run against its test merge commit, while benchmarks attach to its head. `target.merge` records that distinction. A merged PR is still queried at its PR head; use `status main` for the squash commit. A trusted tracker runs default-branch code, so it is associated through the benchmark check's run identity, not by assuming its workflow SHA is the measured source. API or authentication failures exit nonzero instead of producing an empty successful result.

Start with the compact response:

```sh
pnpm -s tools status main
pnpm -s tools status --pr 123
```

Request larger evidence only when needed:

```sh
pnpm -s tools status --pr 123 --details
pnpm -s tools status main --text
```

`--details` adds full benchmark check summaries, the matching trusted report comment and workflow links under `details`. It does not download logs or historical reports. `--text` is an optional small human overview. Neither option triggers jobs or changes GitHub state. Authenticate the GitHub CLI with `gh auth login` first.

Build and measurement tasks print one compact object containing `out`, `log` and `result`. Subprocess output and progress go to the adjacent `OUT.log`, leaving machine-readable stdout free of Cargo, compiler and Criterion output. A failed operation exits nonzero with the log path. Each `compare resume` invocation uses a separate adjacent `OUT.resume-TIMESTAMP-PID.log`, preserving previous logs. Preparation, validation, memory and group progress go to these diagnostics, not machine-readable stdout. Use `--verbose` only for interactive diagnosis; it streams subprocess diagnostics instead and no longer guarantees JSON-only stdout. Report regeneration prints its output location without reading or rebuilding sources. Plans print TOML by default or compact JSON with `--format json`.

A successful `status` exit means the read succeeded, not that every workflow passed. An experiment's command exit describes execution, not proof of a speedup. Algorithm comparisons retain exact executable pairs and recorded settings before measuring; `compare resume DIR` accepts no selection or sampling overrides. See [recovery](benchmarking.md#resuming-algorithm-comparisons) for interruptions and locks. `continuous submit` records each remote measurement's outcome in `outcomes.json`, attempts both configurations even after a failure, and exits nonzero if either failed; regression checks are evaluated by report publication.

## Reproduce continuous work locally

The local commands use the same implementation as CI, not a second script translating workflow steps. Static artifacts require Linux x86-64 with glibc, Cargo and GNU `ldd`. Scheduling requires Linux, `taskset` and two CPUs for serial-only artifacts or four CPUs when parallel IDs are listed. Four-thread cases run exclusively after serial workers exit; the supervisor records allowed masks and visible topology. Native checks additionally require the [native comparison toolchain](benchmarking.md#native-prerequisites). None of these operations needs Python.

```sh
# One isolated build directory is shared by both configurations and removed afterward.
pnpm -s tools continuous build --out bench/results/artifacts

# Use the executable's embedded continuous plan for full coverage.
pnpm -s tools continuous run bench/results/artifacts/default/bench

# Reproduce scheduling on one saved or preset selection during development.
pnpm -s tools continuous run bench/results/artifacts/default/bench --plan local
```

A plan override is saved as `plan.json`; captured Criterion output is retained as `criterion.txt`. It deliberately changes coverage and is not equivalent to the full cloud suite. Omit `--plan` to reproduce the embedded selection. All output directories must be new. Temporary snapshots and build products live under `~/code/tessera-bench/` or `TESSERA_BENCH_WORK` and are cleaned after success or a handled failure; outputs and logs remain.

Remote submission is explicit. Install the Bencher CLI used by CI, prepare an immutable image containing both executables and the [trusted worker image layout](../../bench/continuous/Dockerfile), and provide `BENCHER_API_KEY` through the environment:

```sh
pnpm -s tools continuous submit \
  --project PROJECT --hash COMMIT \
  --image REGISTRY/PROJECT@sha256:DIGEST --out bench/results/remote

# PRs compare with main or the parent PR's exact baseline.
pnpm -s tools continuous submit \
  --project PROJECT --hash COMMIT --pr 123 \
  --start-point main --base-hash BASE_COMMIT \
  --image REGISTRY/PROJECT@sha256:DIGEST --out bench/results/remote-pr
```

The command owns thresholds, Testbed identities, timeouts and per-build outcomes. Image publication, credentials, permissions and queue ordering remain with the operator or CI. The remote image runs only Node.js and the benchmark executables; it receives no API credentials.

## CI boundary

| Workflow | Shared operation | Workflow-only responsibilities |
|---|---|---|
| Build benchmarks | `continuous build` | Checkout attribution, artifact upload, permissions |
| Native comparison | `check --native` | Install system tools, provision a runner |
| Track benchmarks | `continuous submit`, trusted report modules | Resolve the triggering event, publish the image, hold the queue slot, publish checks |
| Change routing | `ci changes` | Supply the event's base/head and collect Actions outputs |
| PR title and DCO | `ci title`, `ci dco` | Supply the title and trusted checkout; fetch fork objects without running them |
| Release | `release verify` | Registry authentication and actual publication |

Routing, title and DCO have dependency-free TypeScript bootstrap entry points under `.github/scripts/`. The first must run before language jobs; DCO must execute the base commit's trusted code without installing or running PR code. These adapters call the same functions as the CLI; they do not maintain separate policy. The shell DCO launcher exists only to enter that trusted checker. Rust feature tests, Miri and sanitizer commands stay in their existing workflows and composite action; they are not benchmark protocols.
