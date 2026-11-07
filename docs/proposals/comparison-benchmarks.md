# Proposal: Comparison benchmarks

Status: draft, for discussion. Date: 2026-11-07.

This proposal plans how Tessera is compared with other sorts before the Rust 0.1 release: which questions the comparison answers, where it runs, which sorts and inputs it covers, how the code is structured and how the results are shown. It also narrows the runner described in [Benchmarking](../development/benchmarking.md), which was written before the [continuous benchmarks](../development/continuous-benchmarks.md) existed and still plans for the full matrix of the research harness.

## Scope: answer four questions, not survey sorting

`sort-research-rs` compares dozens of sorts on many types, patterns and lengths, because its purpose is to study sorting. This repository needs a comparison that backs the claims in the README and in a release, and nothing else. Every implementation, type, distribution and length in the suite answers one of these questions:

| # | Question a user asks | Evidence needed |
|---|---|---|
| Q1 | Should I replace `slice::sort_by_key` with Tessera? | Time against the standard stable sort, by input shape and length |
| Q2 | Is it the fastest stable sort? | Time against the fastest stable sorts: radix sorts with O(n) and O(√n) buffers, and stable comparison sorts |
| Q3 | How does it compare with the fastest sorts of any kind? | Time against the fastest single-threaded unstable sorts, SIMD quicksorts and in-place radix sorts among them, on tasks where stability is not observable |
| Q4 | What memory does that speed need? | Peak extra memory against every variant, and time against a stable sort given the same small buffer |

