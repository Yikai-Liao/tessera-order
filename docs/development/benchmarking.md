# Benchmarking

The comparison runner in `bench/rust` measures sorting algorithms on one machine. It answers four questions: how Tessera compares with the standard stable sort, with other stable sorts, with unstable sorts when equal keys are indistinguishable, and how much extra memory each sort uses. The [continuous suite](continuous-benchmarks.md) tracks regressions on Bencher; release comparisons use the paired measurements described here.

## Command capabilities

Run `pnpm tools` from the repository root with Node.js 24, the pinned pnpm and Rust toolchains. Install JavaScript dependencies with `pnpm install --frozen-lockfile`.

CLI benchmark builds default to 64-byte loop and non-fallthrough block alignment (`-C llvm-args=-align-loops=64`, `-C llvm-args=-align-all-nofallthru-blocks=6`) for both revisions, Rust algorithm competitors and memory workers. The shared builder first lets Cargo resolve [its flag sources](https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags), then prepends the defaults to those resolved flags and rebuilds when needed; explicitly configured alignment values occur later and can override the defaults. This can add a compilation when the initial build lacks alignment. Retained provenance records the effective flags. Direct Cargo commands retain their own configuration; hosted continuous comparisons use the same CLI alignment defaults.

| Task | Command | What it provides |
|---|---|---|
| Compare working-tree code with a commit | `pnpm tools compare commit COMMIT` | Two revision builds, adjacent AB/BA sample pairs, verification, provenance and a report |
| Sample revision comparisons across layouts | `pnpm tools compare commit COMMIT --layout-seeds 1,2,3,4,5` | One revision pair per fixed LLD seed, adjacent sample pairs, link maps and descriptive cross-layout ranges; Linux x86-64 GNU only |
| Compare fixed-work counters | `pnpm tools compare counters COMMIT` | Verified serial batches, Callgrind instruction and simulated cache/branch counts, source/build provenance and offline reporting |
| Inspect adjacent main changes | `pnpm tools continuous history --project PROJECT --out DIR` | Read-only existing Testbed histories restricted to each latest report's hardware spec, with retained responses for offline reporting |
| Probe local PMU access | `pnpm tools continuous probe-counters --out DIR` | Read-only, per-event `perf` availability on the current host |
| Compare algorithms | `pnpm tools compare algorithms` | Registered algorithms in one timing executable per configuration, nine shuffled blocks, two Tessera calls for A/A, output verification, allocation measurement and a report |
| Local parallel comparison | `pnpm tools compare algorithms --plan parallel --cpu 0,2,4,6,8,10,12,14` | Serial controls, Tessera-8 and Rayon stable-8 in the shared paired harness; choose a CPU list appropriate to the host |
| Full algorithm comparison | `pnpm tools compare algorithms --plan release --simd` | Release coverage, native opponents and a separate Portable SIMD build |
| Resume an interrupted algorithm comparison | `pnpm tools compare resume DIR` | Recorded selection, settings and exact retained builds; no rebuilding |
| Preview or customize inputs | `pnpm -s tools plan NAME` | Validated selection without compiling or measuring |
| Regenerate a report | `pnpm tools report DIR` | Statistics and figures from retained results, without new measurements |
| Read current main or PR feedback | `pnpm -s tools status main` or `pnpm -s tools status --pr NUMBER` | Exact-ref workflow and benchmark states; [read-only interpretation](continuous-benchmarks.md#read-feedback-from-a-pr) |

`compare commit` defaults to `HEAD` and the `local` plan; `compare algorithms` defaults to the smoke algorithm plan. Replace uppercase placeholders. The [CLI reference](tooling.md) defines output, logs and CI commands. Use [plans and axis overrides](#experiment-plans) for custom coverage. `report` reads retained results without compiling or measuring; incomplete algorithm sessions require `--partial`, while revision reports require every case’s complete recorded sample budget (or all nine blocks for legacy results).

## Run an experiment

1. State the hypothesis, baseline or algorithm opponents, relevant inputs and expected unaffected controls. Preview the selected plan and axis overrides before an expensive run:

   ```sh
   pnpm -s tools plan local --types u64,row_u64 --distributions random,zipf2
   ```

   Use `local` for revision comparisons, `smoke` for a small algorithm comparison, or `release` for full coverage. Preview with the same preset and overrides you intend to measure.

2. Record host and compiler conditions. Choose a CPU allowed by the host affinity policy. Prefer an idle machine, performance governor, disabled turbo/SMT and a pinned logical CPU; obtain operator approval before changing governor, turbo or SMT settings. Native opponents also need the [native prerequisites](#native-prerequisites).
3. Run the selected command with those inputs:

   ```sh
   # Revision comparison; HEAD~1 must have a compatible API and harness.
   pnpm tools compare commit HEAD~1 --types u64,row_u64 --distributions random,zipf2 --cpu 2

   # Rust-only algorithm smoke comparison, including verification and reporting.
   pnpm tools compare algorithms

   # Full algorithm comparison, including native opponents and a second SIMD build.
   pnpm tools compare algorithms --plan release --simd --cpu 2
   ```

   CPU 2 is an example, not a portable default. Both commands accept `--out DIR`, require a new directory and otherwise use `bench/results/<timestamp>`. Keep raw results and research harnesses outside Git. Algorithm comparisons accept `--sample-ms` (default 5) and `--memory-mib` (default 256); choose them before starting. Resume a prepared session with `pnpm tools compare resume DIR`, without changing its selection, settings or executables.

4. Retain the resolved plan, source/build provenance, verification results and controls. Report tested coverage, environment, ratios, A/A ranges and result location. Identify partial, interrupted and unmeasured coverage explicitly. Investigate changes outside the controls, including apparently untouched cases; do not dismiss them as noise or tune source layout to one executable. The command contracts below define the scope of each result.

## Additional source-attribution controls

Algorithm comparisons from `compare algorithms` use the [paired timing contract](#timing-and-statistics) and report results for the measured inputs and environment. Attributing a gain to a source change rather than code placement additionally needs in-process revision comparisons and independent placement controls, as in the [engine measurements](../design/port.md). Process-level `compare commit` and continuous alerts alone do not provide that attribution evidence.

`compare algorithms` compares registered implementations from the current source snapshot, not a baseline commit. `compare commit` accepts a baseline commit and builds two executables per selected layout; it alternates adjacent A/B samples without a separate A/A pass. `--layout-seeds` samples section ordering across builds, but does not put revisions in one address space or provide independent-layout A/A within a block. The CLI does not automatically register both revisions and independent layout copies in one timing executable.

Before adding a custom experiment, identify the exact unmet requirement, explain why existing plans, options or adapters cannot meet it, and obtain operator agreement. Prefer extending the maintained comparison pipeline so verification, provenance and reporting remain shared. Historical harnesses record past evidence, not the default procedure for new measurements.

## Working-tree comparisons

`tools compare commit` and `tools compare algorithms` accept `--out DIR` and require a new directory. Their default output is `bench/results/<timestamp>`. They snapshot the current sources and clean up their temporary worktree and build directory under `~/code/tessera-bench/`; `TESSERA_BENCH_WORK` overrides that location. Repository-internal symlinks are materialized; escaping or dangling links and directory cycles are rejected before building. Current sources may use absolute links to internal files. Baseline links must resolve inside an archive of the selected commit; absolute links into the live checkout are rejected. Results are retained. The [direct executable commands](#direct-executable-sessions) support resumable algorithm sessions and custom investigations.

`compare commit [COMMIT]` defaults to `HEAD` and uses any resolvable local commit compatible with the current crate API, harness and lockfile. Both revisions are built from the current benchmark harness with one toolchain and the same features; Linux revision workers use `setarch -R` to control process address randomization (support is checked before building); the baseline substitutes only `rust/tessera-sort` from the named commit. Selected timed IDs must match. The `local` plan measures only `rust/u64/random/65536/s1/api`; override its axes or choose another plan explicitly, rather than implicitly running the full continuous inventory. `--filter` optionally narrows the resulting timed IDs.

Fresh worker pairs measure adjacent A/B samples, reversing startup, warm-up and measurement order on every other pair. The catalog owns one total warm-up and one total sampling budget per revision; each is divided across samples, with no outer repetition. Input preparation repeats in each fresh process outside the clock. Report the median B/A ratio and its distribution-free 99% median interval; only an interval entirely outside 0.95–1.05 alerts. Nominal coverage assumes independent pairs and is per case, not family-wide. The [continuous protocol](continuous-benchmarks.md#within-job-scheduling) defines batching, clocks and interference limits. This is a development screen, not an in-process or placement-controlled source claim.

`--shard INDEX/COUNT` optionally measures a deterministic partition of complete timed IDs, retaining their complete per-case sampling budget. The result records the complete inventory, shard identity and selected subset. Plans with `parallel` rules enable Rayon in both builds, select identical serial/four-thread IDs and require Linux with four allowed CPUs. Serial and parallel preparation never overlap; serial calls use one CPU, parallel calls four. Hosted CI uses this path with the continuous plan; layout sampling remains separate.

The output contains the resolved `plan.json`, `local.json`, `summaries.json`, `index.md` and `local.svg`. The JSON records revision and source hashes, executable hashes, both builds' actual Cargo provenance, configuration, host information, selected IDs, measurement-time sample counts and observations. Offline regeneration uses those retained counts, not the current catalog; early v1/v2 artifacts without counts retain their original fixed 20/30/40-sample contract. Compiler and encoded flags come from the build script, including `.cargo` target-specific settings and explicit `RUSTC`; mismatched build settings fail before measurement. Interrupted comparisons retain completed blocks but cannot be resumed or reported as complete. Use `pnpm tools report DIR` to regenerate a completed report.

Standard algorithm-report figures show fixed slices and aggregates, not every measured case. A selected-input panel is generated only when no standard panel can be drawn. Custom lengths omitted from those figures remain in Full results and `summaries.json`.

The [tooling design](../design/benchmark-tooling.md) describes the module boundaries and [figure guide](benchmark-figures.md) explains the shared presentation layer.

### Layout sampling

Use `compare commit --layout-seeds 1,2,3,4,5` to investigate sensitivity to function-section ordering. This opt-in experiment requires Linux x86-64, the `x86_64-unknown-linux-gnu` target, a GNU-compatible `cc` driver and LLD. Seeds are a comma-separated list of distinct integers in 1..4294967295. Zero is rejected because LLD interprets it as random. The option applies `--shuffle-sections=.text.*=<seed>` through final benchmark rustc arguments, keeping dependency flags unchanged. These builds explicitly select `cc` and LLD; they are a separate local configuration, not a reproduction of continuous benchmark flags. Linker and driver versions, final rustc arguments and both revisions' link maps are retained. Inspect the maps to confirm that the functions under investigation moved; distinct executable hashes alone do not establish this.

Preview the same plan and axes first, then run a small same-source calibration before comparing an engine change:

```sh
pnpm tools plan local --types u32,u64 --distributions random --lengths 48,65536
pnpm tools compare commit HEAD --types u32,u64 --distributions random --lengths 48,65536 \
  --layout-seeds 1,2,3,4,5 --out bench/results/layout-calibration
```

For a same-source calibration, keep the working-tree library identical to `HEAD`; the recorded source hashes verify that condition. Each seed gets the same adjacent-pair protocol; seeds run sequentially, with no outer repetition. Measurement work therefore grows linearly with the number of seeds: this command does not split a fixed sample budget. Start with sensitive small inputs and larger random controls rather than the full continuous plan.

Schema-2 `local.json` retains every seed's schema-1 revision run. Reports keep each seed’s paired median interval and verdict, then describe the median and min/max of the per-seed median ratios. Each revision's spread is max/min minus one across its per-seed median times. Per-case `layout-N.svg` figures show absolute median times and observed sample ranges for both revisions. These ranges describe the observed sample; they include host/process variation and are not confidence intervals or automatic regression thresholds. Equal seeds need not produce matching addresses across revisions, because section inventories and sizes can change. A few favorable seeds cannot establish source attribution or determine how many layouts suffice for a cloud budget. Function-section shuffling also does not independently vary block ordering inside a function or CGU/inlining choices.

Use `pnpm tools report DIR` for either local schema; every seed must have its complete recorded sample budget (nine blocks only for legacy runs). Layout sampling does not change continuous Testbeds, thresholds, sampling windows or submission behavior.

With layout seeds, `--layout-control cgu1` applies one code-generation unit to both revisions, `aligned` explicitly requests the shared 64-byte alignment defaults, and `engine-first` relinks after selecting engine-owned out-of-line functions from matching raw/demangled `nm` views. Inline code stays in its callers. Default CLI alignment also applies to CGU and symbol-order experiments; these options do not establish isolated or combined performance gains. The result retains the chosen control, actual flags, link maps and, for `engine-first`, per-role `.order` files. Controls preserve Cargo-resolved flags and require Linux x86-64 GNU, `cc`, LLD and (for engine ordering) `nm`.

### Fixed-work counters

The command defaults to `HEAD` and the `local` plan.

`compare counters [COMMIT]` uses the current smoke harness for both library revisions, with the local-only `layout-study` feature and the default ISA build. It requires Linux x86-64 and Valgrind; `--simd` is rejected because a simulated ISA path is not the real host's dispatch. Select a small smoke plan with the usual axes, then run:

```sh
pnpm tools compare counters HEAD --types u32,u64 --distributions random --lengths 48,65536 \
  --sorts 8 --out bench/results/counters-null
```

Each selected serial API case runs in three separate processes: baseline, a second baseline for A/A, and current. Their order reverses on alternate cases. Each process first verifies a complete sort, prepares `--sorts` independent copies (default 8, range 1..1024), calls the stable `tessera_counted_sorts` marker once and verifies every output afterwards. Callgrind collects that dynamic call tree with cache and branch simulation enabled. The batch loop and dispatch overhead are included; input generation, allocation of prepared copies, restoration and verification are outside collection. Counts describe this prepared-cache workload and simulator, not cold-cache execution or hardware PMU events. Large selections multiply both prepared memory and simulation cost; input count must remain practical for the host.

`counters.json` retains both source hashes, executable hashes, compiler flags, Valgrind version, the fixed sort count, event totals and whether the measured working tree was modified. Initial and regenerated reports display that state beside HEAD; older results without the field are labelled unrecorded. Every process's completion receipt is checked against its requested ID and sort count. Raw `callgrind-<case>-<role>.out` files allow annotation. `Ir` is instructions; `D1mr`/`D1mw` and `DLmr`/`DLmw` are simulated data-cache read/write misses; `Bcm`/`Bim` are simulated conditional/indirect mispredictions. Cache and predictor models can still depend on code and data addresses. Equal instructions and simulated events cannot prove that a time change is caused by layout, and changed counts need not explain the whole time change. The report is descriptive and does not submit a Bencher metric or change a verdict. A failed process or zero/malformed instruction summary fails the operation; incomplete results cannot regenerate as a complete report.

Some host C libraries or loaders require instructions unsupported by Valgrind, even when the measured Rust code uses the default ISA. For such hosts, build the provided baseline-ISA container, then use `--valgrind-image tessera-callgrind`. `BASE_IMAGE` may select another compatible glibc image. The command records the local immutable image ID and Valgrind version, mounts temporary sources read-only and results writable, and runs without network access or extra capabilities. Version probes and measurements use uniquely named containers; after failure or cancellation, forced removal is awaited before returning control to workspace cleanup. A cleanup failure is logged with the owned name while preserving the original failure. The host builds the executable; an incompatible container libc or unsupported executable instruction fails visibly.

```sh
docker build -t tessera-callgrind -f bench/local/Callgrind.Dockerfile .
pnpm tools compare counters HEAD --types u32 --distributions random --lengths 48 \
  --valgrind-image tessera-callgrind --out bench/results/counters-container
```

`continuous probe-counters --out DIR` separately tries `instructions:u`, `branch-misses:u`, `idq.dsb_uops:u`, `idq.mite_uops:u` and `baclears.any:u` with the host's `perf`. `probe.json` preserves each success/failure and raw output; Intel-specific aliases may be unavailable on AMD. The probe does not change `perf_event_paranoid`, request capabilities or infer cloud access from local access. Use the actual measured host before integrating PMU reads into a timed run.


## Experiment plans

Benchmark workflows use schema-1 plans in [`bench/plans/`](../../bench/plans/). Repository presets, the shared catalog and selection fixtures use TOML for comments and readable input rules; custom files may use either TOML or JSON. Both formats decode into the same configuration model and use the same validation and selection rules. Plans describe what to measure; the harness owns input generation and the measurement protocol.

| Preset | Harness | Default selection |
|---|---|---|
| `local` | `smoke` | Tessera, random `u64`, 65,536 records, `api` |
| `continuous` | `smoke` | Regression inputs and timed IDs from the [continuous plan](../../bench/plans/continuous.toml), documented in the [continuous guide](continuous-benchmarks.md#cases) |
| `smoke` | `comparison` | Rust competitors, 125 inputs at 1,000 records |
| `release` | `comparison` | Rust and native competitors, 381 inputs across four lengths |
| `parallel` | `comparison` | Serial controls, Tessera-8 and Rayon stable-8; 16 inputs at 1M records |

`compare commit` defaults to `local`; `compare algorithms` defaults to `smoke`. The executable embeds `continuous.toml` at build time. Hosted comparisons additionally pass and retain the normalized plan. Editing coverage does not require another workflow case inventory; the trusted publisher must recognize the updated plan after merge.

A custom plan contains a union of products, followed by exclusions. For example:

```toml [my-plan.toml]
schema = 1
harness = "comparison"
algorithms = ["glidesort", "radsort"]

# Each rule expands into a Cartesian product.
[[include]]
types = ["u32", "u64"]
distributions = ["random", "zipf2"]
lengths = [65_536, 1_000_000]

[[include]]
types = ["row_u64"]
distributions = ["d16"]
lengths = [65_536]

[[exclude]]
types = ["u32"]
distributions = ["zipf2"]
lengths = [1_000_000]
```

JSON accepts the same decoded fields; use the exported schema rather than maintaining a second example.

Values within an axis are alternatives; axes within a rule form a Cartesian product. `include` rules are unioned and duplicate inputs are measured once. Any matching `exclude` rule removes that combination. Omitted axes use the harness's catalog defaults when expanding inputs and match every value when excluding them. An empty choice array is invalid. Unknown fields and identities are errors, not ignored filters.

The selectable case axes are `types`, `distributions` and `lengths`. Lengths are positive record counts; custom numeric lengths need not already occur in a preset. Configuration files can also use `"insertion-max"`, `"insertion-next"` and `"tile-next"`. These resolve from the measured library's thresholds and the record width in the catalog. Case IDs retain the resolved numeric length; a commit comparison rejects different selected IDs between revisions.

The `smoke` harness measures Tessera through its public sorting functions; its supported record representations and distributions are under `smoke` in [the catalog](../../bench/catalog.toml). The `comparison` harness measures supported algorithm/type pairs; its default types, distributions and lengths are under `comparison`. Criterion's serial IDs end in `/api`; optional `parallel` rules in a smoke plan independently select four-thread `/par-4` inputs for the continuous supervisor. They use the same input axes and shared exclusions, and axis overrides apply to both rule sets. Comparison plans reject these rules and retain their own algorithm selections. Comparison input IDs use `type/distribution/length/s1`; neither harness has a general mode selector. Inputs always use seed 1. A plan that selects nothing fails before timing.

Algorithm comparisons retain `tessera`, `std-stable` and the separately timed Tessera A/A control, even when only additional opponents are named. Native algorithms automatically enable source acquisition and native compilation. Portable SIMD is requested with `--simd`; that additional build measures the shared Tessera and standard-library anchors, plus any selected parallel adapters; other competitors run in the default build. Unsupported adapter/type pairs are omitted from the manifest's per-input capabilities, not recorded as measurements.

```sh
# Preview a resolved plan without compiling or timing.
pnpm --silent tools plan local --types u32,u64 --distributions random,zipf2

# Save that selection and reuse it against different commits.
pnpm --silent tools plan local --types u32,u64 > my-plan.toml
pnpm tools compare commit main --plan my-plan.toml
pnpm tools compare commit HEAD~1 --plan my-plan.toml

# JSON remains available for saved plans and automation.
pnpm --silent tools plan local --format json > my-plan.json

# Override algorithms and inputs for one horizontal comparison.
pnpm tools compare algorithms --plan smoke --algorithms glidesort,radsort \
  --types u64 --distributions random,zipf2 --lengths 1234,65536

# Export the validation schema for editors or external tooling.
pnpm --silent tools plan --schema > benchmark-plan.schema.json
```

`--plan` accepts a preset name or a `.toml`/`.json` path relative to the current directory. `plan` prints TOML by default; `--format json` selects JSON. `--schema` always emits JSON Schema, which describes the decoded model for either format. The comma-separated CLI axes replace the corresponding axis in every inclusion rule; exclusions remain in force. `--algorithms` replaces the opponent list before required controls are added. Preview commands print the resolved selection, not a list of code-relative case IDs; the native harness expands those IDs using the measured code. Every result directory retains the normalized `plan.json`, regardless of the input encoding, and algorithm manifests retain the resolved case and capability lists. A selected subset is a complete experiment in its own right; `--group` instead runs one resumable group from that plan and marks the report partial.

The lower-level Rust binaries also accept `--plan FILE`; `--suite smoke|release` remains available for their built-in sessions.

## Local parallel comparisons

The `parallel` plan uses the same preparation, verification, nine shuffled blocks, serial Tessera A/A pair, memory worker, retained binaries, resume and reports as other algorithm comparisons. It automatically enables the benchmark's `rayon` feature. Hosted continuous comparisons use their own four-thread cases in the [continuous plan](continuous-benchmarks.md#within-job-scheduling), not this eight-worker local comparison plan.

Start with one representative input while iterating, using one logical CPU per physical core. CPU numbering varies by host; check `lscpu -e=CPU,CORE` before choosing a mask. On the Ryzen 7 8845H development host, adjacent CPU pairs are SMT siblings:

```sh
pnpm tools plan parallel --types row_u64 --distributions random --lengths 10000000
pnpm tools compare algorithms --plan parallel \
  --types row_u64 --distributions random --lengths 10000000 \
  --cpu 0,2,4,6,8,10,12,14 --out /tmp/tessera-parallel
pnpm tools compare resume /tmp/tessera-parallel
pnpm tools report /tmp/tessera-parallel
```

The default opponents are `tessera-par-8` and `rayon-stable-8`, plus the required serial controls. With `--simd`, the parallel adapters also run in the additional Portable SIMD build, using the same pool widths. One-, two- and four-worker versions are explicit algorithm choices, for example `--algorithms tessera-par-1,tessera-par-2,tessera-par-4,tessera-par-8`; they do not run by default. Type, distribution and length overrides work unchanged.

Each parallel adapter owns a pool of the recorded width. Pool construction and worker warm-up are outside timing and allocation counting; each sort includes `ThreadPool::install` and the complete public sort call. Workers inherit the process affinity mask; they are not individually pinned. Pools join all workers before the next adapter. Linux thread checks permit only the selected pool's workers plus the caller, preserving the one-thread check for sequential opponents. Pool widths and the effective affinity/topology are retained in the manifest and host fingerprint. Enough allowed CPUs and physical cores are the operator's responsibility; oversubscribed runs are not scaling evidence.

The report's ratio is variant time divided by the paired serial Tessera time; its inverse is whole-call speedup over the serial API. The A/A range controls the serial baseline, not parallel-only variability. Source-change attribution still needs the [additional controls](#additional-source-attribution-controls); this plan does not provide an independent placement control or a separate A/A pair for each parallel opponent.

Peak extra heap bytes count allocations inside the sort, not pre-existing pool/runtime storage or thread stacks. The existing memory coverage limits still apply. Inputs below the parallel cutoff and one-worker Tessera pools exercise the serial fallback; small inputs check integration, not useful parallel speedup.

## Native prerequisites

The Rust competitors need Cargo's stable toolchain. Native competitors additionally need Linux x86-64 with glibc, Node.js 24, Clang with C++17 support, CMake, GNU binutils, and TBB headers (`libtbb-dev` on Debian/Ubuntu). The pinned IPS²Ra/IPS⁴o headers include TBB queue declarations even for their sequential entry points; the benchmark executables do not link TBB or OpenMP. The CLI fetches and verifies sources before building; direct sessions use the separate fetch command below. Building and measuring use the verified local sources without network access. The report requires Node.js 24 and `pnpm install --frozen-lockfile`.

For static Rust checks without the native toolchain or fetched sources, use `TESSERA_BENCH_CHECK_ONLY=1 cargo clippy --workspace --all-targets --all-features` or `TESSERA_BENCH_CHECK_ONLY=1 cargo doc --workspace --no-deps --all-features`. This mode omits native compilation and linker inputs, not the native-facing Rust code. Leave the variable unset for builds, tests and measurements that link native competitors.

Native builds default to `clang` and `clang++`. For published comparisons, prefer Clang with the same LLVM major version reported by `rustc -vV`; check it against `clang --version`. Set `CC` and `CXX` to select versioned Clang executables or another compiler. The manifests and report record the actual compiler versions, including any LLVM version difference.

## Resuming algorithm comparisons

`compare algorithms` builds and retains every selected configuration before starting measurements. Each configuration keeps its `compare` and `memory` executables under `bin/`; `session.json` records their hashes, the plan hash, CPU, optional group and sampling limits. Preparation removes its temporary source/build tree only after the executable pairs are retained. A session interrupted before `session.json` exists is not prepared; start a new output directory.

```sh
pnpm tools compare algorithms --plan release --simd --cpu 2 --out bench/results/release
# After an interruption, use the same directory; no rebuild or selection overrides.
pnpm tools compare resume bench/results/release
```

Resume verifies retained files before executing them. Validation receipts bind each successful validation to its executable pair, plan and group. The Rust journal retains complete nine-block groups and complete memory rows; it replaces interrupted groups and drops an incomplete final JSONL write. Plans or executables changed after preparation require a new session. Rust also checks the recorded host and build provenance before measurement.

Only one session writer is allowed. After an abrupt parent exit, its lease must expire before another CLI writer can recover; a still-running comparison or memory worker retains its own kernel lock and blocks conflicting work. Handled cancellation rejects success and cleans up child processes. Neither a completed preparation nor a failed/cancelled measurement is a completed experiment.

## Direct executable sessions

For normal comparisons and recovery, use `pnpm tools compare algorithms` and `pnpm tools compare resume`. Use the binaries directly to split groups across compatible directories or investigate a lower-level harness issue. Keep default and Portable SIMD builds separate. The following commands show a native release session; omit `compare-native` and the fetch step for Rust-only opponents.

```sh
pnpm tools native fetch
cargo build --release -p tessera-bench --features compare,compare-native \
  --target-dir target/compare-default
target/compare-default/release/compare list
target/compare-default/release/compare validate --suite smoke
target/compare-default/release/compare validate --suite release

taskset -c 2 target/compare-default/release/compare run \
  --suite release --out bench/results/release
pnpm tools report bench/results/release
```

CPU 2 is illustrative; use the [host conditions](#run-an-experiment) chosen for the experiment.

`compare run` first invokes the sibling `memory` executable, which repairs its journal and measures missing adapter/case rows before timing continues. Memory can also be measured separately:

```sh
taskset -c 2 target/compare-default/release/memory \
  --suite release --out bench/results/release
```

`--group TYPE/SHAPE` selects one complete group, for example `--group u64/random`. Repeating a run skips completed groups and replaces any interrupted group. An interrupted final JSONL write is discarded on resume. A change to the executable, build provenance, suite or host fingerprint requires a new output directory. Groups measured in separate directories can be reported together when their host fingerprints and build manifests agree:

```sh
pnpm tools report bench/results/part-1 bench/results/part-2
```

The report requires the complete suite. `--partial` generates a report of the completed groups during development. Results go into the first directory's `report/`, with Markdown, SVG figures where the required cases are present, and machine-readable summaries. `bmf.json` in that directory can be uploaded with the Bencher CLI. The report reads only the result directories.

`list` derives supported types from the same adapters used by `validate`, timing and memory measurement.

### Portable SIMD

Build the Portable SIMD executable separately so the default executable remains available:

```sh
cargo build --release -p tessera-bench --features compare,portable-simd \
  --target-dir target/compare-simd
target/compare-simd/release/compare validate --suite smoke
taskset -c 2 target/compare-simd/release/compare run \
  --suite release --out bench/results/release
pnpm tools report bench/results/release
```

This executable measures Tessera twice and `slice::sort` on the same cases, plus selected parallel adapters when the `rayon` feature is also enabled. Other competitors are measured in the default build. Each build has its own A/A control; `slice::sort` anchors the ranking to the same scale. The report records the absolute anchor drift between processes and flags differences above 5%. Both builds belong to the same session and use the same source revision, toolchain and target flags.

## Cases and suites

Case IDs are `type/distribution/length/s1`. A group contains all lengths of one type and distribution. The release suite contains 381 inputs: the original 260-case cross-type matrix and 121 additional cases using the high-value distributions from continuous tracking. `smoke` validates the same 125 type/distribution pairs at length 1,000. The existing Criterion smoke suite retains its own regression cases and IDs.

| Type | Task | Representation |
|---|---|---|
| `u32`, `u64` | keys | Unsigned integers |
| `f64` | keys | Arbitrary float bits, ordered by `total_cmp` |
| `row_u64` | rows | `{u64 key, u32 row}`, C layout, 16 bytes |
| `record128` | records | `{u64 key, [u8; 120]}`, C layout, 128 bytes |
| `owned` | owned records | `{u64 key, String}`, 16-character payload |
| `row_u32` | supplementary rows | `{u32 key, u32 row}`, C layout, 8 bytes |

Payloads encode the original index. Wide-record payloads also fill every remaining byte deterministically. Verification checks complete payloads, not just keys and indices.

The ten baseline distributions remain `random`, `d16`, `d256`, `zipf1.1`, `zipf2`, `skew99`, `sorted`, `reverse`, `sorted99` and `runs`. Each runs on all seven types at 1,000, 65,536, 1,000,000 and 10,000,000 records; `record128` and `owned` stop at 1,000,000. They give the ranking and cross-type aggregate figures one common input set. The expanded release plan adds:

| Distributions | What they distinguish | Types | Lengths |
|---|---|---|---|
| `equal`, `two_values`, `d4096`, `hot50` | Constant keys, widely separated ties, intermediate cardinality, and a repeated key beside a high-cardinality remainder | All seven | 65,536; 1,000,000 |
| `far_swaps`, `sorted_tail` | Long-distance disorder and an ordered prefix with an outlier tail, rather than adjacent inversions alone | All seven | 65,536; 1,000,000 |
| `sparse8` | Eight full-width keys rather than a narrow small-integer range | `u32`, `u64`, `row_u32`, `row_u64` | 65,536; 1,000,000 |
| `sparse_digits`, `bimodal` | Sparse digit occupancy and two distant high-cardinality clusters | `u64`, `row_u64`, `record128`, `owned` | 65,536; 1,000,000 |
| `finite_signed` | Finite signed floats with bounded magnitudes rather than arbitrary float bits | `f64` | 1,000; 65,536; 1,000,000; 10,000,000 |

All nine new integer distributions also run on `u64` at 10,000,000. These are formal comparison inputs, plotted individually in the 1M per-type panels. The type restrictions preserve the generators' meaning: narrowing `bimodal` to `u32` would discard its cluster bit, and converting integer masks to floats changes their encoded digit structure. All generators and their existing input hashes are unchanged.

Memory is measured for every supported variant through 1,000,000 and for all `u64` cases at 10,000,000. Serial Tessera's peak is also measured for the remaining larger cases to provide `glidesort-sqrt` with its buffer size. Selected `tessera-par-*` adapters measure every selected length and type, including large `u32` keys and records; other adapters retain the limits above.

### Input generation

`case.rs` owns the seed-one generator used by all three executables and Criterion. Tests pin the little-endian input hashes from the preceding smoke implementation for `u32`, `u64`, `u128` and `f64` at two lengths for each existing distribution.

The random source is SplitMix64, starting at state 1. Each draw adds `0x9e3779b97f4a7c15`, then applies the xor-shift/multiply finalizer with shifts 30, 27, 31 and multipliers `0xbf58476d1ce4e5b9`, `0x94d049bb133111eb`, using wrapping 64-bit arithmetic. `u32` takes the low 32 bits; `u64` takes the draw; `u128` concatenates two draws, high word first; `f64` interprets the draw as bits.

- `random` draws each key over its full bit range.
- `dD` takes `next mod D`, for D = 16, 256 or 4,096.
- Zipf inputs select ranks `k − 1`, for k = 1…n, from cumulative integer weights. `zipf2` uses `floor(2^62 / k²)`. `zipf1.1` uses `floor(2^48 / (k · floor(256 · k^(1/10))))`; the tenth root is computed by integer binary search. A draw selects `floor(next · total / 2^64)` and the first cumulative weight greater than it. The fixed-point root makes `zipf1.1` a reproducible approximation to the exponent 1.1.
- `skew99` uses ten spread values: `v_k = k · floor(MAX / 10)` for integers and `k · 2^1020` for floats. A first draw selects `v_0` with 99% probability; otherwise a second selects `v_(1 + next mod 9)`.
- `sorted` is 0…n−1; `reverse` reverses it. `sorted99` starts sorted and swaps adjacent elements at a spacing of `max(2, floor(n / max(1, floor(n / 100))))`.
- `runs` draws random keys, then sorts consecutive runs of `16 + next mod 2048` keys, the final run shorter. Each run reverses if the next draw's high bit is set.

- `equal` repeats one; `two_values` picks the spread values `v_0` and `v_9` with equal probability.
- `sparse8` first draws eight full-width keys, then picks among them uniformly.
- `sparse_digits` masks a draw with `0x5555_5555_5555_5555`, leaving 16 possible values per 8-bit digit.
- `bimodal` sets bit 63 from one draw and fills bits 39…0 from the next, giving two clusters 2^63 apart over 2^40 keys each.
- `hot50` takes zero with 50% probability and `draw_high` otherwise. For integers, the latter sets the top bit of a full-width draw; for `f64`, it draws positive NaN payloads or infinity. The remainder is high-cardinality but not guaranteed unique.
- `far_swaps` starts ascending and swaps `max(1, len / 100)` pairs for lengths above one, choosing one position in each end quarter. Each swap spans at least half the array; positions may repeat.
- `sorted_tail` starts with 0…`len - len / 128 - 1`, then appends `len / 128` draws from `draw_high`, above the prefix. The tail itself is not sorted.
- `finite_signed` draws an `f64` sign, an exponent field in 1000…1029 and a random mantissa: magnitudes range from 2^-23 to below 2^7, without NaN or infinity.

Tests pin the seed-one hashes of every distribution. Float validation additionally checks signed zeros, infinities, subnormals and NaNs of both signs with distinct payloads.

## Implementations

Rust competitors are optional dependencies behind `compare`, pinned to exact versions. Native sources are pinned by repository, commit, archive SHA-256 and license in `native/sources.toml`; `bench/rust/native/fetch.ts` verifies the archives and the build verifies the extracted trees. Third-party sources stay in ignored `bench/vendor/`.

The [catalog](../../bench/catalog.toml) owns variant identities, stability and display names; executable adapters in [variant.rs](../../bench/rust/src/comparison/variant.rs) own actual capabilities. Use the runner's `list` output for supported tasks rather than maintaining a second matrix here.

Voracious 1.2.0's record path fails the stable-reference check on duplicate keys, and its float path uses partial comparison for small inputs. Those tasks are unsupported by this adapter. VQSort and x86-simd-sort's floating-point interfaces do not provide the required NaN payload and signed-zero total order; their pair interfaces use layouts different from the row tasks.

The DuckDB adapter pins the v1.4.0 headers used by the [Sorting, Again](https://duckdb.org/2025/09/24/sorting-again) article: Vergesort discovers runs and invokes Ska Sort for unsorted ranges, with DuckDB's pdqsort fallback inside Ska Sort. It is an **unstable, single-threaded in-memory kernel**, not a SQL query or DuckDB end-to-end measurement. The adapter sorts raw integer keys and complete records; it excludes SQL key encoding, external merging and cancellation checks. Its extractor presents 32-bit keys in the high half of DuckDB's 64-bit key part and scans only their four bytes. Floating-point and owned-string tasks are unsupported. The headers remain unmodified; the wrapper supplies only the release-mode numeric-cast and fallthrough helpers plus key extraction. The verified archive is scoped to the three compiled header trees and their licenses, excluding database sources and unrelated fixture symlinks.

The native comparison sorts compare keys inside C or C++. Each FFI call sorts a complete buffer. The batch clock stays in Rust, and no per-element callback or binding conversion is timed. Record sizes, alignments and payload offsets are checked across the boundary. Native code uses `-O3 -DNDEBUG -march=x86-64 -mtune=generic`; x86-simd-sort's separate dispatch objects use their upstream fixed ISA targets. IPS²Ra and IPS⁴o are built without the parallel-library configuration. The runner checks the process thread count after each sort sample. The report names the SIMD dispatch targets.

On keys, stable and unstable sorts perform the same observable task. On records, unstable results are labelled separately: they answer an unstable-record task and do not enter the stable ranking or stable grid.

## Timing and statistics

For every sample, the runner restores independent buffers outside the timed region, clocks only the batch of sort calls, then verifies every output outside the clock. Stable variants must match the standard stable reference exactly. Unstable records must be ordered, contain each original index once, and preserve every record's payload.

Calibration chooses a batch from one verified warm-up sort, a minimum sample duration (5 ms by default) and a prepared-buffer cap (256 MiB). At least one buffer is used, including when that buffer exceeds the cap. `--sample-ms N` and `--memory-mib N` set these values; both are recorded. The input and stable reference are generated once per case and shared immutably. One variant's prepared buffers are resident at a time. Batch size depends on the variant's speed, so variants can have different prepared-buffer working sets and cache behavior. Scratch allocation occurs inside the timed sort call.

Each case has nine blocks. Each block shuffles all supported variants and two Tessera calls into a deterministic random order. The second Tessera sample is named `tessera-aa`. All blocks reuse the same seed-one input, so their range measures execution variability on that input rather than variability across random inputs. CI validates the smoke suite and report generation; it does not run the complete release matrix.

For each block, a variant's ratio is its time per sort divided by the mean of the two Tessera times per sort. The report gives the median ratio, per-block minimum and maximum, and median nanoseconds per element. The A/A noise range is the minimum and maximum of both Tessera times divided by their block mean, across all nine blocks. This uses the same denominator as the variant ratios and is unchanged when the two control labels are exchanged. A variant is called faster or slower only when at least eight blocks agree on that direction and its median is outside that range. For example, controls of 100 and 120 give a mean of 110 and a noise range of 100/110 to 120/110; an opponent time of 109 remains within noise. The `tessera-aa` row separately reports A2 / A1, with its own per-block bounds.

The standard report panels show the 1M-key ranking, 1M and 1,000-key distribution comparisons, 1M element-type comparison, 1M per-type input panels and 10M-key peak heap. Ranking and cross-type geometric means use the ten common baseline distributions; per-type panels show every selected distribution at 1M, including targeted additions. Memory maxima include every selected 10M `u64` distribution. Keys admit stable and unstable opponents; record rankings admit stable opponents only. Speed panels anchor to `slice::sort` within the same blocks, with Portable SIMD shown separately. Every figure includes plotted values; [the figure guide](benchmark-figures.md) owns visual encoding and layout. Custom cases remain in full results even when no standard panel includes them.

Hosted continuous comparisons use the same CLI alignment defaults for both feature configurations, as described in the [continuous benchmark guide](continuous-benchmarks.md).

The [port record](../design/port.md) describes separate placement controls for source A/B investigations. The release comparison measures lengths of at least 1,000 with the default target; native-CPU builds, branch alignment and cold-cache investigations remain separate work.

## Results and provenance

```text
DIR/
  plan.json                       normalized experiment selection
  session.json                    immutable settings and executable hashes
  bin/BUILD/compare, memory       retained executable pair for each build
  bin/BUILD/validated.json        successful validation receipt
  manifest.json                   default timing build and complete case plan
  manifest-portable-simd.json     optional SIMD timing build
  memory-manifest-BUILD.json      memory executable's build provenance
  host.json                       machine fingerprint
  blocks.jsonl                    verified samples and group observations
  memory.jsonl                    verified peak extra allocation sizes
  bmf.json                        report-generated Bencher summaries
  report/
    index.md                      environment, capabilities, figures and tables
    summaries.json                per-case ratios, bounds, verdicts and times
    *.svg                         figures
```

Manifests record the source revision and dirty state, source and executable SHA-256, `rustc -vV`, Cargo lockfile, features, target, profile, optimization level, debug setting and encoded Rust flags. Native manifests add compiler versions, source pins, licenses and translation-unit flags. A group's start and end observations record load, frequency, temperature and memory pressure separately from the host fingerprint.

The memory and timing executable must agree on source, toolchain, target, profile, optimization level, debug setting, features, native build and actual C/C++ compiler versions (`cc` and `cxx`). Native manifests must include both compiler versions. Default and Portable SIMD builds must also agree on optimization level and debug setting. The report rejects incompatible manifests or hosts, unknown or unverified samples, duplicate samples or slots, missing blocks, incomplete groups, and missing required memory samples. Observations may vary; the report flags load, frequency or temperature deviations from the session median.

## Memory accounting

The separate memory executable measures a single sort after its input and reference have been allocated. Rust-only builds count requested bytes through a global allocator. Native builds count allocator usable sizes, including allocator rounding, through wrapped `malloc`, `calloc`, `realloc`, `free`, `aligned_alloc` and `posix_memalign`. C++ allocation operators are overridden in this executable because dynamic libstdc++'s internal calls are not intercepted by linker wrapping alone. A self-test checks both Rust and native allocations for double counting.

The count excludes input buffers and owned payloads; fixed stack storage is unmeasured. `glidesort-sqrt` allocates its buffer during the sort call, using the whole number of records that fits in Tessera's measured peak for that case. Its own internal fixed stack scratch remains part of glidesort's implementation.

Release archives include the complete results directory. Published performance claims come from that archive; synthetic chart fixtures are layout tests. The first release-candidate comparison and publication remain on the [roadmap](../../ROADMAP.md).
