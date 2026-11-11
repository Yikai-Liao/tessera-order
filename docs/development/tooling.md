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
| Compare linked serial revisions and placement copies | `pnpm -s tools compare revisions BASE --current HEAD` |
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

Use a task's `--help` for its options. Algorithm and linked-revision comparisons default to three paired blocks; `--blocks 9` requests an extended run explicitly. Reports label the budget, inconclusive short screens do not trigger extra measurements, and resume retains the recorded count. `compare commit` instead uses the catalogued adjacent A/B sample budget without outer blocks. Local comparisons accept a Linux CPU list or range with `--cpu`; the [parallel measurement contract](benchmarking.md#local-parallel-comparisons) explains pool timing, affinity and memory exclusions. `plan` and both comparison tasks share the [experiment selection model](benchmarking.md#experiment-plans). `check` is a functional smoke check with a one-millisecond sample target, not a performance experiment. It materializes HEAD and working-tree revision copies, runs harness tests including generated adapters and memory-journal recovery, validates the smoke suite, measures one `u64/random` group, verifies that native programs do not link parallel runtimes when `--native` is selected, and generates a partial report.

## Agent output

`status` prints one compact JSON object by default. It resolves a remote ref once, reports that exact `target.head`, keeps only the latest workflow run per workflow/event/branch, and lists only unfinished or failed jobs and active or failed steps. Benchmark verdicts are independent of workflow success: `Measurement failed`, `no comparable baseline` and a slowdown alert retain their published check titles rather than becoming one generic failure. A missing check has `state: "missing"`; an empty workflow list is not a passing verdict.

For an open PR, CI may run against its test merge commit, while benchmarks attach to its head. `target.merge` records that distinction. A merged PR is still queried at its PR head; use `status main` for the squash commit. All current benchmark jobs belong to the normal CI run; the performance check is attached to the measured head. API or authentication failures exit nonzero instead of producing an empty successful result.

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

`--details` adds full benchmark check summaries, the matching report comment and workflow links under `details`. It does not download logs or historical reports. `--text` is an optional small human overview. Neither option triggers jobs or changes GitHub state. Authenticate the GitHub CLI with `gh auth login` first.

Build and measurement tasks print one compact object containing `out`, `log` and `result`. Completed revision and algorithm comparisons return structured results: protocol, ratio denominator, interval meaning, measured counts, faster/slower/within-noise counts, and up to five changes with their numeric evidence. `omittedChanges` counts changes beyond that limit; full results remain in `summaries.json`. Algorithm counts describe opponent/input pairs and exclude the two Tessera controls. Layout results keep each layout separate rather than pooling verdicts. Preparation returns a prepared identity and case count, not a performance verdict. Subprocess output and progress go to the adjacent `OUT.log`, leaving machine-readable stdout free of Cargo, compiler and Criterion output. A failed operation exits nonzero with the log path. Each `compare resume` invocation uses a separate adjacent `OUT.resume-TIMESTAMP-PID.log`, preserving previous logs. Preparation, validation, memory and group progress go to these diagnostics, not machine-readable stdout. Use `--verbose` only for interactive diagnosis; it streams subprocess diagnostics instead and no longer guarantees JSON-only stdout. Report regeneration returns the same compact comparison summary without reading or rebuilding sources. Plans print compact JSON by default or editable TOML with `--format toml`.

A successful `status` exit means the read succeeded, not that every workflow passed. An experiment's command exit describes execution, not proof of a speedup. Algorithm comparisons retain exact executable pairs and recorded settings before measuring; `compare resume DIR` accepts no selection or sampling overrides. See [recovery](benchmarking.md#resuming-algorithm-comparisons) for interruptions and locks. `continuous submit` retains `bmf.json` and the Cloud response `bencher.json`, and exits nonzero if validation or upload fails. It only reads validated JSON; paired regression checks are evaluated separately by report publication.

## Reproduce continuous work locally

The hosted preparation job builds the two revisions once; measurement jobs reuse their frozen executables through the maintained prepared-comparison command, with the complete continuous plan and one partition of timed IDs. Linux and `taskset` with four available CPUs are required for the four-thread plan. Serial cases and parallel cases never overlap on a runner. Both revisions use the same current harness, lockfile, SIMD backend and toolchain; historical libraries enable their opt-in feature explicitly. No Cloud history is needed.

```sh
# Build once; each measurement uses its own new result directory.
pnpm tools compare commit main --plan continuous --prepare-only \
  --out bench/results/prepared
pnpm tools compare prepared bench/results/prepared --shard 1/4 \
  --out bench/results/paired-1

# Regenerate one completed shard without building or measuring.
pnpm tools report bench/results/paired-1
```

Each shard keeps adjacent A/B sample pairs and the full catalogued sampling budget for its selected cases, without outer repetition. Omit `--shard` to run all cases on one host. `--shard INDEX/COUNT` partitions a resolved listing deterministically; it does not shorten samples. Results record the full inventory and selected subset. Directories must be new; temporary snapshots and default builds are cleaned, retained results and adjacent logs remain. Set `TESSERA_BENCH_TARGET_DIR` to a caller-owned Cargo cache to retain builds across runs; do not share it across trust boundaries. CI caches this exact target directory by configuration and sets the CLI alignment defaults before building.

Upload is explicit and never executes code. Install the pinned Bencher CLI, download `prepared-revisions` into `PREPARED` and every `paired-*` artifact for that run into separate directories under `ARTIFACTS`, and provide `BENCHER_API_KEY` through the environment:

```sh
pnpm tools continuous submit --project PROJECT \
  --hash COMMIT --base-hash BASE_COMMIT --pr 123 \
  --prepared PREPARED \
  --artifacts ARTIFACTS --out bench/results/submission
```

The command validates the prepared plan and inventory, disjoint shard coverage, exact revision pair, common source/build settings and positive complete sample pairs before uploading. Use the source version that created the prepared pair; no inventory build is needed. New relative Testbeds and the `paired-time-ratio` Measure keep old latency history untouched. Cloud receives paired median ratios and median intervals. An upload is attempted first so successful reports include Cloud links; upload failure remains explicit without changing raw-sample verdicts. Native Bencher Heads keyed by protocol and baseline split PR history. Artifact metadata catches inconsistent or incomplete evidence but cannot prove honest measurement by malicious PR code. Submission reads JSON only. Omitting `--pr` retains explicit legacy/manual main upload support; CI does not measure or archive main.

## CI boundary

| Workflow | Shared operation | Workflow-only responsibilities |
|---|---|---|
| CI prepare/measure | `continuous matrix`, `compare commit --prepare-only`, `compare prepared` | Event attribution, runners and artifact transfer |
| Native comparison | `check --native` | Install system tools, provision a runner |
| CI report | Paired validation, report modules, `continuous submit` | Same-run artifact download, publication credentials, GitHub checks/comments and Cloud archival |
| Change routing | `ci changes` | Supply the event's base/head and collect Actions outputs |
| PR title and DCO | `ci title`, `ci dco` | Supply the title and trusted checkout; fetch fork objects without running them |
| Release | `release verify` | Registry authentication and actual publication |

Routing, title and DCO have dependency-free TypeScript bootstrap entry points under `.github/scripts/`. The first must run before language jobs; DCO must execute the base commit's trusted code without installing or running PR code. These adapters call the same functions as the CLI; they do not maintain separate policy. The shell DCO launcher exists only to enter that trusted checker. Rust feature tests, Miri and sanitizer commands stay in their existing workflows and composite action; they are not benchmark protocols.