Measurements of the research prototype, which are outside this repository, put Tessera several times ahead of common radix sorts, level with VQSort, the fastest single-threaded unstable sort known, ahead of most other unstable sorts, and with few equals among stable sorts. The crate has become faster than the prototype since the port, so those numbers are a starting point, not the current standing; the [continuous benchmarks](../development/continuous-benchmarks.md) show the trend but are not evidence for a comparison. This runner exists to measure where the crate stands, in a way anyone can rerun, and the [figures](#figures) are designed to show it at a glance if it holds and to show where it does not.

An implementation, type or input that answers none of them stays out. Engine paths that users do not choose between, such as `sparse8` or `sorted_tail`, stay in the smoke suite, where they guard against regressions.

The regression A/B question (is a change faster than `main`) uses the same runner with a Tessera build as the baseline, so it needs no second design. The research harness is not reproduced here, in line with [decision 0001a](../decisions/0001a-single-bsd-2-clause-license.md).

## Where it runs: one machine, one session

The 0.1 comparison runs on a machine the maintainer controls, in one session that measures every case. Nothing in it depends on GitHub Actions or a hosted service, so a contributor reruns it with the same commands on their own hardware. Running on [Bencher's hosted machines](#later-hosted-runs) is a later addition that reuses the same runner and results directory.

```sh
python3 bench/fetch.py                                     # pinned C and C++ sources, once
cargo build --release -p tessera-bench --features compare,compare-native
target/release/compare validate --suite release
taskset -c 2 target/release/compare run --suite release --out results/2026-12-01
taskset -c 2 target/release/memory --suite release --out results/2026-12-01
npm run bench:report -- results/2026-12-01
```

The Bencher CLI may wrap the run to keep a history of it, as one more Testbed of the existing project: `bencher run --testbed <machine> --adapter json --file DIR/bmf.json -- compare run ...`, or with `--dry-run` to store nothing. No step needs it.

### Results directory

The runner's output directory is the source of truth. The report reads nothing else, and a release attaches it as a compressed archive, so every published number can be recomputed.

```text
DIR/
  manifest.json      build provenance (below) and the suite version
  host.json          the host fingerprint: machine, CPU model and topology, kernel, governor, turbo, SMT, CPU affinity
  blocks.jsonl       one line per sample: case, variant, build, block, slot, batch, elapsed time, verified;
                     and one observation line at the start and end of each group
  bmf.json           summaries in Bencher Metric Format, for optional upload
  memory.jsonl       peak extra bytes per case and variant
```

The runner writes all of it: build information comes from `build.rs` and `sources.toml`, host conditions from `/proc` and `/sys`. It never changes the host.

`manifest.json` records everything that shapes the compiled code, so any number can be traced to how it was built:

| Part | Fields |
|---|---|
| Rust | commit and whether the tree was clean, `rustc -vV`, target, `RUSTFLAGS`, profile, Cargo features per executable |
| Native | C and C++ compiler and version, target, `CFLAGS`, `CXXFLAGS`, linker and relevant link flags |
| Per variant | crate version or source repository and commit, source SHA-256, and options specific to it, such as the libraries IPS⁴o is linked without |

`build.rs` compiles the native sources with fixed flags and the same baseline target as the Rust code, never `-march=native`; the IPS⁴o and IPS²Ra CMake files would otherwise optimize for the build machine. VQSort and x86-simd-sort still choose their instruction set at run time, which the report names.

The host has two kinds of state. The **fingerprint** in `host.json` describes the environment and does not change while it stays the same environment. **Observations** change from minute to minute: load average, temperature where it can be read, observed CPU frequency, memory pressure. Each group records an observation at its start and end in `blocks.jsonl`. The [Benchmarking](../development/benchmarking.md#build-profiles) page will list the recommended setup (performance governor, turbo boost and SMT off where the machine allows, an otherwise idle system, the run pinned to one core); the report prints the recorded conditions in its header and marks any that differ from the recommendation. A run interrupted part-way resumes with `--group`. Parts can be merged only if their fingerprints are equal, and the report fails otherwise. Observations never block a merge: the report warns where a group's load, temperature or frequency stands out from the run's other groups.

### Measurement

The procedure in [Benchmarking](../development/benchmarking.md#timing) stays: restore the batch outside the timed region, time the batch, verify outside the timed region, random variant order within each block, Tessera twice per block, nine blocks, the median ratio with its per-block minimum and maximum, and at least 8 of 9 blocks agreeing on a direction before a result is called faster or slower. The A/A control is reported separately, as variant `tessera-aa` with ratio A2 / A1, and a result also needs its median outside that A/A range.

Memory is measured in a separate executable with a counting global allocator, because a counting allocator changes the allocation cost that `api` mode times. It reports peak extra bytes for every variant and case up to 1,000,000, and for `u64` keys at 10,000,000, which the headline figure needs; this answers Q4 and supplies the buffer size for `glidesort-sqrt`. A Rust global allocator does not see C `malloc` or C++ `operator new`, so the memory executable also links with `--wrap` for `malloc`, `calloc`, `realloc`, `free`, `aligned_alloc` and `posix_memalign`, which libstdc++'s `operator new` reaches, and counts both into one peak.

The `workspace` mode, cold-cache mode, more seeds, the `native` and `branch-aligned` profiles stay out of the release comparison. They answer investigation questions and run with the same runner.

### Builds

The `portable-simd` feature changes only Tessera, so the competitors are measured once. Cargo cannot link one crate with two feature sets into one executable, so the Portable SIMD build is a second, smaller executable, run in the same session: Tessera's A/A pair and `slice::sort` on the same cases. `slice::sort` is the anchor that puts its results on the same relative scale as the other variants, and catches a drift in the machine between the two processes.

## Implementations

| Variant | Crate and call | Stable | Extra memory | Question | Tasks |
|---|---|---|---|---|---|
| `tessera` (×2, A/A) | `tessera_sort::stable_sort`, `stable_sort_by_key` | yes | O(√n) | baseline | all |
| `std-stable` | `slice::sort`, `sort_by_key` (driftsort); `f64` by `total_cmp` | yes | up to n | Q1 | all |
| `radsort` | `radsort` 0.1: `sort`, `sort_by_key` | yes | n | Q2 | keys, rows, records |
| `voracious` | `voracious_radix_sort` 1.2: `voracious_stable_sort`, single-threaded, `Radixable` implemented for the bench records | yes | n | Q2 | keys, rows, records |
| `glidesort` | `glidesort` 0.1: `sort_by_key` | yes | default buffer | Q1, Q2 | all |
| `glidesort-sqrt` | `glidesort::sort_with_buffer_by_key` with a buffer as large as Tessera's measured peak scratch for that case, allocated in the timed region | yes | O(√n) | Q4 | all |
| `std-unstable` | `slice::sort_unstable`, `sort_unstable_by_key` (ipnsort) | no | O(1) | Q3 | all |
| `rdst` | `rdst` 0.20, `default-features = false` (single-threaded), `radix_sort_unstable` | no | n | Q3 | keys, rows, records |

The Rust crates alone would leave out the fastest single-threaded sorts known, which are written in C and C++. A comparison without them would favor Tessera by omission, so the runner also links them, built from sources fetched at pinned commits:

| Variant | Source | Stable | Extra memory | Question | Tasks |
|---|---|---|---|---|---|
| `clausecker` | Robert Clausecker's [`radsort`](https://github.com/clausecker/radsort), `radixsort_permuted.c`: the O(√n) stable LSD sort Tessera's block partition comes from | yes | O(√n) | Q2, Q4 | keys, rows |
| `fluxsort` | [scandum/fluxsort](https://github.com/scandum/fluxsort), stable quicksort and merge hybrid | yes | n | Q1, Q2 | keys, rows, records |
| `ips2ra` | [IPS²Ra](https://github.com/ips4o/ips2ra), in-place super scalar radix sort: `ips2ra::sort`, compiled without its parallel libraries | no | O(1) in n | Q3 | keys, rows |
| `ips4o` | [IPS⁴o](https://github.com/ips4o/ips4o), in-place samplesort: `ips4o::sort`, compiled without its parallel libraries | no | O(1) in n | Q3 | keys, rows, records |
| `vqsort` | VQSort from [Highway](https://github.com/google/highway), SIMD quicksort with runtime dispatch | no | O(1) | Q3 | `u32`, `u64`, `f64` keys; `{u64, u64}` pairs |
| `x86simdsort` | [x86-simd-sort](https://github.com/numpy/x86-simd-sort), AVX2 and AVX-512 quicksort, runtime dispatch | no | O(1) | Q3 | keys; key-value pairs |
| `crumsort` | [scandum/crumsort](https://github.com/scandum/crumsort), branchless unstable quicksort | no | O(1) | Q3 | keys, rows, records |

These variants are called through small C shims written for this repository, one exported function per algorithm and element type, such as `bench_ips4o_sort_u64(ptr, len)`. Each call sorts a whole buffer, and the batch loop and clock stay in Rust, so the comparison measures the same thing as for the Rust variants; a call costs a few nanoseconds, negligible at the smallest length in the suite (1,000). This is not the per-call FFI the protocol excludes, which times a binding's conversion of each call. Records are passed as `#[repr(C)]` structs with the same layout on both sides, and comparison sorts receive an inlined key comparison in C++, not a callback into Rust.

Dovetail sort, listed in [Benchmarking](../development/benchmarking.md#c-runner), is a parallel algorithm and is not included. VQSort and x86-simd-sort choose their instruction set at run time, like Tessera's Portable SIMD build, so the measuring machine's CPU decides; the report names the level each one dispatched to.

Rules:

- **Same task only.** A table compares only variants that perform the task. A variant whose output fails validation for a case, for example because it orders floats differently from `total_cmp`, is listed as unsupported for that type rather than compared with different semantics. `radsort` documents a float order that matches `total_cmp` on NaN signs; `voracious` and `rdst` are checked by validation.
- **Stability where it is observable.** On `keys`, equal keys are indistinguishable, so a stable and an unstable sort produce the same output and compete directly; every mark still shows which kind it is. On `rows`, `records` and `owned`, unstable variants are listed separately, because they do not perform the task.
- **Default configuration.** No variant enables crate features that change its algorithm.
- **Pinned versions.** Crates are pinned with `=x.y.z` in `bench/rust/Cargo.toml`, C and C++ sources by commit and SHA-256; both are upgraded deliberately, and the version or commit appears in the report.
- **Sources stay out.** Third-party C and C++ sources are fetched into an ignored directory, never committed or copied into the shims ([decision 0001a](../decisions/0001a-single-bsd-2-clause-license.md)). Each license is checked when its commit is pinned, because the executable links the code and the results archive names it.
- **No parallel sorting.** Tessera is single-threaded, so the comparison is too. Every variant runs on one thread: crates without their thread features (`rdst` without `multi-threaded`, `voracious` without `voracious_multithread`), IPS²Ra and IPS⁴o through `ips2ra::sort` and `ips4o::sort`, compiled by `build.rs` without TBB, OpenMP or the threads library, which is what their CMake options `IPS2RA_DISABLE_PARALLEL` and `IPS4O_DISABLE_PARALLEL` do, and nothing that needs a thread pool. Sorts that are only parallel, such as Dovetail sort, are left out. The runner checks this: it counts the process's threads after every variant's first sort and fails the run if any variant started one.
- **Not included.** `sort_by_cached_key` (it answers a key-cost question, not a sort question); string and comparator sorts (Tessera does not support them).

## Inputs

### Types

| Task | Type | Why |
|---|---|---|
| `keys` | `u32`, `u64` | The common integer widths; `u32` fits more keys per tile |
| `keys` | `f64` | Float encoding and `total_cmp` order, including NaNs, infinities and subnormals |
| `rows` | `{u64 key, u32 row}` | An index sort, the common database use |
| `records` | `{u64 key, [u8; 120]}`, 128 bytes | Wide records, where moves cost more than key reads and an O(n) buffer is large |
| `owned` | `{u64 key, String}` | Records that are not `Copy`; most radix crates cannot sort them |

`u128`, `i64`, the 8- and 16-byte records and `f32` stay in the smoke suite and the extended suite: they test encodings and thresholds, and no user chooses between sorts on them differently from `u64` or `u32`.

### Distributions

Each distribution is one data shape a user might have. The generator is the one in [Benchmarking](../development/benchmarking.md#inputs) and in `smoke.rs`.

| Shape | Distributions | What it shows |
|---|---|---|
| No structure | `random` | Worst case for adaptivity; every pass is needed |
| Few distinct values | `d256`, `d16` | Varying-bit skipping against radix sorts that run every digit, and comparison sorts that detect duplicates |
| Skewed repetition | `zipf1.1`, `zipf2`, `skew99` | Real-world frequency skew, mild to extreme |
| Presorted | `sorted`, `reverse`, `sorted99` | Adaptivity; comparison sorts are strong here and LSD radix sorts are not |
| Merged sources | `runs` | Ascending and descending runs, which run-adaptive merges exploit |

Removed from the comparison: `skew50` and `sorted95` (between two shapes already covered), `equal` (both families finish in one scan), `two_values`, `sparse8` and `sorted_tail` (engine paths covered by the smoke suite).

### Lengths

| Length | Why |
|---|---|
| 1,000 | Small sorts, mostly in-tile; reported, but code placement moves results by up to 41% at n ≤ 128 on the development machine, so the smallest lengths are left out |
| 65,536 | Eight tiles: one block-partition level, data in L2 |
| 1,000,000 | The size at which engine decisions were measured; data in L3 or beyond |
| 10,000,000 | Out of cache, where the O(n) buffer of other radix sorts costs bandwidth; `keys` and `rows` only, because 128-byte and owned records at this size need more than 1 GiB per buffer |

The suite has 5 types × 10 distributions × 4 lengths, less the 20 large cases for `records` and `owned`: about 180 cases. With up to 16 variants per case, a first estimate, to be replaced by the first run: cases up to 1,000,000 need about 5 s each and each 10,000,000 case about 60 s, so a full run takes one and a half to two hours, and the Portable SIMD executable adds about a fifth.

## Code structure

```text
bench/rust/
  Cargo.toml          competitors are optional dependencies behind the `compare` feature
  src/lib.rs
  src/case.rs         CaseId, Distribution, Type; the input generator
  src/check.rs        order, permutation and stability check against the reference
  src/variant.rs      the variant table and one adapter per implementation
  src/run.rs          batches, blocks, A/A, ratios
  src/suite.rs        suite definitions: exact case IDs and their groups
  src/host.rs         reads the host conditions from /proc and /sys
  src/bin/compare.rs  CLI: list, validate, run [--group G] --out DIR
  src/bin/memory.rs   counting allocator, writes memory.jsonl
  benches/smoke.rs    the Criterion smoke suite, now using `case`
  native/             C and C++ shims, one exported function per algorithm and element type
  native/sources.toml pinned repository, commit and SHA-256 of each third-party source
  build.rs            compiles the shims and fetched sources when `compare-native` is on
bench/fetch.py        downloads and checks the pinned sources into the ignored bench/vendor/
```

Each module owns one decision, so that a change touches one place:

- **`case` owns the inputs.** The generator moves out of `smoke.rs` into the library, and the smoke suite, the comparison and the memory executable all use it. The move must leave every smoke input byte-identical, because Bencher history is keyed by case ID; a test pins the hash of a few inputs per distribution so the generator cannot drift silently. The cross-language golden hashes in `conformance/vectors/` wait until a second language has a generator.
- **`variant` owns capabilities.** A variant is a name, a stability flag and a function `prepare(&Case) -> Option<Box<dyn Kernel>>`. Returning `None` is the only way to say "unsupported", so `list` derives the capability table by asking every variant, and there is no separate table to keep consistent. `Kernel` has `restore`, `run_batch` and `check`. It is implemented by one generic struct per element type, so each sort inside a batch is a monomorphized call and the dynamic call happens once per batch.
- **`check` owns correctness.** Every record carries its original index; `check` compares keys with the reference order and, for stable variants, the indices of equal keys. `validate` runs it for every case and variant, and `run` runs it after every sample.
- **`run` owns the statistics.** It is the only module that knows about blocks, slots and the A/A pair, and it produces the ratios that `compare` writes.
- **`native` owns the foreign boundary.** The shims are the only code that knows a third-party C or C++ signature. Rust sees `extern "C" fn(*mut T, usize)` per element type, and the record layouts are checked by a test that compares `size_of` and field offsets reported by the shim. A shim never allocates on behalf of Rust or holds state between calls, so a failed sort cannot leave anything behind.
- **`suite` owns the plan.** Suites are Rust constants of exact case IDs, divided into **groups** that each run under one block plan, checked by a test that every case of a suite appears in exactly one group. Groups make a run resumable and let one group be rerun alone; they are not a time limit. A text format shared with other languages is added when another language needs it.
- **`host` owns the conditions.** It is the only code that reads the machine's state, and the runner never changes that state: tuning the host is the operator's choice, recording it is the runner's job.

Two optional features keep third-party code out of the workspace MSRV run (`cargo +1.85 test --workspace`), the Miri run and the smoke build: `compare` adds the Rust crates, and `compare-native` adds the C and C++ variants, which need `bench/fetch.py` and a C++17 compiler. CI runs `validate` with both on stable. `build.rs` never downloads anything; it fails with the fetch command if `compare-native` is on and the sources are missing.

The report is a TypeScript module in `bench/report/`, run with `npm run bench:report -- DIR` on a results directory. It imports the chart and SVG code of `.github/scripts/bencher/` rather than living there, because it is not tied to CI. Its output is what the README and the Performance page show. The `bench/cases/` directory planned in [`bench/README.md`](../../bench/README.md) is no longer needed; suites live in `suite.rs`.

## Figures

The comparison has eight dimensions: variant, stability, task, type, distribution, length, build and memory. No single chart shows all of them, and a chart that tries is read by nobody. Each figure instead holds most dimensions fixed, varies two, and answers one question for one reader.

| Reader | Time they give it | Where | What they need |
|---|---|---|---|
| A visitor deciding whether to look further | 30 seconds | README, Performance section | The one claim, and that it holds across input shapes |
| A user deciding whether Tessera suits their data | Minutes | A Performance page on the documentation site, regenerated per release and linked from the README | How it behaves for their record type, length and data shape, and what it costs in memory |
| A contributor checking a change | Per PR | The existing continuous benchmark report in PR comments | Regressions against `main`; not part of this comparison |

| # | Figure | Question | Varies | Fixed | Form | Where |
|---|---|---|---|---|---|---|
| 1 | Speed ranking | Q1–Q3 | variant × length | `u64` keys, mean over shapes | ranked dot plot, emphasis | README |
| 2 | Against the fastest stable sort | Q2 | distribution × length | the fastest other stable variant per cell | grid, diverging | README and page |
| 3 | Against the fastest unstable sort | Q3 | distribution × length | `keys` only, the fastest unstable variant per cell | grid, diverging | page |
| 4 | Scaling with length | Q1, Q2 | length × variant | random input, one panel per type | lines, emphasis | page |
| 5 | Memory growth | Q4 | length × variant | `u64` keys and 128-byte records | lines, log-log | page |

Figures 2 and 3 use the same grid, and figures 4 and 5 the same line chart, so a reader learns two forms, not five. Everything else is a table: the full results per type, with every variant as a column, collapsed by type on the Performance page. Builds (Default against Portable SIMD), the `workspace` mode and lengths below 1,000 get no figure: they matter to contributors, not to someone choosing a sort, and the tables carry them.

### Rules for every figure

- **The case selection is fixed before any data exists.** The type, length and aggregation below are part of this proposal. Choosing the most flattering case after seeing results would make the figure an advertisement, and a reader who reruns the suite would find out.
- **The figure shows what the data says.** If a competitor beats Tessera, its row ranks above Tessera's and its cell is orange. The headline numbers are computed by fixed rules, not picked.
- **Every number in a figure is also in text.** Below each figure, a collapsed table lists every plotted value, as the [documentation rules](../../CLAUDE.md#documentation-rules) require for agents and screen readers.
- **Default is Tessera; Portable SIMD is a second row.** Every figure shows the Default build as Tessera, because that is what `cargo add` gives, and it is the conservative comparison: VQSort and x86-simd-sort dispatch to AVX2 or AVX-512 where the CPU has them, while Default uses the baseline x86-64 instruction set and compiler auto-vectorization. Figure 1 adds a second Tessera row for the `portable-simd` build, ranked like any other, so users see what the feature buys; figure 3's caption gives the Portable SIMD result beside Default's, and the tables have both. This holds however large or small the difference turns out to be. The Portable SIMD row comes from its own smaller executable, anchored by `slice::sort` (see [Builds](#builds)).
- **One look across figures.** The SVGs reuse the report's styles in `.github/scripts/bencher/svg.ts`: the same surfaces and inks, light and dark through `prefers-color-scheme` inside the SVG, 720 px wide. Blue `#4a80e6` means Tessera or less time, orange `#e8622f` more time, and every other mark is the report's muted grey. The blue and orange pair passes the colour-vision and contrast checks on both surfaces (`#fafaf8`, `#1e1e1d`); grey is de-emphasis, not a series colour, and identity never depends on colour alone, because every mark is labelled.

### Figure 1: speed ranking

This is the headline. The claim is speed: ahead of every other stable sort and level with the fastest unstable ones. A ranking shows that in one look, and it stays readable with fifteen variants where a bar chart per length would not.

```text
  Tessera  3.1× faster than slice::sort  ·  1.0× VQSort  ·  2.6× the next stable sort
  ───────────────────────────────────────────────────────────────────────────────────
                        65,536            1,000,000          10,000,000     extra memory
  ● Tessera + simd    ──────────●       ──────────●        ──────────●      0.4 MB
  ● Tessera           ─────────●        ─────────●         ─────────●       0.4 MB
  ○ vqsort            ────────○         ─────────○         ─────────○        0
  ○ x86simdsort       ───────○          ────────○          ────────○         0
  ○ ips2ra            ──────○           ───────○           ───────○          small
  ● voracious         ─────●            ─────●             ────●            80 MB
  ● clausecker        ────●             ────●              ────●             0.4 MB
  ● radsort           ───●              ───●               ──●              80 MB
  ○ sort_unstable     ──○               ──○                ──○               0
  ● fluxsort          ──●               ──●                ──●              80 MB
  ● slice::sort       ─●                ─●                 ─●               40 MB
                      1×     4×         1×     4×          1×     4×
                     speed relative to slice::sort (log), higher is better
  ● stable  ○ unstable        u64 keys, geometric mean over 10 input shapes
```

The values in this sketch are placeholders that show the layout only.

| Element | Encoding |
|---|---|
| Case | `u64` keys, where stability is unobservable and every variant runs, so stable and unstable sorts compete on equal terms. Three lengths: 65,536 (in cache), 1,000,000 and 10,000,000 (out of cache) |
| Rows | One per variant, ranked by speed at 10,000,000, fastest first. The order is the result, so it is computed, never arranged |
| Columns | One per length, all on the same log scale: speed relative to `slice::sort`, the geometric mean over the 10 distributions of `t(slice::sort) / t(variant)`. Each ratio is paired within a block through Tessera's interleaved A/A pair |
| Marks | A thin stem from 1× to a dot. Filled for stable, hollow for unstable, so stability is a shape, not a colour. Tessera's row is blue with a bold label; the others are grey. A thin vertical line through Tessera's dot in each column makes "who is ahead" a matter of left or right |
| Memory | A text column, not an axis: peak extra memory at 10,000,000. It shows that the speed does not come from a second copy of the data, without a second scale |
| Header | Three large numbers computed from the Default build's plotted values at 10,000,000: Tessera against `slice::sort`, against the fastest unstable variant, and against the fastest other stable variant |
| Not shown | Spread across input shapes and other types: figures 2 to 4 carry them, and the caption says so |

### Figure 2: against the fastest stable sort

The first figure averages over input shapes; this one shows the shapes, for the stable claim (Q2). It is a grid with the 10 distributions as rows, grouped by shape as in [Distributions](#distributions), and the 4 lengths as columns, in two panels side by side: `u64` keys and `{u64 key, u32 row}` rows.

| Element | Encoding |
|---|---|
| Comparison | Tessera against the fastest **other stable** variant in that cell, the hardest fair opponent. Against `slice::sort` alone the figure would say little, and figure 1 already shows that comparison |
| Cell colour | Diverging: blue where Tessera takes less time, orange where it takes more, intensity by the log of the ratio, capped at 4×. A cell whose result is within noise by the protocol's rule (outside the A/A range and at least 8 of 9 blocks agreeing) is neutral grey, so the colour carries the statistical verdict |
| Cell text | The ratio, such as `1.8×`; where Tessera loses, also the winner's name, such as `0.7× fluxsort`, so the reader sees who beats it without a legend |
| Order | Rows in the fixed distribution order, never sorted by result, so the same grid compares across releases |

On the Performance page the grid adds `records` and `owned` panels; the README shows the first two.

### Figure 3: against the fastest unstable sort

How does Tessera compare with the fastest sorts of any kind (Q3)? The figure 2 grid for `u32`, `u64` and `f64` keys, where stability is unobservable, with each cell comparing Tessera with the fastest **unstable** variant in that cell and naming it. This is the hardest comparison in the suite, and the grid shows its wins and losses by shape and length rather than a single average. Same colour rule, noise rule and fixed row order as figure 2.

### Figure 4: scaling with length

Where does Tessera overtake comparison sorts, and does the lead hold out of cache? One panel per type (`u64` keys, rows, 128-byte records, owned), random input only, because figures 2 and 3 already cover shapes. The horizontal axis is the length on a log scale (1,000 to 10,000,000), the vertical axis nanoseconds per element, so a linear-time sort is a flat line and an n log n sort rises. These are absolute times compared across lengths, which is valid because a release run measures every case on one machine in one session; the report refuses to draw this figure from a results directory whose groups were measured on different hosts. Tessera is the blue line, `slice::sort` a dark grey line, the other stable variants light grey; every line is labelled at its right end. Panels share the horizontal axis and each has its own vertical axis, stated in its title, because a 128-byte record costs several times a key. Unstable variants are left out to keep each panel to at most eight lines; they are in figures 1 and 3.

### Figure 5: memory growth

How does the extra memory grow? Peak extra bytes against length, both on log scales, for `u64` keys and 128-byte records. A sort with an O(n) buffer is a line of slope 1, Tessera's O(√n) bound a line of slope ½, and in-place sorts stay near the bottom, so the asymptotic claim is visible as a slope, not asserted. A dashed line marks the size of the input itself. Same marks and labels as figure 4. On the Performance page it sits next to the `glidesort-sqrt` rows of the tables, which show what a comparison sort does with the same buffer.

### Producing the figures

The report module renders every figure as SVG from the results directory, alongside the report tables. Before real data exists, its tests render them from a synthetic fixture, as `fixtures/chart.svg` does for the continuous report, so the layout is settled and reviewed first: long variant names, close values in one column and a Tessera row that is not first must all render legibly. The documentation site can later show the same data with hover detail, but the README images are static SVGs, so nothing in them depends on hovering.

## Steps

| Step | Delivers | Done when |
|---|---|---|
| 1. Shared generator | `case.rs` in the library; `smoke.rs` uses it | Smoke inputs hash identically before and after; Bencher shows no new IDs |
| 2. Runner | `variant`, `check`, `run`, `suite`, `host`, `compare` with Tessera and the std sorts | `compare run` writes the results directory on a contributor's machine with no network access; self-test: a deliberately wrong variant that also mutates its input is caught, and every variant receives the original input; `validate` passes in CI |
| 3. Rust competitors | The crate adapters behind `compare` | `list` shows the capability table; `validate` passes or marks unsupported types |
| 4. Native competitors | `fetch.py`, `build.rs`, the shims behind `compare-native` | Every license is recorded with its commit; the executable builds on Linux x86-64; `validate` passes; the report names each SIMD sort's dispatched level |
| 5. Memory | `memory` executable | Peak bytes for every variant and case up to 1,000,000, and `u64` keys at 10,000,000 |
| 6. Report | The report module with its figures and tables | The figures render from a synthetic fixture and are reviewed before any real run; a full run's directory produces the report alone, and a second run on the same machine reproduces its verdicts |
| 7. Release report | The first run on the release candidate, on the maintainer's machine | The README and the Performance page show its figures; the release attaches the results directory; the report names the machine, its conditions, the variant versions and the A/A ranges |

## Changes to existing documents

When step 2 lands, [Benchmarking](../development/benchmarking.md) is rewritten to describe the runner that exists: the four questions, the results directory, the reduced matrix, the recommended host setup and the TypeScript report. The parts this proposal defers (the `core` and `extended` suites, cold-cache mode, cross-language JSONL) are removed or moved to [`ROADMAP.md`](../../ROADMAP.md), where the C++ milestone takes over the golden hashes. The C++ runner's list of baselines moves here, because they now run in the Rust runner; the C++ runner keeps the job of measuring the C++ implementation. The rule against per-call FFI is reworded to say what it excludes: timing a binding's per-call conversion, not a native sort that the runner calls once per buffer. The choice to measure release comparisons on a maintainer's machine, with hosted runs as a later addition, becomes a decision record.

## Later: hosted runs

Running the same comparison on Bencher Bare Metal's `intel-v1` machines would let anyone rerun it on the same public hardware, from a pinned image. It is not part of 0.1, because the free plan's limits would shape the runner: what follows records what that addition needs, so that the 0.1 design does not block it.

| Limit | Consequence |
|---|---|
| One job at a time, 5 minutes each | Groups become **shards** that finish in under 240 s each, submitted one after another through the existing `bencher-bare-metal` concurrency group, by manual dispatch or a release tag, never by PRs |
| Consecutive jobs may run on different machines of the same spec | Ratios stay comparable across shards, but absolute times do not: figure 4 needs one shard per type with every length, or it is labelled descriptive |
| 65,535 Metrics per day | Summaries take about 3,000 Metrics per Measure for the whole suite; per-block values stay in the job output, not in Metrics |
| No network access on shared runners | The image holds the runner and every competitor, built on the GitHub host; the image digest goes into `manifest.json` |

The Bencher model for those runs:

| Concept | Use |
|---|---|
| Branch | One fixed branch, `compare`, with `--hash <commit>`: the shards of one run share a branch and hash and so form one version, and successive runs form that branch's history |
| Testbed | One Testbed, `intel-v1-compare`, spec `intel-v1`. A Testbed is the measurement environment, not a build, so the build is a segment of the benchmark ID. The continuous suite keeps a Testbed per build ([decision 0018](../decisions/0018-benchmark-feature-configurations.md)) because thresholds are scoped by Testbed; the comparison sets no thresholds |
| Benchmark | `compare/<case_id>/<variant>/<build>`, for example `compare/u64/random/1000000/s1/radsort/default` |
| Measures | `ratio`, the median over blocks with the per-block minimum and maximum as its bounds; `latency`, the median nanoseconds per element. The A/A control is its own benchmark, `tessera-aa`, never another metric's bounds |
| Iterations | One `bencher run` iteration per shard. Blocks are the runner's statistical unit: each `--iter` restarts the executable, and `--fold median` would fold absolute times across processes, not paired ratios |

Bencher keeps each finished job's stdout, stderr, exit code and output files, so the runner would print the BMF on stdout and the per-block samples on stderr, and the workflow would rebuild the results directory from them, writing `manifest.json` itself from the build. Bencher's [Project Jobs API](https://bencher.dev/docs/api/projects/jobs/) returns a finished job's output to callers with `view` permission but also lists viewing a job as a Bencher Plus feature. Before this addition is built, a feasibility shard settles three points:

1. **Output.** Whether the free plan returns job output to the project's API key. If not, the per-block ratios go into nine fixed Measures `ratio-b0` to `ratio-b8` on the same benchmarks, about 30,000 more Metrics per run, which leaves room for one full run per day.
2. **Noise.** The A/A range within a job, the spread of one shard across three jobs, and `bencher noise --format json` run in the same image.
3. **Host conditions.** Bencher's runner documents ASLR, SMT, turbo boost and the NMI watchdog off and the performance governor by default; whether this holds on the shared runners is confirmed and recorded in `host.json`, or `host.json` records the host as unknown apart from the spec.

Bencher's self-hosted runners and a self-hosted Bencher server could serve the same jobs on other machines; they are not planned.

## Open questions

- **A u64 key with 32 varying bits.** Many real `u64` keys are IDs or timestamps whose high bits are constant. A `low32` distribution would show varying-bit skipping on such data; it is not in the generator yet.
