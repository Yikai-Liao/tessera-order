# Benchmarking

The comparison runner in `bench/rust` measures sorting algorithms on one machine. It answers four questions: how Tessera compares with the standard stable sort, with other stable sorts, with unstable sorts when equal keys are indistinguishable, and how much extra memory each sort uses. The [continuous suite](continuous-benchmarks.md) tracks regressions on Bencher; release comparisons use the paired measurements described here.

## Development commands

Install Node.js 24 and the pnpm version pinned in `package.json`, then run `pnpm install --frozen-lockfile`. The command surface is `pnpm bench --help`.

```sh
# Working tree, including uncommitted source changes, versus a commit.
pnpm bench compare HEAD~1
pnpm bench compare main --types u64,row_u64 --distributions random,zipf2 --cpu 2

# Rust-only smoke comparison, including validation and a report.
pnpm bench run

# Full native comparison and optional Portable SIMD build.
pnpm bench run --plan release --simd --cpu 2

# Reports do not rebuild or rerun measurements.
pnpm bench report bench/results/SESSION
```

Both measurement commands accept `--out DIR` and require a new directory. Their default output is `bench/results/<timestamp>`. They snapshot the current sources and clean up their temporary worktree and build directory under `~/code/tessera-bench/`; `TESSERA_BENCH_WORK` overrides that location. Results are retained. The lower-level commands below remain available for resumable algorithm sessions and custom investigations.

### Working-tree comparisons

`compare [COMMIT]` defaults to `HEAD` and uses any resolvable local commit compatible with the current crate API, harness and lockfile. Both revisions are built from the current benchmark harness with one toolchain and the same features; the baseline substitutes only `rust/tessera-sort` from the named commit. Selected timed IDs must match. The `local` plan measures only `rust/u64/random/65536/s1/api`; override its axes or choose another plan explicitly, since each selected case runs 27 times across nine blocks. `--filter` optionally narrows the resulting timed IDs.

Each block runs two baseline processes and one current process, rotating through the six possible orders. Criterion's mean nanoseconds per sort is the observation. The current/baseline ratio uses the mean of the block's baseline pair; the report gives its median and block range, plus the paired A/A range. A direction needs eight of nine blocks and a median outside that range. This is a development screen with process-level A/A controls, not an in-process or placement-controlled performance claim.

The output contains the resolved `plan.json`, `local.json`, `summaries.json`, `index.md` and `local.svg`. The JSON records revision and source hashes, executable hashes, compiler, flags, configuration, host information, selected IDs and block observations. Interrupted comparisons retain completed blocks but cannot be resumed or reported as complete. Use `pnpm bench report DIR` to regenerate a completed report.

The [tooling design](../design/benchmark-tooling.md) describes the module boundaries and [figure guide](benchmark-figures.md) explains the shared presentation layer.


## Experiment plans

All three workflows use the schema-1 JSON plans in [`bench/plans/`](../../bench/plans/). Plans describe what to measure; the harness owns input generation and the measurement protocol. JSON keeps the format readable by Node, Rust, editors and CI without an executable configuration language.

