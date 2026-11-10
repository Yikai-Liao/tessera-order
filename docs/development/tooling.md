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
| Inspect adjacent main changes | `pnpm -s tools continuous history --project yikai-liao-s-project --out bench/results/main-history` |
| Probe current-host PMU access | `pnpm -s tools continuous probe-counters --out bench/results/pmu` |
| Sample local function-section layouts | `pnpm -s tools compare commit HEAD --layout-seeds 1,2,3,4,5` |
| Compare sorting algorithms | `pnpm -s tools compare algorithms --plan smoke` |
| Resume a prepared algorithm comparison | `pnpm -s tools compare resume bench/results/SESSION` |
| Reproduce the native comparison CI job | `pnpm -s tools check --native` |
| Preview hosted benchmark jobs | `pnpm -s tools continuous matrix` |
| Reproduce one paired shard | `pnpm -s tools compare commit main --plan continuous --shard 1/4` |
| Upload completed paired JSON | `pnpm -s tools continuous submit --help` |
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

A successful `status` exit means the read succeeded, not that every workflow passed. An experiment's command exit describes execution, not proof of a speedup. Algorithm comparisons retain exact executable pairs and recorded settings before measuring; `compare resume DIR` accepts no selection or sampling overrides. See [recovery](benchmarking.md#resuming-algorithm-comparisons) for interruptions and locks. `continuous submit` records each configuration’s upload outcome in `outcomes.json`, attempts both even after a failure, and exits nonzero if either failed. It only reads validated JSON; paired regression checks are evaluated separately by report publication.

## Reproduce continuous work locally

The hosted jobs call the maintained revision comparison command, with the complete continuous plan and one partition of timed IDs. Linux and `taskset` with four available CPUs are required for the four-thread plan. Serial cases and parallel cases never overlap on a runner. Both revisions use the same current harness, lockfile, features and toolchain; no Cloud history is needed.

```sh
# New output directories; baseline can be any compatible local commit.
pnpm tools compare commit main --plan continuous --shard 1/4 \
  --out bench/results/paired-default-1
pnpm tools compare commit main --plan continuous --simd --shard 1/4 \
  --out bench/results/paired-portable-simd-1

# Regenerate one completed shard without building or measuring.
pnpm tools report bench/results/paired-default-1
```

Each shard keeps nine complete process-level blocks and the full sampling budget for its selected cases. Omit `--shard` to run all cases on one host. `--shard INDEX/COUNT` partitions a resolved listing deterministically; it does not shorten samples. Results record the full inventory and selected subset. Directories must be new; temporary snapshots and builds are cleaned, retained results and adjacent logs remain.

Upload is explicit and never executes code. Install the pinned Bencher CLI, download all eight `paired-*` artifacts into separate directories under `ARTIFACTS`, and provide `BENCHER_API_KEY` through the environment:

```sh
# Run from a trusted checkout; this builds/lists, without timing any cases.
pnpm tools continuous inventory --out bench/results/trusted-inventory
pnpm tools continuous submit --project PROJECT \
  --hash COMMIT --base-hash BASE_COMMIT --pr 123 \
  --inventory bench/results/trusted-inventory/cases.json \
  --artifacts ARTIFACTS --out bench/results/submission
```

Omit `--pr` for main. The command validates the complete continuous plan, exact trusted inventory, disjoint shard coverage, exact revision pair, common source/build settings and positive complete blocks before uploading each configuration. Local dirty-tree or custom-plan results are reportable locally, not accepted as continuous publication evidence. New relative Testbeds and the `paired-time-ratio` Measure keep old latency history untouched. Cloud receives median ratios and observed block ranges, not raw process observations. Artifact source metadata is self-reported by the unprivileged measurement job: validation catches inconsistent or incomplete evidence but cannot prove that malicious PR-authored code measured honestly. The publisher never evaluates artifact scripts or configuration as code.

## CI boundary

| Workflow | Shared operation | Workflow-only responsibilities |
|---|---|---|
| Measure benchmarks | `continuous matrix`, `compare commit --plan continuous --shard` | Checkout attribution, matrix runners, artifact upload, read-only permissions |
| Native comparison | `check --native` | Install system tools, provision a runner |
| Track benchmarks | `continuous inventory`, `continuous submit`, trusted report modules | Independently resolve the triggering event, download JSON, hold credentials, serialize image writes and publish checks |
| Change routing | `ci changes` | Supply the event's base/head and collect Actions outputs |
| PR title and DCO | `ci title`, `ci dco` | Supply the title and trusted checkout; fetch fork objects without running them |
| Release | `release verify` | Registry authentication and actual publication |

Routing, title and DCO have dependency-free TypeScript bootstrap entry points under `.github/scripts/`. The first must run before language jobs; DCO must execute the base commit's trusted code without installing or running PR code. These adapters call the same functions as the CLI; they do not maintain separate policy. The shell DCO launcher exists only to enter that trusted checker. Rust feature tests, Miri and sanitizer commands stay in their existing workflows and composite action; they are not benchmark protocols.
