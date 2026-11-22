# Benchmarking

The comparison runner in `bench/rust` measures sorting implementations on one machine: Tessera against the standard stable sort, other stable sorts, unstable sorts under their weaker tie contract, and the extra memory of each. [Continuous benchmarks](continuous-benchmarks.md) track PR regressions and publish the implementation comparison; local experiments and source attribution use the paired measurements described here.

## Command capabilities

Run `bun run tools` from the repository root after `bun install --frozen-lockfile`, with the pinned Bun and the Rust toolchain. Bun runs the TypeScript sources directly. `bun run --silent tools` suppresses the script banner for agents; each task's `--help` lists its options. [CI](ci.md#reproducing-locally) lists the non-benchmark commands.

| Task | Command | What it provides |
|---|---|---|
| Compare working-tree code with a commit | `bun run tools compare commit COMMIT` | Two revision builds, adjacent AB/BA sample pairs, verification, provenance and a report |
| Sample revision comparisons across layouts | `bun run tools compare commit COMMIT --layout-seeds 1,2,3,4,5` | One revision pair per fixed LLD seed and descriptive cross-layout ranges; Linux x86-64 GNU only |
| Compare fixed-work counters | `bun run tools compare counters COMMIT` | Verified serial batches with Callgrind instruction and simulated cache/branch counts |
| Probe local PMU access | `bun run tools compare probe-counters --out DIR` | Read-only, per-event `perf` availability on the current host |
| Compare linked source revisions | `bun run tools compare revisions BASE --current HEAD` | Serial revisions and independently named copies in one process, repeated-current A/A, verification, allocation and retained executables |
| Compare implementations | `bun run tools compare implementations` | Registered implementations in one timing executable, three shuffled blocks by default, two Tessera calls for A/A, verification and allocation measurement |
| Local parallel comparison | `bun run tools compare implementations --plan parallel --cpu 0,2,4,6,8,10,12,14` | Serial controls, Tessera-8 and Rayon stable-8 in the same harness; choose a CPU list for the host |
| Full implementation comparison | `bun run tools compare implementations --plan release` | Release coverage and native opponents |
| Resume an interrupted comparison | `bun run tools compare resume DIR` | Recorded selection, settings and exact retained builds; no rebuilding |
| Preview or customize inputs | `bun run --silent tools plan NAME` | Validated selection without compiling or measuring |
| Regenerate a report | `bun run tools report DIR` | Statistics and figures from retained results, without new measurements |
| Read main or PR feedback | `bun run --silent tools status main` or `bun run --silent tools status --pr NUMBER` | Exact-ref workflow and benchmark states; [read-only interpretation](continuous-benchmarks.md#read-feedback-from-a-pr) |
| Inspect core engine production lines | `bun run --silent tools code [BASE] [--current COMMIT] [--text] [--details]` | The same source scope and line statistics as PR reports, without builds or measurements |

`compare commit` defaults to `HEAD` and the `local` plan; `compare implementations` to the `smoke` plan. Incomplete implementation sessions report only with `--partial`; revision reports need every case's full sample budget.

CLI benchmark builds, including hosted ones, align loops and non-fallthrough blocks to 64 bytes (`-C llvm-args=-align-loops=64`, `-C llvm-args=-align-all-nofallthru-blocks=6`) for both revisions, Rust competitors and memory workers. The builder prepends these to the flags a Cargo probe resolves from the environment and configuration, so explicit values override them, and records the effective flags in provenance. Direct Cargo commands keep their own configuration.

### Optional local AOT CLI

For frequent CLI calls, a local [Bun AOT](https://bun.com/docs/compiler/aot) executable reduces startup and module loading; it does not change the Rust kernels. It needs a Bun with `bun compile` (validated with `1.4.4-canary.1+447ff870b`; stable `1.4.4` lacks it), builds for the host platform and is validated on Linux x86-64 only:

```sh
bun run tools:aot:build
./target/dev-tools/tessera-tools plan local
```

The build embeds the TOML resources and forbids runtime code generation from strings; Zod keeps its regular parser. The binary is a snapshot with no freshness check: rebuild after changing CLI sources, dependencies or embedded configuration, and use `bun run tools` while editing tooling. It lives in the ignored `target/` directory and is never a release asset or used by CI. Offline plan, metadata and small-report paths are checked; real build orchestration and larger reports remain [open validation work](../../ROADMAP.md#benchmark-tooling-and-investigations).

## Command output

Build and measurement tasks print one compact JSON object with `out`, `log` and `result`. Subprocess output goes to the adjacent `OUT.log` (each `compare resume` writes its own `OUT.resume-TIMESTAMP-PID.log`); a failed task exits nonzero with the log path. `--verbose` streams diagnostics instead, so stdout is no longer JSON only.

A completed comparison returns the protocol, ratio denominator, interval meaning, measured and faster/slower/within-noise counts, the host affinity, and up to five changes with numeric evidence: current regressions first for revision screens, opponents faster than Tessera first otherwise. `omittedChanges` counts the rest and `summaries.json` has everything. Implementation counts cover opponent/input pairs, excluding the two Tessera controls; layout results stay per layout; preparation returns an identity and case count, not a verdict. An affinity of `inherited` or a multi-CPU mask means no single CPU was pinned. `report` returns the same summary from retained files; `plan` prints compact JSON or editable TOML.

Exit status describes execution, not a speedup: a successful `status` means the read succeeded, not that workflows passed. `status` prints one object for the exact `target.head` of a remote ref, keeping the latest run per workflow/event/branch and listing only unfinished or failed jobs; `--details` adds check summaries, the report comment and links, `--text` a short overview. It changes no GitHub state and needs `gh auth login`. [PR feedback](continuous-benchmarks.md#read-feedback-from-a-pr) explains how to read it.

## Core engine code size

`code` compares the working tree with `HEAD` by default, including staged, unstaged and unignored untracked files. An explicit base selects another commit; `--current` selects a committed current revision instead of the working tree. It resolves commits locally and performs no build, measurement or network request. JSON output identifies the resolved baseline/current hashes (`hash: null` means the working tree), totals and net change; `--details` adds every source file's before/after counts and additions/removals. `--text` prints the compact table shared with PR reports: totals followed by files with added or removed production lines, including replacements with zero net change. With `--details`, the local table also includes unchanged files.

```sh
bun run --silent tools code --text
bun run --silent tools code main --text --details
bun run --silent tools code main --current HEAD --text
```

[`bench/code-change.ts`](../../bench/code-change.ts) owns the source selection: engine execution/planning, block partitioning, key encoding/classification, SIMD/scalar scans and panic recovery. API entry files, figure instrumentation (`trace.rs`, plus embedded `trace::emit` calls and `crate::trace` or `crate::engine::trace` imports) and benchmark/tooling sources are outside this metric. The lazily loaded [`code-source.ts`](../../bench/code-source.ts) uses ast-grep's Rust Tree-sitter grammar to distinguish syntax, comments and literals. AST patterns select the repository's test markers (`#[test]`, `#[cfg(test)]` and `#[cfg(all(..., test, ...))]`); `#[cfg(not(test))]` remains eligible. Test-only modules are excluded using their declarations in each revision, including declarations in `lib.rs` and `engine/mod.rs`, which do not contribute lines themselves. Other test-dependent configurations, inner test attributes and nested or path-attributed test modules fail explicitly. Source copies reuse the shared revision and working-tree snapshot operations.

jsdiff compares the filtered physical lines, ignoring comment and within-line whitespace edits; file moves count as removal and addition. This is a source-size metric across features, not a measure of maintainability or compiled code size. Missing history or unsupported source syntax fails the CLI command; the [PR report](continuous-benchmarks.md#reports) shows the statistic as unavailable without changing its performance verdict.

## Run an experiment

1. State the hypothesis, baseline or opponents, relevant inputs and the controls expected to stay unchanged. Preview the selection:

   ```sh
   bun run --silent tools plan local --types u64,row_u64 --distributions random,zipf2
   ```

   Use `local` for revision comparisons, `smoke` for a small implementation comparison, `release` for full coverage.

2. Record host and compiler conditions. Prefer an idle machine, performance governor, disabled turbo/SMT and a pinned logical CPU the affinity policy allows; changing governor, turbo or SMT needs operator approval. Native opponents need the [native prerequisites](#native-prerequisites).
3. Run the command:

   ```sh
   # Revision comparison; HEAD~1 must have a compatible API and harness.
   bun run tools compare commit HEAD~1 --types u64,row_u64 --distributions random,zipf2 --cpu 2

   # Rust-only implementation smoke comparison, including verification and reporting.
   bun run tools compare implementations

   # Full implementation comparison, including native opponents.
   bun run tools compare implementations --plan release --cpu 2
   ```

   CPU 2 is an example. `--out DIR` names a new directory (default `bench/results/<timestamp>`). Implementation comparisons take `--sample-ms` (default 5) and `--memory-mib` (default 256), fixed for the session. Keep raw results and research harnesses outside Git.

4. Retain the plan, provenance, verification and controls. Report coverage, environment, ratios, A/A ranges and result location, and name partial, interrupted and unmeasured coverage. Investigate changes outside the controls, including in untouched cases; do not dismiss them as noise or tune source layout to one executable.

## Additional source-attribution controls

Results hold for the measured inputs and environment. Attributing a gain to a source change rather than code placement needs in-process revision comparisons with independent placement controls ([engine evidence](../design/evidence.md#reading-small-case-results)); `compare commit` and continuous alerts alone do not provide it. `compare commit` alternates A/B samples of two executables without an A/A pass, and `--layout-seeds` varies section order across builds without sharing an address space; `compare revisions` provides that control. `compare implementations` measures the current snapshot only, not a baseline commit.

For an experiment the tools cannot run, extend the maintained pipeline so verification, provenance and reporting stay shared, starting with a bounded prototype; confirm unusual resource costs or host-wide changes with the operator.

## Linked revision comparisons

`bun run tools compare revisions BASE --current HEAD` links two named crates per revision into one serial executable: `tessera` and `tessera-copy` for current, `tessera-baseline` and `tessera-baseline-copy` for baseline, with repeated-current `tessera-aa` and `std-stable` as controls. Without `--current` it uses the working snapshot. It shares the implementation pipeline's axes, sampling, affinity, verification, budgets, allocation measurement, retention, resume and reports, but chooses its own adapters, rejects `--implementations` and selects no parallel APIs.

The temporary workspace renames copied packages without rewriting Rust source; it may add local lock entries but no different registry or Git entries. Provenance keeps resolved commits, original and generated source hashes and all four libraries' features, checked against Cargo artifacts. Incompatible baselines fail before timing; historical libraries enable their opt-in SIMD feature.

Ratios are **placement time / paired current time**, so a slower baseline row is a current improvement. Inspect both baseline copies and current B, not just the favorable copy. Distinct entries do not prove distinct bodies: check the retained executable's symbols and disassembly before claiming attribution, and reject folded paths. A/A bounds timing variability and same-source copies expose placement sensitivity; neither bounds all placement effects.

Direct-key scan experiments use the opt-in types `reverse_u64` and `reverse_f64`: Tessera's direct API in all four placements, the reversed total order for standard and Glidesort adapters, and no support in external radix or native adapters. No default preset selects them. A bounded three-block screen:

```sh
bun run tools compare revisions HEAD --types reverse_u64,reverse_f64 \
  --distributions sorted,sorted99,far_swaps,random --lengths 63,64,1000 \
  --blocks 3 --sample-ms 1 --memory-mib 16
```

This measures whole calls, not scan throughput, code generation or attribution; expand lengths and inspect the dispatched bodies before judging a scan replacement.

## Working-tree comparisons

`compare commit [COMMIT]` and `compare implementations` snapshot staged, unstaged and untracked sources without touching the checkout, build under `~/code/tessera-bench/` (`TESSERA_BENCH_WORK`) and clean up afterwards. Builds reuse `target/tessera-bench` (`TESSERA_BENCH_TARGET_DIR`, relative to the repository root); remove it for a cold build and never share it across trust boundaries. Internal symlinks are materialized; escaping or dangling links and cycles are rejected, and baseline links must resolve inside the selected commit's archive.

`compare commit` accepts any local commit compatible with the current crate API, harness and lockfile: both revisions use the current harness, lockfile and toolchain, and the baseline substitutes only `rust/tessera-order`. Linux workers run under `setarch -R` (checked before building). Timed IDs must match between revisions. The `local` plan measures only `rust/sort/u64/random/65536/s1/serial`; override its axes instead of running the full continuous inventory, and narrow IDs with `--filter`.

Fresh worker pairs measure adjacent A/B samples, reversing startup, warm-up and measurement order on alternate pairs. The catalog owns each revision's warm-up and sampling budget, divided across samples. Input preparation repeats in each process outside the clock. The report gives the median B/A ratio and its distribution-free 99% median interval; only an interval entirely outside 0.95–1.05 alerts. Coverage is nominal, per case and assumes independent pairs; the [continuous protocol](continuous-benchmarks.md#within-job-scheduling) defines batching and clocks. This is a development screen, not a placement-controlled claim.

Output: `plan.json`, `local.json`, `summaries.json`, `index.md`, `local.svg` and the executed `binaries/`. `local.json` records revision, source and executable hashes, executable paths, both builds' Cargo provenance, host information, selected IDs and observations. Regeneration reads only JSON and never runs downloaded executables, which are untrusted. Interrupted comparisons keep completed cases but cannot be resumed or reported as complete. [Local use of continuous work](continuous-benchmarks.md#local-use) covers `continuous prepare` and `continuous measure --shard`.

Standard report figures show fixed slices and aggregates; other lengths stay in Full results and `summaries.json`. The [figure guide](benchmark-figures.md) owns presentation and the [tooling design](../design/benchmark-tooling.md) the module boundaries.

### Layout sampling

`compare commit --layout-seeds 1,2,3,4,5` measures sensitivity to function-section order. It needs Linux x86-64, the `x86_64-unknown-linux-gnu` target, a GNU-compatible `cc` and LLD. Each seed (distinct, in 1..4294967295) becomes `--shuffle-sections=.text.*=<seed>` in the final rustc arguments; this is a local configuration, not the continuous flags. Linker versions, arguments and both link maps are retained; check the maps to confirm the functions under study moved.

Calibrate on identical sources first, with sensitive small inputs and large random controls:

```sh
bun run tools compare commit HEAD --types u32,u64 --distributions random --lengths 48,65536 \
  --layout-seeds 1,2,3,4,5 --out bench/results/layout-calibration
```

Seeds run sequentially. Reports keep each seed's paired interval and verdict, then the median and range of per-seed ratios and each revision's spread; per-case `layout-N.svg` figures show medians and observed ranges, which are not confidence intervals. Equal seeds need not give equal addresses across revisions, a few favorable seeds establish nothing, and shuffling sections does not vary block order, CGU or inlining. `--layout-control cgu1`, `aligned` or `engine-first` (engine functions placed first via `nm`) add a control, retained with flags, maps and `.order` files; none isolates a gain.

### Fixed-work counters

`compare counters [COMMIT]` uses the shared worker with the local-only `layout-study` feature. It requires Linux x86-64 and Valgrind (simulated ISA dispatch can differ from the host):

```sh
bun run tools compare counters HEAD --types u32,u64 --distributions random --lengths 48,65536 \
  --sorts 8 --out bench/results/counters-null
```

Each serial API case runs in three processes: baseline, a second baseline for A/A, and current (order reversed on alternate cases). Each verifies a sort, prepares `--sorts` copies (default 8, range 1..1024), calls the `tessera_counted_sorts` marker once and verifies the outputs; Callgrind collects that call tree with cache and branch simulation. `counters.json` keeps hashes, flags, Valgrind version, sort count, event totals and the dirty state. `Ir` counts instructions, `D1mr`/`D1mw` and `DLmr`/`DLmw` simulated data-cache misses, `Bcm`/`Bim` simulated mispredictions. Counts describe this warm workload and simulator, not hardware: equal counts cannot prove a time change is layout, and changed counts need not explain it. The report changes no verdict.

If the host libc needs instructions Valgrind lacks, build the baseline-ISA container (`docker build -t tessera-callgrind -f bench/local/Callgrind.Dockerfile .`) and pass `--valgrind-image tessera-callgrind`; it records the image ID, mounts sources read-only and has no network.

`compare probe-counters --out DIR` tries `instructions:u`, `branch-misses:u`, `idq.dsb_uops:u`, `idq.mite_uops:u` and `baclears.any:u` with the host's `perf` and records each result in `probe.json`. It changes no `perf_event_paranoid` setting and says nothing about cloud hosts.

## Experiment plans

Plans in [`bench/plans/`](../../bench/plans/) use TOML or JSON and decode into one validated selection. Every plan uses schema 3. They describe what to measure; the harness owns input generation and the protocol.

| Preset | Default selection |
|---|---|
| `local` | Tessera, random `u64`, 65,536 records, serial |
| `continuous` / `main` | One shared PR/main selection of Tessera, serial and `t4`; see the [cases](continuous-benchmarks.md#cases) |
| `smoke` | Rust competitors, 125 inputs at 1,000 records |
| `release` | Rust and native competitors, 381 inputs across four lengths |
| `parallel` | Serial controls, Tessera and Rayon stable at `t8`; 16 inputs at 1M records |

A plan is a union of `include` products followed by `exclude` products:

```toml [my-plan.toml]
schema = 3
implementations = ["glidesort", "radsort"]

[[include]]
types = ["u32", "u64"]
distributions = ["random", "zipf2"]
lengths = [65_536, 1_000_000]

[[exclude]]
types = ["u32"]
distributions = ["zipf2"]
lengths = [1_000_000]
```

Values within an axis are alternatives, axes within a rule form a Cartesian product, each included input is measured once, and any matching exclusion removes a combination. Omitted axes use catalog defaults when including and match everything when excluding. Empty choices and unknown fields or identities are errors. The input axes are `types`, `distributions` and `lengths`: positive record counts, or `"insertion-max"`, `"insertion-next"` and `"tile-next"`, resolved from the measured library's thresholds and record width. Case IDs keep the resolved number, and a commit comparison rejects differing IDs. A rule may also set `operation` ([operation comparisons](#serial-operation-comparisons)), defaulting to `sort`, and `executions`, defaulting to `["serial"]`; an exclusion with either matches only those values.

There is no `harness` field: all plans use the same catalog, input generator and sorting adapters. Parsing retains exactly the requested implementations. An implementation measures an execution only through an entry point it has: `serial` unless the catalog marks it parallel-only, and `tN` only for a parallel full sort, in a pool of N workers. The Criterion/paired worker measures one implementation because its [Criterion IDs](#case-identities) name case and execution, not implementation; tracking stores implementations in separate directories. The continuous plan selects `tessera` serially and at `t4`; a Tessera selection does not implicitly add another implementation. Paired implementation blocks instead use the union of both selections as a common input cohort and preparation adds `tessera` and the operation's serial [denominator](#choosing-references-and-cases) as controls to the frozen plan, with a separate Tessera A/A call during sampling. Inputs use seed 1, and an empty selection fails before timing. Native implementations add pinned source acquisition and native compilation. Unsupported adapter/type pairs are never measured or reported as zero.

```sh
bun run --silent tools plan local --types u32,u64 --distributions random,zipf2           # preview
bun run --silent tools plan local --types u32,u64 --format toml > my-plan.toml           # save, then reuse:
bun run tools compare commit main --plan my-plan.toml
bun run tools compare implementations --plan smoke --implementations glidesort,radsort \
  --types u64 --distributions random,zipf2 --lengths 1234,65536                  # override
bun run --silent tools plan --schema > benchmark-plan.schema.json                         # editor schema
```

`--plan` takes a preset name or a `.toml`/`.json` path. Comma-separated CLI axes replace that axis in every inclusion rule; exclusions stay. `--implementations` (only `compare implementations` and `plan`) replaces the implementation selection; only implementation-block preparation adds its required controls. `--executions serial,t4` replaces every inclusion rule's executions. Presets are not different execution routes: use `local` or `continuous` for revision/counter work, and `smoke`, `release` or `parallel` for implementation comparisons. Revision comparisons measure only `tessera`, at any executions, because their builds replace only the Tessera library; their operation rules run serially, and counters require a serial-only selection. These are operation constraints, not plan formats. `--group` runs one resumable group and marks the report partial. Every result keeps its frozen `plan.json`.

## Choosing references and cases

These rules decide which implementations and cases a cross-implementation comparison measures, for sorting and every other operation. The [comparison model proposal](../design/comparison-model-proposal.md) plans what the published comparisons of other operations still lack.

Every reference has one role for the operation it is compared on; an implementation without a role is not added.

| Role | Meaning | Required |
|---|---|---|
| Denominator | What a Rust user writes today with the standard library (or Rayon, for parallel execution) to get the same result. Speeds are ratios to it, and figure guides name it. The catalog's operation entry names it as `reference` and `parallel_reference`. | Yes |
| Composition baseline | The same result composed from Tessera's existing calls, such as a full sort for selection. It shows the work the operation saves, which is the condition for [admitting the operation](../design/operation-extensions-proposal.md#0-2-additions). | For every operation except full sorting |
| Strongest public implementation | The fastest known implementation of the same result contract, in any language. | When one exists |
| Ecosystem choice | An implementation callers commonly use for the result. | No |

An implementation is admitted for an operation when its result passes the shared checker, it is pinned as a dependency or a [verified native source](#implementations) with a compatible license, native code uses the published LLVM major, and it supports the operation's ranking cohort. A weaker tie contract is labelled separately and never enters a stable ranking. Remove an implementation when upstream is abandoned, it cannot build on supported hosts, or it repeats another reference's algorithm without a distinct question. Hosted [main tracking](continuous-benchmarks.md#main-long-term-tracking) additionally needs each of the implementation's jobs to finish within its time limit; an implementation that cannot is left out of hosted tracking only, and local release comparisons, which no hosted limit governs, keep it.

Cases reuse the shared [input generator](#input-generation) and its identities, so readings line up across operations; an operation adds only its own parameter axes, such as the selected rank or the thread count. Each axis answers a stated question:

- **Record shape:** keys, rows, wide records and owned records. Wide records matter most where an operation avoids moving records, as argsort does.
- **Distribution:** at least one pattern from each catalog group, so every group has a representative.
- **Length:** a short input, a cache-resident input and one beyond the last-level cache, chosen around the thresholds where costs change.
- **Parameters:** stated relative to the input length (for example a middle, a 1% and a last rank), with absolute values in the case ID.

Each operation has one named ranking cohort; an implementation that lacks a cohort case is not ranked rather than credited. Fix the cases before measuring the opponents: a case is added for the question it answers, not for its result, and cases Tessera loses stay. Before adding lengths to a tracked operation, estimate its cases against the hosted job limit.

## Serial operation comparisons

New plans admit four operations. Admission is for a caller-visible result worth measuring, not for every API entry point or internal engine route. Add an operation only when it answers a distinct performance question, has a complete result that an independent checker can verify, and has a meaningful reference on the same inputs. Adding a comparison operation does not automatically add it to the PR or main suites.

| Operation | CLI parameters | Result being measured |
|---|---|---|
| `sort` (default) | None | Complete in-place sorting. |
| `argsort` | None | Exact stable index permutation of an immutable input, including creation of the index vector. |
| `select_nth` | `--index I` | One zero-based ranked member and correct membership on either side. Stable variants preserve the input order of ties throughout the slice. |
| `partition-buckets` | `--bucket-count R` | Stable grouping into R categories and all R+1 offsets, including empty categories. |

- **Argsort.** Tessera calls `argsort` or `argsort_by_key`. The reference constructs indices and sorts them by key; `std-unstable` breaks ties by original index to return the same stable permutation.
- **Selection.** Tessera calls `select_nth` or `select_nth_by_key`. `std-stable`, Glidesort, Radsort and the optional `tessera-full` control sort the whole input as semantic references, not as partial selectors. `std-unstable` uses partial selection and may choose arbitrary equal-key members, including on payload records. It must still return the correct ranked key, partition membership by key and preserve every record. Stable routes also preserve the exact stable member identities at the rank and on each side. Ratios between these routes do not isolate the cost of stability.
- **Bucket partition.** The category is the encoded key modulo R, not the complete original key. Tessera calls `sort_by_cached_key` using the narrowest unsigned key width that represents the categories, then computes offsets. On inputs of at least two records, the cached-key sort evaluates the classifier once per record; the offset pass evaluates it again. `std-stable`, Glidesort, Radsort and `tessera-full` stably sort by the same category and compute the same offsets. These are sorting compositions, not dedicated output-buffer partition implementations. The checker verifies stable category order and every offset.

`compare implementations` measures one operation at a time; parallel executions apply only to full sorting. Typed Rust preparation admits only supported adapter/input pairs; every case's manifest records each measured column's route, the exact call it makes, and each omission, and reports list them under Capabilities. Tessera and the operation's serial denominator are required controls for every selected case, even in a restricted group; an unsupported control fails the run before it writes measurement journals.

```sh
bun run --silent tools plan smoke --operation select_nth --index 32768 \
  --types u64,row_u64 --distributions random,d16 --lengths 65536
bun run tools compare implementations --operation select_nth --index 32768 \
  --implementations std-stable,std-unstable,glidesort,radsort,tessera-full \
  --types u64,row_u64 --distributions random,d16 --lengths 65536 --cpu 2
bun run tools compare implementations --operation argsort \
  --implementations std-stable,std-unstable,glidesort,radsort \
  --types u64,record128 --distributions random --lengths 65536 --cpu 2
```

A rule selects an operation with, for example, `operation = { kind = "select_nth", index = 32768 }`. The bucket operation uses `bucket_count`. Non-sort plans require literal positive input lengths. The selection index must be a nonnegative integer smaller than every surviving input length, and the bucket count must be positive. Unrelated parameter flags are errors.

Revision plans can mix sorting and serial operations across rules, as the [PR plan](continuous-benchmarks.md#cases) does. An explicit `--operation` drops rules with their own operation and applies the requested operation to the remaining rules. Implementation comparisons require a single operation, because a report's sections and denominator belong to one; linked revision and counter studies are full-sort only. [Main tracking](continuous-benchmarks.md#main-long-term-tracking) measures every operation's serial rules in each serial column.

The timer covers each complete call, including scratch allocation, classification where applicable, and mandatory output construction. Argsort and bucket partition retain their newly allocated results until checking; discarding earlier results, restoring mutable inputs and verification are outside the clock. The prepared batch-memory cap includes mutable record/payload buffers and retained output; shared originals/references and transient call workspace are separate. Implementation comparisons use the same paired blocks and A/A statistic as sorting, and report time per input record, not per retained index, selected rank or bucket.

The separate allocation worker verifies one complete call for every admitted case and variant. Peak extra heap includes output: O(n) indices for argsort, O(R) offsets for bucket partition and no heap output for in-place selection. Memory rows retain peak extra heap and output bytes separately; reports estimate workspace by subtracting retained output bytes from peak extra heap.

The manifest keeps the exact operation and frozen plan. Non-sort implementation reports remain tables and JSON; the full-sort figures keep their cohorts and meaning. A non-sort report's speed table divides the operation's serial denominator's median time by each column's on the same case and states the denominator's route and stability; these speeds are not paired, so verdicts stay with the paired Tessera ratios in Full results.

## Local parallel comparisons

The `parallel` plan is an ordinary implementation comparison with the `rayon` feature; hosted comparisons use their own four-thread cases. Iterate with one representative input and one logical CPU per physical core (`lscpu -e=CPU,CORE`; on the Ryzen 7 8845H adjacent CPUs are SMT siblings):

```sh
bun run tools compare implementations --plan parallel \
  --types row_u64 --distributions random --lengths 10000000 \
  --cpu 0,2,4,6,8,10,12,14 --out /tmp/tessera-parallel
```

The plan measures `tessera` and `rayon-stable` at `t8` beside the serial controls; `--executions serial,t1,t2,t4` chooses other pool sizes. Each adapter owns a pool of its width, built and warmed outside timing and allocation counting; each sort includes `ThreadPool::install` and the public call. Workers inherit the affinity mask, and Linux thread checks allow only the pool's workers plus the caller. Oversubscribed runs are not scaling evidence.

The ratio is a column's time over paired serial Tessera time; its inverse is the whole-call speedup. The A/A range controls the serial baseline, not parallel variability, and there is no placement control. Peak extra heap excludes pool storage and thread stacks. Inputs below the parallel cutoff and one-worker pools only check the serial fallback.

## Native prerequisites

Rust competitors need Cargo's stable toolchain. Native competitors also need Linux x86-64 with glibc, Bun, Clang with C++17, CMake, GNU binutils and TBB headers (`libtbb-dev`, for declarations in the pinned IPS²Ra/IPS⁴o headers; nothing links TBB or OpenMP). The CLI fetches and verifies sources once; later builds are offline. Native builds use `clang`/`clang++` (override with `CC`/`CXX`; manifests record versions); published comparisons use the LLVM major version of `rustc -vV`.

`TESSERA_BENCH_CHECK_ONLY=1 cargo clippy --workspace --all-targets --all-features` (or `cargo doc`) checks without the native toolchain by omitting native compilation and link inputs; leave it unset for builds, tests and measurements.

## Resuming implementation comparisons

`compare implementations` retains one `compare`/`memory` executable pair under `bin/` before measuring. Schema-2 `session.json` records their hashes, the plan hash, CPU, group, block budget and sampling limits. A session interrupted before `session.json` exists was never prepared; start a new directory.

```sh
bun run tools compare implementations --plan release --cpu 2 --out bench/results/release
bun run tools compare resume bench/results/release   # same directory; no rebuild or overrides
```

Resume verifies retained files before executing them. Memory sorts, calibration and every timed batch check their outputs, so there is no separate validation pass; `validate` runs functional checks alone. The journal keeps groups with their full budget and complete memory rows, replaces interrupted groups and drops an incomplete final JSONL write. Changed plans or executables need a new session, and the host and build provenance are rechecked. One session writer is allowed: after an abrupt exit its lease must expire before another writer recovers, and a running worker keeps its kernel lock. A completed preparation or a failed measurement is not a completed experiment.

## Direct executable sessions

Use the binaries directly only to split groups across directories or debug the harness; omit `compare-native` and the fetch step for Rust-only opponents.

```sh
bun run tools native fetch
cargo build --release -p tessera-bench --features compare-native --target-dir target/compare-default
taskset -c 2 target/compare-default/release/compare run --plan release --out bench/results/release
bun run tools report bench/results/release
```

`compare list` and `compare validate --plan smoke|release` check support and correctness. Direct binaries default to nine blocks (`--blocks 3` for a short screen, for timing and memory alike); a retained manifest fixes the budget. `compare run` first runs the sibling `memory` executable, which repairs its journal and measures missing rows. `--group OPERATION/TYPE/DISTRIBUTION[/PARAMS]` (for example `sort/u64/random`) selects one group; a repeated run skips completed groups and replaces an interrupted one. A changed executable, provenance, plan or host fingerprint needs a new directory. Directories with matching hosts and manifests report together (`bun run tools report DIR1 DIR2`, written to the first). The report needs the complete plan unless `--partial` is given, and writes `bmf.json` for the Bencher CLI.

Every build uses the SIMD backend. Revisions without it, dual-build results and schema-1 sessions are rejected; report them with their original tooling revision.

## Cases and plans

### Case identities

A case is one operation on one input; a column is one implementation at one execution. [`catalog.ts`](../../bench/catalog.ts) and [`plan.rs`](../../bench/rust/src/plan.rs) own the formatters and decoders, checked against a shared fixture:

| Identity | Grammar | Example |
|---|---|---|
| Case | `operation/type/distribution/length/s1[/params]` | `select_nth/u64/random/65536/s1/32768` |
| Group | `operation/type/distribution[/params]`: all lengths of one input | `sort/u64/random` |
| Execution | `serial`, or `tN` for the parallel entry point in an N-worker pool; `t1` is not `serial` | `t4` |
| Column | `implementation/execution` | `rayon-stable/t4` |
| Criterion ID | `rust/case/execution` | `rust/sort/u64/random/65536/s1/serial` |

Results with earlier IDs regenerate with their original tooling revision.

### Input coverage

The release plan has 381 inputs: the original 260-case cross-type matrix and 121 cases using the high-value distributions from continuous tracking; `smoke` validates the same 125 type/distribution pairs at 1,000 records.

| Type | Task | Representation |
|---|---|---|
| `u32`, `u64` | keys | Unsigned integers |
| `f64` | keys | Arbitrary float bits, ordered by `total_cmp` |
| `reverse_u64`, `reverse_f64` | keys (opt-in) | `Reverse<u64>`, `Reverse<f64>` through the direct-key API |
| `row_u64` | rows | `{u64 key, u32 row}`, C layout, 16 bytes |
| `record128` | records | `{u64 key, [u8; 120]}`, C layout, 128 bytes |
| `owned` | owned records | `{u64 key, String}`, 16-character payload |
| `row_u32` | supplementary rows | `{u32 key, u32 row}`, C layout, 8 bytes |

Payloads encode the original index and fill every remaining byte deterministically; verification checks complete payloads. The ten baseline distributions (`random`, `d16`, `d256`, `zipf1.1`, `zipf2`, `skew99`, `sorted`, `reverse`, `sorted99`, `runs`) run on all seven types at 1,000, 65,536, 1,000,000 and 10,000,000 records (`record128` and `owned` stop at 1,000,000) and give rankings one common input set. The release plan adds:

| Distributions | What they distinguish | Types | Lengths |
|---|---|---|---|
| `equal`, `two_values`, `d4096`, `hot50` | Constant keys, separated ties, intermediate cardinality, a repeated key beside a high-cardinality remainder | All seven | 65,536; 1M |
| `far_swaps`, `sorted_tail` | Long-distance disorder; an ordered prefix with an outlier tail | All seven | 65,536; 1M |
| `sparse8` | Eight full-width keys | `u32`, `u64`, `row_u32`, `row_u64` | 65,536; 1M |
| `sparse_digits`, `bimodal` | Sparse digit occupancy; two distant high-cardinality clusters | `u64`, `row_u64`, `record128`, `owned` | 65,536; 1M |
| `finite_signed` | Finite signed floats with bounded magnitudes | `f64` | 1,000; 65,536; 1M; 10M |

All nine added integer distributions also run on `u64` at 10M. Type restrictions keep each generator meaningful (narrowing `bimodal` to `u32` would discard its cluster bit). Memory is measured for every supported variant on inputs up to 128 MiB of records, which includes `u64` at 10M; serial Tessera's peak is also measured on larger inputs to size `glidesort-sqrt`, and parallel columns on every selected case.

### Input generation

`case.rs` owns the seed-one generator shared by all executables and Criterion; tests pin every distribution's seed-one input hash, and float validation covers signed zeros, infinities, subnormals and NaNs of both signs with distinct payloads. The source is SplitMix64 from state 1: each draw adds `0x9e3779b97f4a7c15` and applies the xor-shift/multiply finalizer (shifts 30, 27, 31; multipliers `0xbf58476d1ce4e5b9`, `0x94d049bb133111eb`) in wrapping 64-bit arithmetic. `u32` takes the low 32 bits, `u64` the draw, `u128` two draws high word first, `f64` the draw as bits.

The opt-in `Reverse` inputs wrap the base generator's `!key` (`u64`) or `-key` (`f64`) in `Reverse`. Both the transformation and `Reverse` complement the encoded key, so every position keeps the base input's encoded key, shape and bit geometry in a different direct-key representation. Float identity checks compare every bit, including zero and NaN signs and payloads.

- `random` draws each key over its full range; `dD` takes `next mod D` (D = 16, 256, 4,096).
- `zipf2` selects ranks `k − 1` for k = 1…n by cumulative integer weights `floor(2^62 / k²)`; `zipf1.1` uses `floor(2^48 / (k · floor(256 · k^(1/10))))`, with the tenth root by integer binary search. A draw selects `floor(next · total / 2^64)` and the first cumulative weight above it.
- `skew99` uses ten spread values `v_k = k · floor(MAX / 10)` (floats: `k · 2^1020`): a first draw selects `v_0` with 99% probability, else a second selects `v_(1 + next mod 9)`. `two_values` picks `v_0` and `v_9` equally; `equal` repeats one value.
- `sorted` is 0…n−1; `reverse` reverses it; `sorted99` swaps adjacent elements at spacing `max(2, floor(n / max(1, floor(n / 100))))`.
- `runs` draws random keys, then sorts consecutive runs of `16 + next mod 2048` keys (last run shorter), each reversed if the next draw's high bit is set.
- `sparse8` draws eight full-width keys and picks uniformly; `sparse_digits` masks a draw with `0x5555_5555_5555_5555`; `bimodal` sets bit 63 from one draw and bits 39…0 from the next.
- `hot50` takes zero with 50% probability and `draw_high` otherwise (integers: top bit set on a full-width draw; `f64`: positive NaN payloads or infinity).
- `far_swaps` starts ascending and swaps `max(1, len / 100)` pairs (for len > 1), one position in each end quarter, so each swap spans at least half the array. `sorted_tail` is 0…`len - len / 128 - 1` followed by `len / 128` `draw_high` draws.
- `finite_signed` draws an `f64` sign, an exponent field in 1000…1029 and a random mantissa: magnitudes from 2^-23 to below 2^7, no NaN or infinity.

## Implementations

Rust competitors are pinned dependencies of the benchmark package, not the library. Native sources are pinned by repository, commit, archive SHA-256 and license in `native/sources.toml`, verified by `bench/rust/native/fetch.ts` and the build, and kept in the ignored `bench/vendor/`. The [catalog](../../bench/catalog.toml) owns variant identities, stability and names; adapters in [variant.rs](../../bench/rust/src/comparison/variant.rs) own capabilities and routes, which the runner's `list` reports for the selected operation as tab-separated column, stability, the types taking a route, and that route, one row per distinct route.

Unsupported tasks: Voracious 1.2.0 records (its stable path fails on duplicate keys) and small floats (partial comparison); VQSort and x86-simd-sort floats (no NaN-payload and signed-zero total order) and rows (different pair layouts). The DuckDB adapter pins the v1.4.0 headers from [Sorting, Again](https://duckdb.org/2025/09/24/sorting-again) (Vergesort over Ska Sort with pdqsort fallback): an **unstable, single-threaded in-memory kernel** on raw integer keys and records, without SQL key encoding, external merging, cancellation checks, floats or owned strings.

Native sorts compare keys in C or C++; each FFI call sorts a complete buffer, the clock stays in Rust, and record sizes, alignments and payload offsets are checked across the boundary. Native code uses `-O3 -DNDEBUG -march=x86-64 -mtune=generic` (x86-simd-sort's dispatch objects keep their upstream ISA targets); IPS²Ra and IPS⁴o are built without parallel libraries, and the runner checks the thread count after each sample. On keys, stable and unstable sorts do the same task; on records, unstable results are labelled separately and never enter the stable ranking or grid.

## Timing and statistics

For every sample the runner restores independent buffers outside the timed region, clocks only the batch of sort calls, then verifies every output outside the clock. Stable variants must match the standard stable reference exactly; unstable records must be ordered, contain each original index once and preserve every payload.

Calibration sizes a batch from one verified warm-up sort, a minimum sample duration (default 5 ms) and a prepared-buffer cap (256 MiB), with at least one buffer. Input and stable reference are generated once per case and shared; only one variant's buffers are resident at a time, so working sets differ with speed. Scratch allocation is inside the timed call.

Each case has its declared budget: three blocks for a short screen, nine for an extended run (the [catalog](../../bench/catalog.toml) owns budgets and direction-agreement counts). Each block runs all variants and two Tessera calls in a deterministic shuffled order; the second Tessera sample is `tessera-aa`. All blocks reuse the seed-one input, so the A/A range measures execution variability on that input, not across inputs.

A variant's per-block ratio is its time per sort over the mean of the two Tessera times. The report gives the median ratio, per-block extremes and median ns per element. The A/A noise range spans both Tessera times over their block mean, across blocks. A variant is faster or slower only when all three short-screen blocks (or at least eight of nine) agree on direction and its median lies outside that range; otherwise it is inconclusive. The range is not a confidence interval: controls of 100 and 120 give the range 100/110 to 120/110, so an opponent at 109 is within noise. `tessera-aa` reports A2/A1 separately.

The standard report shows the 1M-key ranking, 1M and 1,000-key distribution comparisons, the 1M element-type comparison, 1M per-type panels and 10M-key peak heap. Rankings and cross-type geometric means use the ten baseline distributions. Key rankings admit stable and unstable opponents, record rankings stable ones only. Speed panels anchor to `slice::sort` within the same blocks; [the figure guide](benchmark-figures.md) owns encoding. Native-CPU builds, branch alignment and cold-cache studies are outside the standard report.

## Results and provenance

```text
DIR/
  plan.json                       normalized experiment selection
  session.json                    immutable settings and executable hashes
  bin/compare, memory            retained timing/memory executable pair
  manifest.json                   timing build and complete case plan
  memory-manifest-default.json    memory executable's build provenance
  host.json                       machine fingerprint
  blocks.jsonl                    verified samples and group observations
  memory.jsonl                    verified peak extra allocation sizes
  bmf.json                        report-generated Bencher summaries
  report/                         index.md, summaries.json and *.svg figures
```

Manifests record the source revision and dirty state, source and executable SHA-256, `rustc -vV`, lockfile, features, target, profile, optimization, debug setting and flags; native manifests add compiler versions, source pins, licenses and translation-unit flags. Each group records load, frequency, temperature and memory pressure at start and end. Memory and timing executables must agree on source, budget, toolchain, target, profile, features and compilers. The report rejects incompatible manifests or hosts, unverified or duplicate samples, missing blocks, incomplete groups and missing memory samples, and flags load, frequency or temperature deviations from the session median.

## Memory accounting

The memory executable measures one call after input and reference are allocated. Rust-only builds count requested bytes through a global allocator. Native builds count usable sizes through wrapped `malloc`, `calloc`, `realloc`, `free`, `aligned_alloc` and `posix_memalign`, plus overridden C++ operators, since libstdc++'s internal calls bypass linker wrapping; a self-test checks both paths for double counting. Input buffers, owned payloads and stack storage are not counted. `glidesort-sqrt` allocates as many whole records as fit Tessera's measured peak for that case.

The [growth plan](../../bench/plans/growth.toml) declares the published memory sweeps: one include rule per operation, each on random `{u32, u32}` records at the same lengths. A sweep measures the plan's implementations that support its operation. Selection fixes zero-based rank 512, the middle of the shortest input, so every length selects the same rank; partitioning uses 16 buckets. Argsort's peak includes the index vector it returns, because the caller keeps it.

Published performance claims come from [main tracking](continuous-benchmarks.md), whose journals are kept on the `benchmark-data` branch; a local report supports a claim only with its complete results directory. Synthetic chart fixtures only test layout.