| Preset | Harness | Default selection |
|---|---|---|
| `local` | `smoke` | Tessera, random `u64`, 65,536 records, `api` |
| `continuous` | `smoke` | The 50 regression inputs and 56 timed IDs documented in the [continuous guide](continuous-benchmarks.md#cases) |
| `smoke` | `comparison` | Rust competitors, 70 inputs at 1,000 records |
| `release` | `comparison` | Rust and native competitors, 260 inputs across four lengths |

`compare` defaults to `local`; `run` defaults to `smoke`. The continuous executable embeds `continuous.json` at build time, so its artifact carries the selection from the measured commit. Editing that file changes cloud coverage without editing the worker, publication workflow or case-registration code.

A custom plan contains a union of products, followed by exclusions:

```json
{
  "schema": 1,
  "harness": "comparison",
  "algorithms": ["glidesort", "radsort"],
  "include": [
    {
      "types": ["u32", "u64"],
      "distributions": ["random", "zipf2"],
      "lengths": [65536, 1000000]
    },
    {
      "types": ["row_u64"],
      "distributions": ["d16"],
      "lengths": [65536]
    }
  ],
  "exclude": [
    { "types": ["u32"], "distributions": ["zipf2"], "lengths": [1000000] }
  ]
}
```

Values within an axis are alternatives; axes within a rule form a Cartesian product. `include` rules are unioned and duplicate inputs are measured once. Any matching `exclude` rule removes that combination. Omitted axes use the harness's catalog defaults when expanding inputs and match every value when excluding them. An empty choice array is invalid. Unknown fields and identities are errors, not ignored filters.

The selectable axes are `types`, `distributions`, `lengths` and `modes`. Lengths are positive record counts; custom numeric lengths need not already occur in a preset. Configuration files can also use `"insertion-max"`, `"insertion-next"` and `"tile-next"`. These resolve from the measured library's thresholds and the record width in the catalog. Case IDs retain the resolved numeric length; a commit comparison rejects different selected IDs between revisions.

The `smoke` harness measures Tessera through `api` and `workspace`; its supported record representations and distributions are under `smoke` in [the catalog](../../bench/catalog.json). The `comparison` harness measures supported algorithm/type pairs through `api`. Its default types, distributions and lengths are under `comparison`. Inputs always use seed 1. A plan that selects nothing fails before timing.

Algorithm comparisons retain `tessera`, `std-stable` and the separately timed Tessera A/A control, even when only additional opponents are named. Native algorithms automatically enable source acquisition and native compilation. Portable SIMD is requested with `--simd`; that additional build measures the shared Tessera and standard-library anchors, while competitors run in the default build. Unsupported adapter/type pairs are omitted from the manifest's per-input capabilities, not recorded as measurements.

```sh
# Preview a resolved plan without compiling or timing.
pnpm --silent bench plan local --types u32,u64 --distributions random,zipf2

# Save that selection and reuse it against different commits.
pnpm --silent bench plan local --types u32,u64 > my-plan.json
pnpm bench compare main --plan my-plan.json
pnpm bench compare HEAD~1 --plan my-plan.json

# Override algorithms and inputs for one horizontal comparison.
pnpm bench run --plan smoke --algorithms glidesort,radsort \\
  --types u64 --distributions random,zipf2 --lengths 1234,65536

# Export the validation schema for editors or external tooling.
pnpm --silent bench plan --schema > benchmark-plan.schema.json
```

`--plan` accepts a preset name or a JSON path relative to the current directory. The comma-separated CLI axes replace the corresponding axis in every inclusion rule; exclusions remain in force. `--algorithms` replaces the opponent list before required controls are added. Preview commands print the resolved selection, not a list of code-relative case IDs; the native harness expands those IDs using the measured code. Every result directory retains `plan.json`, and algorithm manifests retain the resolved case and capability lists. A selected subset is a complete experiment in its own right; `--group` instead runs one resumable group from that plan and marks the report partial.

`release` remains an alias for `run`. The lower-level Rust binaries also accept `--plan FILE`; `--suite smoke|release` remains available for their built-in sessions.

## Run a comparison

The Rust competitors need Cargo's stable toolchain. Native competitors additionally need Linux x86-64 with glibc, Python 3.12 or later, Clang with C++17 support, CMake, GNU binutils, and TBB headers (`libtbb-dev` on Debian/Ubuntu). The pinned IPS²Ra/IPS⁴o headers include TBB queue declarations even for their sequential entry points; the benchmark executables do not link TBB or OpenMP. Fetching sources is a separate step; building and measuring use the verified local sources without network access. The report requires Node.js 24 and `pnpm install --frozen-lockfile`.

For static Rust checks without the native toolchain or fetched sources, use `TESSERA_BENCH_CHECK_ONLY=1 cargo clippy --workspace --all-targets --all-features` or `TESSERA_BENCH_CHECK_ONLY=1 cargo doc --workspace --no-deps --all-features`. This mode omits native compilation and linker inputs, not the native-facing Rust code. Leave the variable unset for builds, tests and measurements that link native competitors.

Native builds default to `clang` and `clang++`. For published comparisons, prefer Clang with the same LLVM major version reported by `rustc -vV`; check it against `clang --version`. Set `CC` and `CXX` to select versioned Clang executables or another compiler. The manifests and report record the actual compiler versions, including any LLVM version difference.

```sh
python3 bench/fetch.py
cargo build --release -p tessera-bench --features compare,compare-native \
  --target-dir target/compare-default
target/compare-default/release/compare list
target/compare-default/release/compare validate --suite smoke
target/compare-default/release/compare validate --suite release

taskset -c 2 target/compare-default/release/compare run \
  --suite release --out bench/results/release
pnpm bench report bench/results/release
```

Choose a CPU allowed by the machine's affinity policy. The recommended conditions are an idle machine, the performance governor, turbo and SMT disabled, and a run pinned to one logical CPU. The runner records these conditions and leaves host configuration to the operator.

`compare run` first invokes the sibling `memory` executable for any missing cases. Memory can also be measured separately:

```sh
taskset -c 2 target/compare-default/release/memory \
  --suite release --out bench/results/release
```

`--group TYPE/SHAPE` selects one complete group, for example `--group u64/random`. Repeating a run skips completed groups and replaces any interrupted group. An interrupted final JSONL write is discarded on resume. A change to the executable, build provenance, suite or host fingerprint requires a new output directory. Groups measured in separate directories can be reported together when their host fingerprints and build manifests agree:

```sh
pnpm bench report bench/results/part-1 bench/results/part-2
```

The report requires the complete suite. `--partial` generates a report of the completed groups during development. Results go into the first directory's `report/`, with Markdown, five SVG figures where the required cases are present, and machine-readable summaries. `bmf.json` in that directory can be uploaded with the Bencher CLI. The report reads only the result directories.

For a Rust-only comparison, omit `compare-native` and the fetch step. `list` derives supported types from the same adapters used by `validate`, timing and memory measurement.

## Portable SIMD

Build the Portable SIMD executable separately so the default executable remains available:

```sh
cargo build --release -p tessera-bench --features compare,portable-simd \
  --target-dir target/compare-simd
target/compare-simd/release/compare validate --suite smoke
taskset -c 2 target/compare-simd/release/compare run \
  --suite release --out bench/results/release
pnpm bench report bench/results/release
```

This executable measures Tessera twice and `slice::sort` on the same cases. Competitors are measured in the default build. Each build has its own A/A control; `slice::sort` anchors the ranking to the same scale. The report records the absolute anchor drift between processes and flags differences above 5%. Both builds belong to the same session and use the same source revision, toolchain and target flags.

## Cases and suites

Case IDs are `type/distribution/length/s1`. A group contains all lengths of one type and distribution. The release suite contains 220 primary cases and 40 supplementary `row_u32` cases for Clausecker's original implementation. `smoke` uses the same types and distributions at length 1,000, for 70 validation cases. The existing Criterion smoke suite retains its own regression cases and IDs.

| Type | Task | Representation |
|---|---|---|
| `u32`, `u64` | keys | Unsigned integers |
| `f64` | keys | Arbitrary float bits, ordered by `total_cmp` |
| `row_u64` | rows | `{u64 key, u32 row}`, C layout, 16 bytes |
| `record128` | records | `{u64 key, [u8; 120]}`, C layout, 128 bytes |
| `owned` | owned records | `{u64 key, String}`, 16-character payload |
| `row_u32` | supplementary rows | `{u32 key, u32 row}`, C layout, 8 bytes |

Payloads encode the original index. Wide-record payloads also fill every remaining byte deterministically. Verification checks complete payloads, not just keys and indices.

| Shape | Distributions |
|---|---|
| Random | `random` |
| Few distinct values | `d256`, `d16` |
| Frequency skew | `zipf1.1`, `zipf2`, `skew99` |
| Presorted | `sorted`, `reverse`, `sorted99` |
| Merged runs | `runs` |

Lengths are 1,000, 65,536, 1,000,000 and 10,000,000. `record128` and `owned` stop at 1,000,000. Memory is measured for every supported variant through 1,000,000 and for all `u64` cases at 10,000,000. Tessera's peak is also measured for the remaining 10M cases to provide `glidesort-sqrt` with its buffer size.

### Input generation

`case.rs` owns the seed-one generator used by all three executables and Criterion. Tests pin the little-endian input hashes from the preceding smoke implementation for `u32`, `u64`, `u128` and `f64` at two lengths for each existing distribution.

The random source is SplitMix64, starting at state 1. Each draw adds `0x9e3779b97f4a7c15`, then applies the xor-shift/multiply finalizer with shifts 30, 27, 31 and multipliers `0xbf58476d1ce4e5b9`, `0x94d049bb133111eb`, using wrapping 64-bit arithmetic. `u32` takes the low 32 bits; `u64` takes the draw; `u128` concatenates two draws, high word first; `f64` interprets the draw as bits.

- `random` draws each key over its full bit range.
- `dD` takes `next mod D`, for D = 16 or 256.
- Zipf inputs select ranks `k − 1`, for k = 1…n, from cumulative integer weights. `zipf2` uses `floor(2^62 / k²)`. `zipf1.1` uses `floor(2^48 / (k · floor(256 · k^(1/10))))`; the tenth root is computed by integer binary search. A draw selects `floor(next · total / 2^64)` and the first cumulative weight greater than it. The fixed-point root makes `zipf1.1` a reproducible approximation to the exponent 1.1.
- `skew99` uses ten spread values: `v_k = k · floor(MAX / 10)` for integers and `k · 2^1020` for floats. A first draw selects `v_0` with 99% probability; otherwise a second selects `v_(1 + next mod 9)`.
- `sorted` is 0…n−1; `reverse` reverses it. `sorted99` starts sorted and swaps adjacent elements at a spacing of `max(2, floor(n / max(1, floor(n / 100))))`.
- `runs` draws random keys, then sorts consecutive runs of `16 + next mod 2048` keys, the final run shorter. Each run reverses if the next draw's high bit is set.

The regression-only `equal`, `two_values`, `sparse8` and `sorted_tail` inputs retain their definitions and hashes. `sparse_digits` clears every second bit of a draw, leaving 16 values per 8-bit digit; `bimodal` sets bit 63 from one draw and fills bits 39…0 from the next, giving two clusters 2^63 apart over 2^40 keys each; `hot50` takes zero for half the records and `draw_high` for the rest; `d4096` takes `next mod 4096`; `far_swaps` starts ascending and swaps `max(1, len / 100)` pairs of random positions; `finite_signed` draws a random sign, an exponent field in 1000…1029 and a random mantissa for `f64`, and the full range for integers, where every value is finite. Tests pin the seed-one hashes of every distribution. Float validation additionally checks signed zeros, infinities, subnormals and NaNs of both signs with distinct payloads.

## Implementations

Rust competitors are optional dependencies behind `compare`, pinned to exact versions. Native sources are pinned by repository, commit, archive SHA-256 and license in `native/sources.toml`; `fetch.py` verifies the archives and the build verifies the extracted trees. Third-party sources stay in ignored `bench/vendor/`.

| Variant | Entry point | Stable | Supported tasks |
|---|---|---|---|
| `tessera` | `stable_sort`, `stable_sort_by_key` | yes | all |
| `std-stable` | standard stable sort, float total order | yes | all |
| `radsort` 0.1.1 | `sort_by_key` | yes | all, including owned records |
| `voracious` 1.2.0 | `voracious_stable_sort` | yes | `u32`, `u64` keys |
| `glidesort` 0.1.2 | `sort_by` | yes | all |
| `glidesort-sqrt` | `sort_with_buffer_by` | yes | all; buffer limited to Tessera's measured peak |
| `std-unstable` | standard unstable sort, float total order | no | all |
| `rdst` 0.20.14 | `radix_sort_unstable`, default features disabled | no | keys, rows, wide records |
| `clausecker` | `radixsort_permuted.c` | yes | `row_u32` |
| `fluxsort` | typed upstream instantiations | yes | keys, rows, wide records |
| `ips2ra` | sequential `ips2ra::sort` | no | keys, rows, wide records |
| `ips4o` | sequential `ips4o::sort` | no | keys, rows, wide records |
| `vqsort` | Highway `VQSort` with runtime dispatch | no | `u32`, `u64` keys |
| `x86simdsort` | `qsort` with runtime dispatch | no | `u32`, `u64` keys |
| `crumsort` | typed upstream instantiations | no | keys, rows, wide records |

Voracious 1.2.0's record path fails the stable-reference check on duplicate keys, and its float path uses partial comparison for small inputs. Those tasks are unsupported by this adapter. VQSort and x86-simd-sort's floating-point interfaces do not provide the required NaN payload and signed-zero total order; their pair interfaces use layouts different from the row tasks.

The native comparison sorts compare keys inside C or C++. Each FFI call sorts a complete buffer. The batch clock stays in Rust, and no per-element callback or binding conversion is timed. Record sizes, alignments and payload offsets are checked across the boundary. Native code uses `-O3 -DNDEBUG -march=x86-64 -mtune=generic`; x86-simd-sort's separate dispatch objects use their upstream fixed ISA targets. IPS²Ra and IPS⁴o are built without the parallel-library configuration. The runner checks the process thread count after each sort sample. The report names the SIMD dispatch targets.

On keys, stable and unstable sorts perform the same observable task. On records, unstable results are labelled separately: they answer an unstable-record task and do not enter the stable ranking or stable grid.

## Timing and statistics

For every sample, the runner restores independent buffers outside the timed region, clocks only the batch of sort calls, then verifies every output outside the clock. Stable variants must match the standard stable reference exactly. Unstable records must be ordered, contain each original index once, and preserve every record's payload.

Calibration chooses a batch from one verified warm-up sort, a minimum sample duration (5 ms by default) and a prepared-buffer cap (256 MiB). At least one buffer is used, including when that buffer exceeds the cap. `--sample-ms N` and `--memory-mib N` set these values; both are recorded. The input and stable reference are generated once per case and shared immutably. One variant's prepared buffers are resident at a time. Batch size depends on the variant's speed, so variants can have different prepared-buffer working sets and cache behavior. Scratch allocation occurs inside the timed sort call.

Each case has nine blocks. Each block shuffles all supported variants and two Tessera calls into a deterministic random order. The second Tessera sample is named `tessera-aa`. All blocks reuse the same seed-one input, so their range measures execution variability on that input rather than variability across random inputs. CI validates the smoke suite and report generation; it does not run the complete release matrix.

For each block, a variant's ratio is its time per sort divided by the mean of the two Tessera times per sort. The report gives the median ratio, per-block minimum and maximum, and median nanoseconds per element. The A/A noise range is the minimum and maximum of both Tessera times divided by their block mean, across all nine blocks. This uses the same denominator as the variant ratios and is unchanged when the two control labels are exchanged. A variant is called faster or slower only when at least eight blocks agree on that direction and its median is outside that range. For example, controls of 100 and 120 give a mean of 110 and a noise range of 100/110 to 120/110; an opponent time of 109 remains within noise. The `tessera-aa` row separately reports A2 / A1, with its own per-block bounds.

Each of the five figures states its finding as its title, says under it what is plotted, and draws one named row per item on one horizontal axis. Words appear once, in the title, column heads and axis ends; cells hold only numbers. Colour gives the kind of sort: blue for Tessera, orange for the other radix sorts and grey for comparison sorts, SIMD sorts included. When the Portable SIMD build was measured, it is a hollow blue ring in every speed figure, drawn over Tessera's dot so that equal speeds show as a ring around it. Speed is relative to `slice::sort` within the same blocks, on an axis symmetric in doublings around it; values beyond 32× point off the edge as triangles. The ranking gives each sort's speed on 1M `u64` keys as a geometric mean over the ten input shapes. The two input figures, at 1M and 1,000 `u64` keys, draw every sort on each shape as a dot and give Tessera's speed and rank, so its position in the whole field shows rather than a comparison with one opponent; keys make stability unobservable, so every sort is eligible. The element-type figure does the same per type at 1M, as a geometric mean over the ten shapes; records rank against stable sorts only. The memory figure gives each sort's peak extra heap on 10M `u64` keys, a ring for the random shape and a dot for the worst of the ten, against the input size. Tables under the figures carry Tessera's time per key and the other lengths. Every figure has a table of plotted values.

Continuous builds apply branch alignment to both feature configurations and use static linking, as described in the [continuous benchmark guide](continuous-benchmarks.md).

The [port record](../design/port.md) describes separate placement controls for source A/B investigations. The release comparison measures lengths of at least 1,000 with the default target; native-CPU builds, branch alignment, workspace mode and cold-cache investigations remain separate work.

## Results and provenance

```text
DIR/
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
