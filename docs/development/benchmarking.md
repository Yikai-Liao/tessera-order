# Benchmarking

The comparison runner in `bench/rust` measures sorting algorithms on one machine. It answers four questions: how Tessera compares with the standard stable sort, with other stable sorts, with unstable sorts when equal keys are indistinguishable, and how much extra memory each sort uses. The [continuous suite](continuous-benchmarks.md) tracks regressions on Bencher; release comparisons use the paired measurements described here.

## Command capabilities

Run `bun run tools` from the repository root with the Bun version pinned in `package.json` and the Rust toolchain (`bun install --frozen-lockfile`). Bun runs the TypeScript sources directly; there is no Node CLI fallback or compilation step. Use `bun run --silent tools` to suppress the script banner when an agent reads stdout, and a task's `--help` for its options. [CI](ci.md#reproducing-locally) lists the non-benchmark commands.

| Task | Command | What it provides |
|---|---|---|
| Compare working-tree code with a commit | `bun run tools compare commit COMMIT` | Two revision builds, adjacent AB/BA sample pairs, verification, provenance and a report |
| Sample revision comparisons across layouts | `bun run tools compare commit COMMIT --layout-seeds 1,2,3,4,5` | One revision pair per fixed LLD seed and descriptive cross-layout ranges; Linux x86-64 GNU only |
| Compare fixed-work counters | `bun run tools compare counters COMMIT` | Verified serial batches with Callgrind instruction and simulated cache/branch counts |
| Probe local PMU access | `bun run tools compare probe-counters --out DIR` | Read-only, per-event `perf` availability on the current host |
| Compare linked source revisions | `bun run tools compare revisions BASE --current HEAD` | Serial revisions and independently named copies in one process, repeated-current A/A, verification, allocation and retained executables |
| Compare algorithms | `bun run tools compare algorithms` | Registered algorithms in one timing executable, three shuffled blocks by default, two Tessera calls for A/A, verification and allocation measurement |
| Local parallel comparison | `bun run tools compare algorithms --plan parallel --cpu 0,2,4,6,8,10,12,14` | Serial controls, Tessera-8 and Rayon stable-8 in the same harness; choose a CPU list for the host |
| Full algorithm comparison | `bun run tools compare algorithms --plan release` | Release coverage and native opponents |
| Resume an interrupted comparison | `bun run tools compare resume DIR` | Recorded selection, settings and exact retained builds; no rebuilding |
| Preview or customize inputs | `bun run --silent tools plan NAME` | Validated selection without compiling or measuring |
| Regenerate a report | `bun run tools report DIR` | Statistics and figures from retained results, without new measurements |
| Read main or PR feedback | `bun run --silent tools status main` or `bun run --silent tools status --pr NUMBER` | Exact-ref workflow and benchmark states; [read-only interpretation](continuous-benchmarks.md#read-feedback-from-a-pr) |

`compare commit` defaults to `HEAD` and the `local` plan; `compare algorithms` defaults to the `smoke` plan. Replace uppercase placeholders. `report` reads retained results without compiling or measuring; incomplete algorithm sessions require `--partial`, while revision reports require every case's complete recorded sample budget.

CLI benchmark builds default to 64-byte loop and non-fallthrough block alignment (`-C llvm-args=-align-loops=64`, `-C llvm-args=-align-all-nofallthru-blocks=6`) for both revisions, Rust competitors and memory workers. A dependency-free Cargo probe resolves environment, build and target configuration before the single benchmark build. The builder prepends the defaults to those resolved flags, so explicitly configured values override them, and records the effective flags in provenance. Direct Cargo commands keep their own configuration; hosted comparisons use the CLI defaults.

### Optional local AOT CLI

For frequent CLI calls, developers and agents can optionally compile a local executable with [Bun AOT](https://bun.com/docs/compiler/aot) after installing the pinned project dependencies. This further reduces startup and module-loading work; it does not speed up the Rust sorting kernels. The normal `bun run tools` entry runs current sources and does not require AOT.

AOT requires a Bun version that provides `bun compile`. Local validation used `1.4.4-canary.1+447ff870b`; stable `1.4.4` does not provide that command. AOT is experimental and builds for the host platform; validation so far covers Linux x86-64. With a compatible Bun on `PATH`, run from the repository root:

```sh
bun run tools:aot:build
./target/dev-tools/tessera-tools plan local
```

The build script embeds the source-relative TOML resources and forbids runtime code generation from strings, including indirect evaluation paths. Zod uses its existing regular parser without changing schemas or maintaining a runtime-specific validator. The executable still includes Bun support code; AOT does not mean runtime-free.

The binary is a snapshot: rebuild after changing CLI sources or imported TypeScript modules, dependencies, or embedded configuration files. There is no automatic freshness check or rebuild. When editing tooling, use `bun run tools` so that a stale binary cannot validate old selection or reporting logic.

This is an optional internal development artifact under the ignored `target/` directory, not a GitHub Release asset. CI uses the pinned stable Bun directly, never this local binary or the AOT canary toolchain. Offline plan, metadata and small real-report paths and dedicated process probes have been checked; real AOT build orchestration and larger-report coverage remain [open validation work](../../ROADMAP.md#benchmark-tooling-and-investigations). Do not implicitly reuse a local AOT binary in CI measurement tasks.

## Command output

Build and measurement tasks print one compact JSON object with `out`, `log` and `result`; subprocess output and progress go to the adjacent `OUT.log` (each `compare resume` writes its own `OUT.resume-TIMESTAMP-PID.log`), and a failed task exits nonzero with the log path. `--verbose` streams diagnostics instead and no longer guarantees JSON-only stdout. Completed revision and algorithm comparisons return the protocol, ratio denominator, interval meaning, measured counts, faster/slower/within-noise counts and up to five changes with numeric evidence (current regressions first for revision screens; opponents faster than Tessera first for algorithm and linked-revision comparisons) (`omittedChanges` counts the rest; `summaries.json` has everything). Algorithm counts describe opponent/input pairs and exclude the two Tessera controls; layout results stay separate per layout; preparation returns an identity and case count, not a verdict. Completed comparisons also return the recorded host affinity; `inherited` (revision screens) or a multi-CPU mask means no single-CPU pin was established by default. `report` returns the same summary without reading or rebuilding sources, and `plan` prints compact JSON or editable TOML.

A command's exit status describes execution, not a speedup: a successful `status` means the read succeeded, not that workflows passed. `status` prints one compact object for the exact `target.head` of a remote ref, keeps the latest workflow run per workflow/event/branch and lists only unfinished or failed jobs; `--details` adds full check summaries, the report comment and workflow links, and `--text` a short human overview. Neither triggers jobs or changes GitHub state; authenticate with `gh auth login` first. [PR feedback](continuous-benchmarks.md#read-feedback-from-a-pr) explains how to read it.

## Run an experiment

1. State the hypothesis, baseline or opponents, relevant inputs and expected unaffected controls. Preview the selection with the preset and overrides you intend to measure:

   ```sh
   bun run --silent tools plan local --types u64,row_u64 --distributions random,zipf2
   ```

   Use `local` for revision comparisons, `smoke` for a small algorithm comparison, `release` for full coverage.

2. Record host and compiler conditions. Choose a CPU allowed by the host affinity policy. Prefer an idle machine, performance governor, disabled turbo/SMT and a pinned logical CPU; obtain operator approval before changing governor, turbo or SMT settings. Native opponents also need the [native prerequisites](#native-prerequisites).
3. Run the command:

   ```sh
   # Revision comparison; HEAD~1 must have a compatible API and harness.
   bun run tools compare commit HEAD~1 --types u64,row_u64 --distributions random,zipf2 --cpu 2

   # Rust-only algorithm smoke comparison, including verification and reporting.
   bun run tools compare algorithms

   # Full algorithm comparison, including native opponents.
   bun run tools compare algorithms --plan release --cpu 2
   ```

   CPU 2 is an example, not a portable default. Both commands accept `--out DIR`, require a new directory and otherwise use `bench/results/<timestamp>`. Algorithm comparisons accept `--sample-ms` (default 5) and `--memory-mib` (default 256); choose them before starting. Keep raw results and research harnesses outside Git.

4. Retain the resolved plan, provenance, verification results and controls. Report tested coverage, environment, ratios, A/A ranges and result location, and name partial, interrupted and unmeasured coverage. Investigate changes outside the controls, including apparently untouched cases; do not dismiss them as noise or tune source layout to one executable.

## Additional source-attribution controls

Algorithm comparisons use the [paired timing contract](#timing-and-statistics) and report results for the measured inputs and environment. Attributing a gain to a source change rather than code placement additionally needs in-process revision comparisons and independent placement controls, as in the [engine evidence](../design/evidence.md#reading-small-case-results). Process-level `compare commit` and continuous alerts alone do not provide it.

`compare algorithms` compares registered implementations of the current source snapshot, not a baseline commit. `compare commit` alternates adjacent A/B samples of two executables without a separate A/A pass; `--layout-seeds` samples section ordering across builds but does not put revisions in one address space. Use the linked-revision command for that control.

For an unsupported experiment, identify the missing measurement capability and extend the maintained pipeline so verification, provenance and reporting stay shared. Start with a bounded prototype; confirm unusual resource costs or host-wide changes with the operator.

## Linked revision comparisons

`bun run tools compare revisions BASE --current HEAD` uses the algorithm pipeline with two named crates per serial revision: `tessera` and `tessera-copy` for current, `tessera-baseline` and `tessera-baseline-copy` for baseline. Repeated-current `tessera-aa` and `std-stable` are controls. Omit `--current` to use the materialized working snapshot. It uses the algorithm pipeline's axes, `--sample-ms`, `--memory-mib`, affinity, verification, block budget, allocation measurement, retention, resume and reports; it chooses its own adapters, rejects `--algorithms` and selects no parallel APIs.

The temporary workspace renames copied packages without rewriting Rust source and may add local lock entries only; external registry/Git entries must remain identical. Provenance retains resolved commits, original and generated source hashes and all four libraries' resolved features, checked against Cargo artifacts. Incompatible baselines fail before timing. Historical libraries enable their opt-in SIMD feature explicitly.

Ratios are **placement time / paired current time**: a slower baseline row indicates a current improvement. Inspect both baseline copies and current B, not just the favorable copy. Distinct entries are checked before measurement but do not prove distinct internal bodies: inspect the retained executable's symbols and disassembly before claiming source attribution, and reject folded paths as placement evidence. Repeated-current A/A bounds timing variability and same-source copies expose placement sensitivity; neither bounds all code placement.

## Working-tree comparisons

`compare commit [COMMIT]` and `compare algorithms` snapshot the current staged, unstaged and untracked sources without touching the checkout, build under `~/code/tessera-bench/` (`TESSERA_BENCH_WORK` overrides) and clean up temporary sources. Builds reuse `target/tessera-bench` in the checkout by default; `TESSERA_BENCH_TARGET_DIR` overrides it (relative paths resolve from the repository root). Remove that cache for a cold build, and never share it across trust boundaries. Internal symlinks are materialized; escaping or dangling links and directory cycles are rejected. Baseline links must resolve inside an archive of the selected commit.

`compare commit` accepts any resolvable local commit compatible with the current crate API, harness and lockfile. Both revisions use the current harness, lockfile and toolchain; the baseline substitutes only `rust/tessera-sort`. Linux workers run under `setarch -R` to control address randomization (checked before building). Selected timed IDs must match. The `local` plan measures only `rust/u64/random/65536/s1/api`; override its axes rather than implicitly running the full continuous inventory. `--filter` narrows the resulting IDs.

Fresh worker pairs measure adjacent A/B samples, reversing startup, warm-up and measurement order on every other pair. The catalog owns one warm-up and one sampling budget per revision, divided across samples, with no outer repetition. Input preparation repeats in each fresh process outside the clock. The report gives the median B/A ratio and its distribution-free 99% median interval; only an interval entirely outside 0.95–1.05 alerts. Nominal coverage assumes independent pairs and is per case, not family-wide. The [continuous protocol](continuous-benchmarks.md#within-job-scheduling) defines batching and clocks. This is a development screen, not an in-process or placement-controlled claim.

Output: `plan.json`, `local.json`, `summaries.json`, `index.md`, `local.svg` and the executed `binaries/`. `local.json` records revision and source hashes, executable hashes and paths, both builds' Cargo provenance, host information, selected IDs and observations; regeneration uses the retained counts. Downloaded executables are untrusted code, and JSON-only regeneration never runs them. Interrupted comparisons retain completed cases but cannot be resumed or reported as complete. `continuous prepare`, `continuous measure` and its `--shard` are described under [local use of continuous work](continuous-benchmarks.md#local-use).

Standard algorithm-report figures show fixed slices and aggregates, not every case; custom lengths omitted from them remain in Full results and `summaries.json`. The [figure guide](benchmark-figures.md) owns presentation and the [tooling design](../design/benchmark-tooling.md) the module boundaries.

### Layout sampling

`compare commit --layout-seeds 1,2,3,4,5` investigates sensitivity to function-section ordering. It requires Linux x86-64, the `x86_64-unknown-linux-gnu` target, a GNU-compatible `cc` driver and LLD. Seeds are distinct integers in 1..4294967295 (zero means random to LLD) applied as `--shuffle-sections=.text.*=<seed>` through final benchmark rustc arguments. Builds are a separate local configuration, not a reproduction of continuous flags. Linker versions, final arguments and both revisions' link maps are retained; inspect the maps to confirm the functions under investigation moved, since distinct hashes alone do not show it.

Calibrate on identical sources before comparing an engine change, with sensitive small inputs and large random controls:

```sh
bun run tools compare commit HEAD --types u32,u64 --distributions random --lengths 48,65536 \
  --layout-seeds 1,2,3,4,5 --out bench/results/layout-calibration
```

Each seed runs the adjacent-pair protocol sequentially, so work grows linearly with the seed count. Reports keep each seed's paired median interval and verdict, then the median and min/max of per-seed median ratios and each revision's time spread; per-case `layout-N.svg` figures show absolute medians and observed ranges. Ranges are not confidence intervals. Equal seeds need not give matching addresses across revisions, a few favorable seeds cannot establish attribution, and section shuffling does not vary block order inside a function or CGU/inlining choices. `--layout-control cgu1`, `aligned` or `engine-first` (engine-owned out-of-line functions placed first via `nm`) add a further control, retained with flags, maps and `.order` files; none establishes an isolated gain.

### Fixed-work counters

`compare counters [COMMIT]` uses the smoke harness with the local-only `layout-study` feature. It requires Linux x86-64 and Valgrind (simulated ISA dispatch can differ from the host):

```sh
bun run tools compare counters HEAD --types u32,u64 --distributions random --lengths 48,65536 \
  --sorts 8 --out bench/results/counters-null
```

Each serial API case runs in three processes: baseline, a second baseline for A/A, and current (order reversed on alternate cases). Each verifies a complete sort, prepares `--sorts` copies (default 8, range 1..1024), calls the `tessera_counted_sorts` marker once and verifies outputs afterwards; Callgrind collects that call tree with cache and branch simulation. `counters.json` retains source and executable hashes, flags, Valgrind version, sort count, event totals and whether the working tree was modified. `Ir` is instructions, `D1mr`/`D1mw` and `DLmr`/`DLmw` simulated data-cache read/write misses, `Bcm`/`Bim` simulated mispredictions. Counts describe this prepared-cache workload and simulator, not cold-cache or hardware events; equal counts cannot prove a time change is layout, and changed counts need not explain it. The report is descriptive and changes no verdict.

Hosts whose libc needs instructions Valgrind lacks can use the baseline-ISA container (`docker build -t tessera-callgrind -f bench/local/Callgrind.Dockerfile .`, then `--valgrind-image tessera-callgrind`); it records the image ID, mounts sources read-only and runs without network access.

`compare probe-counters --out DIR` separately tries `instructions:u`, `branch-misses:u`, `idq.dsb_uops:u`, `idq.mite_uops:u` and `baclears.any:u` with the host's `perf` and preserves each result in `probe.json`. It changes no `perf_event_paranoid` setting and infers nothing about cloud hosts.

## Experiment plans

Plans in [`bench/plans/`](../../bench/plans/) are schema-1 TOML (repository presets) or TOML/JSON (custom files) and decode into one validated model. They describe what to measure; the harness owns input generation and the protocol.

| Preset | Harness | Default selection |
|---|---|---|
| `local` | `smoke` | Tessera, random `u64`, 65,536 records, `api` |
| `continuous` | `smoke` | Regression inputs and timed IDs from the [continuous plan](../../bench/plans/continuous.toml), see the [cases](continuous-benchmarks.md#cases) |
| `smoke` | `comparison` | Rust competitors, 125 inputs at 1,000 records |
| `release` | `comparison` | Rust and native competitors, 381 inputs across four lengths |
| `parallel` | `comparison` | Serial controls, Tessera-8 and Rayon stable-8; 16 inputs at 1M records |

A plan is a union of `include` products followed by `exclude` products:

```toml [my-plan.toml]
schema = 1
harness = "comparison"
algorithms = ["glidesort", "radsort"]

[[include]]
types = ["u32", "u64"]
distributions = ["random", "zipf2"]
lengths = [65_536, 1_000_000]

[[exclude]]
types = ["u32"]
distributions = ["zipf2"]
lengths = [1_000_000]
```

Values within an axis are alternatives; axes within a rule form a Cartesian product; included inputs are measured once; any matching exclusion removes a combination. Omitted axes use the catalog defaults when including and match everything when excluding. Empty choices, unknown fields and unknown identities are errors. The axes are `types`, `distributions` and `lengths` (positive record counts, or `"insertion-max"`, `"insertion-next"` and `"tile-next"`, resolved from the measured library's thresholds and the record width). Case IDs keep the resolved number, and a commit comparison rejects different IDs between revisions.

The `smoke` harness measures Tessera through its public functions; its types and distributions are under `smoke` in [the catalog](../../bench/catalog.toml), those of `comparison` under `comparison`. Criterion IDs end in `/api`; optional `parallel` rules in a smoke plan select four-thread `/par-4` inputs on the same axes when built with Rayon. The plan is the only runtime parallel-selection authority; no parallel-enable environment variable is needed. Comparison plans reject `parallel` rules. Inputs always use seed 1, and a plan that selects nothing fails before timing. Algorithm comparisons always retain `tessera`, `std-stable` and the Tessera A/A control; native algorithms enable source acquisition and native compilation. Unsupported adapter/type pairs are omitted from the manifest's capabilities, not recorded as measurements.

```sh
bun run --silent tools plan local --types u32,u64 --distributions random,zipf2           # preview
bun run --silent tools plan local --types u32,u64 --format toml > my-plan.toml           # save, then reuse:
bun run tools compare commit main --plan my-plan.toml
bun run tools compare algorithms --plan smoke --algorithms glidesort,radsort \
  --types u64 --distributions random,zipf2 --lengths 1234,65536                  # override
bun run --silent tools plan --schema > benchmark-plan.schema.json                         # editor schema
```

`--plan` takes a preset name or a `.toml`/`.json` path. Comma-separated CLI axes replace that axis in every inclusion rule; exclusions stay. Only `compare algorithms` and `plan` accept `--algorithms`, replacing the opponent list before the controls are added. Revision and counter experiments accept smoke-harness plans (`local` or `continuous`); algorithm and linked-revision experiments accept comparison-harness plans (`smoke`, `release` or `parallel`). `--group` runs one resumable group and marks the report partial. Every result retains the normalized `plan.json`.

## Local parallel comparisons

The `parallel` plan shares preparation, verification, block budget, serial A/A, memory worker, resume and reports with other algorithm comparisons and enables the `rayon` feature. Hosted comparisons use their own four-thread cases instead. Iterate with one representative input and one logical CPU per physical core (`lscpu -e=CPU,CORE`; on the Ryzen 7 8845H, adjacent CPU pairs are SMT siblings):

```sh
bun run tools compare algorithms --plan parallel \
  --types row_u64 --distributions random --lengths 10000000 \
  --cpu 0,2,4,6,8,10,12,14 --out /tmp/tessera-parallel
```

The defaults are `tessera-par-8` and `rayon-stable-8` plus the serial controls; one-, two- and four-worker adapters are explicit `--algorithms` choices. Each adapter owns a pool of the recorded width, built and warmed outside timing and allocation counting; each sort includes `ThreadPool::install` and the full public call. Workers inherit the affinity mask and are not individually pinned. Linux thread checks permit only the selected pool's workers plus the caller. Enough allowed CPUs and physical cores are the operator's responsibility; oversubscribed runs are not scaling evidence.

The report's ratio is variant time over paired serial Tessera time, its inverse the whole-call speedup. The A/A range controls the serial baseline, not parallel-only variability, and the plan provides no independent placement control. Peak extra heap excludes pool storage and thread stacks. Inputs below the parallel cutoff and one-worker pools exercise the serial fallback and check integration only.

## Native prerequisites

Rust competitors need Cargo's stable toolchain. Native competitors additionally need Linux x86-64 with glibc, Bun, Clang with C++17, CMake, GNU binutils and TBB headers (`libtbb-dev`; the pinned IPS²Ra/IPS⁴o headers include TBB declarations, but nothing links TBB or OpenMP). The CLI fetches and verifies sources before building; later builds use them offline.

For static checks without the native toolchain, use `TESSERA_BENCH_CHECK_ONLY=1 cargo clippy --workspace --all-targets --all-features` or the matching `cargo doc`. This omits native compilation and linker inputs; leave it unset for builds, tests and measurements that link native competitors.

Native builds default to `clang` and `clang++`; for published comparisons use the LLVM major version reported by `rustc -vV`. `CC` and `CXX` select another compiler, and manifests record the actual versions.

## Resuming algorithm comparisons

`compare algorithms` retains one `compare` / `memory` executable pair under `bin/` before measuring. Schema-2 `session.json` records their hashes, the plan hash, CPU, group, block budget and sampling limits; preparation removes its temporary tree only afterwards. A session interrupted before `session.json` exists is not prepared; start a new directory.

```sh
bun run tools compare algorithms --plan release --cpu 2 --out bench/results/release
bun run tools compare resume bench/results/release   # same directory; no rebuild or overrides
```

Resume verifies retained files before executing them. Memory sorts, calibration and every timed batch check their outputs; there is no separate automatic validation pass or validation receipt. The direct `validate` command remains available for functional checks without a full measurement. The Rust journal keeps groups with their full budget and complete memory rows, replaces interrupted groups and drops an incomplete final JSONL write. Changed plans or executables need a new session; Rust also rechecks the host and build provenance. Only one session writer is allowed: after an abrupt parent exit its lease must expire before another CLI writer can recover, and a still-running worker keeps its kernel lock. A completed preparation or a failed/cancelled measurement is not a completed experiment.

## Direct executable sessions

Use the binaries directly only to split groups across directories or debug the harness; omit `compare-native` and the fetch step for Rust-only opponents.

```sh
bun run tools native fetch
cargo build --release -p tessera-bench --features compare,compare-native --target-dir target/compare-default
taskset -c 2 target/compare-default/release/compare run --suite release --out bench/results/release
bun run tools report bench/results/release
```

`compare list` and `compare validate --suite smoke|release` check support and correctness. Direct binaries default to nine blocks (`--blocks 3` for a short screen, passed to timing and memory alike); a retained manifest fixes the budget. `compare run` first invokes the sibling `memory` executable, which repairs its journal and measures missing rows; `memory` also runs alone. `--group TYPE/SHAPE` selects one group (for example `u64/random`); a repeated run skips completed groups and replaces an interrupted one. A change to the executable, build provenance, suite or host fingerprint needs a new directory. Groups measured in separate directories report together when host fingerprints and manifests agree (`bun run tools report DIR1 DIR2`; the first directory receives `report/`). The report needs the complete suite unless `--partial` is given, and writes `bmf.json` for the Bencher CLI.

Every build uses the SIMD backend; no option or feature selects it. Revisions without a supported backend, dual-build results and schema-1 sessions are rejected; recover them with their original source revision.

## Cases and suites

Case IDs are `type/distribution/length/s1`. A group is all lengths of one type and distribution. The release suite has 381 inputs: the original 260-case cross-type matrix and 121 cases using the high-value distributions from continuous tracking; `smoke` validates the same 125 type/distribution pairs at 1,000 records.

| Type | Task | Representation |
|---|---|---|
| `u32`, `u64` | keys | Unsigned integers |
| `f64` | keys | Arbitrary float bits, ordered by `total_cmp` |
| `row_u64` | rows | `{u64 key, u32 row}`, C layout, 16 bytes |
| `record128` | records | `{u64 key, [u8; 120]}`, C layout, 128 bytes |
| `owned` | owned records | `{u64 key, String}`, 16-character payload |
| `row_u32` | supplementary rows | `{u32 key, u32 row}`, C layout, 8 bytes |

Payloads encode the original index, and wide payloads fill every remaining byte deterministically; verification checks complete payloads. The ten baseline distributions (`random`, `d16`, `d256`, `zipf1.1`, `zipf2`, `skew99`, `sorted`, `reverse`, `sorted99`, `runs`) run on all seven types at 1,000, 65,536, 1,000,000 and 10,000,000 records (`record128` and `owned` stop at 1,000,000) and give rankings one common input set. The release plan adds:

| Distributions | What they distinguish | Types | Lengths |
|---|---|---|---|
| `equal`, `two_values`, `d4096`, `hot50` | Constant keys, separated ties, intermediate cardinality, a repeated key beside a high-cardinality remainder | All seven | 65,536; 1M |
| `far_swaps`, `sorted_tail` | Long-distance disorder; an ordered prefix with an outlier tail | All seven | 65,536; 1M |
| `sparse8` | Eight full-width keys | `u32`, `u64`, `row_u32`, `row_u64` | 65,536; 1M |
| `sparse_digits`, `bimodal` | Sparse digit occupancy; two distant high-cardinality clusters | `u64`, `row_u64`, `record128`, `owned` | 65,536; 1M |
| `finite_signed` | Finite signed floats with bounded magnitudes | `f64` | 1,000; 65,536; 1M; 10M |

All nine new integer distributions also run on `u64` at 10M. Type restrictions preserve the generators' meaning (narrowing `bimodal` to `u32` discards its cluster bit). Memory is measured for every supported variant through 1M and for all `u64` cases at 10M; serial Tessera's peak is also measured for larger cases to size `glidesort-sqrt`, and selected `tessera-par-*` adapters measure every selected length and type.

### Input generation

`case.rs` owns the seed-one generator shared by all executables and Criterion; tests pin every distribution's seed-one input hash, and float validation covers signed zeros, infinities, subnormals and NaNs of both signs with distinct payloads. The source is SplitMix64 from state 1: each draw adds `0x9e3779b97f4a7c15` and applies the xor-shift/multiply finalizer (shifts 30, 27, 31; multipliers `0xbf58476d1ce4e5b9`, `0x94d049bb133111eb`) in wrapping 64-bit arithmetic. `u32` takes the low 32 bits, `u64` the draw, `u128` two draws high word first, `f64` the draw as bits.

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

Rust competitors are optional dependencies behind `compare`, pinned to exact versions. Native sources are pinned by repository, commit, archive SHA-256 and license in `native/sources.toml`; `bench/rust/native/fetch.ts` and the build verify them, and trees stay in ignored `bench/vendor/`. The [catalog](../../bench/catalog.toml) owns variant identities, stability and names; adapters in [variant.rs](../../bench/rust/src/comparison/variant.rs) own capabilities, so use the runner's `list` rather than a second matrix.

Voracious 1.2.0's record path fails the stable-reference check on duplicate keys and its float path uses partial comparison for small inputs; those tasks are unsupported. VQSort and x86-simd-sort's floating-point interfaces lack the required NaN payload and signed-zero total order, and their pair interfaces use different layouts than the row tasks. The DuckDB adapter pins the v1.4.0 headers from [Sorting, Again](https://duckdb.org/2025/09/24/sorting-again) (Vergesort over Ska Sort with pdqsort fallback). It is an **unstable, single-threaded in-memory kernel**, not an end-to-end SQL measurement: it sorts raw integer keys and complete records, excludes SQL key encoding, external merging and cancellation checks, and supports no float or owned-string tasks.

Native sorts compare keys inside C or C++; each FFI call sorts a complete buffer, the clock stays in Rust, and record sizes, alignments and payload offsets are checked across the boundary. Native code uses `-O3 -DNDEBUG -march=x86-64 -mtune=generic` (x86-simd-sort's dispatch objects use their upstream ISA targets); IPS²Ra and IPS⁴o are built without parallel libraries and the runner checks the thread count after each sample. On keys, stable and unstable sorts perform the same task; on records, unstable results are labelled separately and never enter the stable ranking or grid.

## Timing and statistics

For every sample the runner restores independent buffers outside the timed region, clocks only the batch of sort calls, then verifies every output outside the clock. Stable variants must match the standard stable reference exactly; unstable records must be ordered, contain each original index once and preserve every payload.

Calibration picks a batch from one verified warm-up sort, a minimum sample duration (5 ms by default) and a prepared-buffer cap (256 MiB), using at least one buffer even above the cap. The input and stable reference are generated once per case and shared immutably; one variant's buffers are resident at a time, so working sets differ with speed. Scratch allocation occurs inside the timed call.

Each case has its declared budget: three blocks for a short screen, nine for an extended run ([catalog](../../bench/catalog.toml) owns budgets and direction-agreement counts). Each block shuffles all variants and two Tessera calls into a deterministic order; the second Tessera sample is `tessera-aa`. All blocks reuse the same seed-one input, so the A/A range measures execution variability on that input, not across random inputs.

A variant's per-block ratio is its time per sort divided by the mean of the two Tessera times. The report gives the median ratio, per-block minimum and maximum, and median ns per element. The A/A noise range is the minimum and maximum of both Tessera times over their block mean, across the blocks; it is unchanged if the control labels swap. A variant is faster or slower only when all three short-screen blocks (or at least eight of nine) agree on direction and its median is outside that range; otherwise the outcome is inconclusive, and the range is not a confidence interval. For example, controls of 100 and 120 give mean 110 and range 100/110 to 120/110, so an opponent at 109 is within noise. `tessera-aa` separately reports A2 / A1 with its own bounds.

The standard report shows the 1M-key ranking, 1M and 1,000-key distribution comparisons, the 1M element-type comparison, 1M per-type input panels and 10M-key peak heap. Rankings and cross-type geometric means use the ten baseline distributions; per-type panels show every selected distribution at 1M. Keys admit stable and unstable opponents, record rankings stable only. Speed panels anchor to `slice::sort` within the same blocks. Every figure includes its plotted values; [the figure guide](benchmark-figures.md) owns encoding. The release comparison covers lengths of at least 1,000 with the default target; native-CPU builds, branch alignment and cold-cache investigations are separate work.

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

Manifests record the source revision and dirty state, source and executable SHA-256, `rustc -vV`, Cargo lockfile, features, target, profile, optimization level, debug setting and encoded flags; native manifests add compiler versions, source pins, licenses and translation-unit flags. A group's start and end observations record load, frequency, temperature and memory pressure. The memory and timing executables must agree on source, block budget, toolchain, target, profile, features and compilers. The report rejects incompatible manifests or hosts, unverified or duplicate samples, missing blocks, incomplete groups and missing required memory samples; it flags load, frequency or temperature deviations from the session median. Historical multi-build results need their original tooling revision.

## Memory accounting

The memory executable measures a single sort after input and reference are allocated. Rust-only builds count requested bytes through a global allocator; native builds count allocator usable sizes (including rounding) through wrapped `malloc`, `calloc`, `realloc`, `free`, `aligned_alloc` and `posix_memalign`, plus overridden C++ allocation operators because libstdc++'s internal calls bypass linker wrapping. A self-test checks both paths for double counting. The count excludes input buffers and owned payloads, and fixed stack storage is unmeasured. `glidesort-sqrt` allocates a buffer of the whole number of records that fits Tessera's measured peak for that case.

Release archives include the complete results directory, and published performance claims come from that archive; synthetic chart fixtures are layout tests. The first release comparison remains on the [roadmap](../../ROADMAP.md).
