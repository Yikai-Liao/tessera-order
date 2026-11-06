# Benchmarking

This document defines how Tessera is measured. The runner described here is not built yet; until it is, published numbers come from the research prototype and say so. A separate [continuous benchmark suite](continuous-benchmarks.md) runs on Bencher Bare Metal for regression triage; it uses this document's case IDs but not its timing and statistics.

## Purposes

| Purpose | Question | Tool |
|---|---|---|
| Port acceptance (once) | Does the ported crate match the research prototype? | The research repository's `radix_ab` harness, with the crate added as a variant in the same binary |
| Regression A/B | Is a change faster or slower than the previous version? | The Rust runner, with both versions in one process |
| Release report | Where is Tessera faster or slower than other sorts? | The Rust runner on the fixed `core` suite |

## Runner

`bench/rust` is an unpublished binary crate with three commands:

| Command | Action |
|---|---|
| `list` | Lists variants and their capabilities: stability, supported tasks, workspace support |
| `validate --suite S` | Checks order, permutation and, for stable variants, the order of equal keys, for every case and variant |
| `measure --suite S --out DIR` | Times the suite and writes `samples.jsonl` and `manifest.json` |

Cases are selected by exact `case_id`, `type/distribution/length/seed`, for example `u64/random/4096/s1` or `row_u64/zipf2/65536/s1`, never by pattern matching. Distribution names: `random`, `d256` and `d16` (uniform over 256 and 16 values), `zipf1.1` and `zipf2`, `sorted`, `reverse`, `sorted95` and `sorted99`, `runs`, `equal`.

Variants include the working-tree crate, a baseline release linked under a renamed dependency (`tessera_base = { package = "tessera-sort", git = "...", tag = "rust/v0.1.0" }`), `slice::sort_by_key`, `slice::sort_unstable_by_key`, and other radix sorts from crates.io, such as `radsort` and `rdst`, where they support the task. If Cargo rejects the same package from a path and a git source, the two versions are built as separate binaries and a driver script alternates them.

## Inputs

Inputs are generated from `(distribution, length, seed)` by a generator specified precisely enough to be reimplemented in other languages:

- the random source is SplitMix64;
- distributions use integer arithmetic only, so no result depends on a math library;
- float inputs are built from bit patterns and include `±0`, `±inf`, NaNs with several payloads, and subnormals;
- `conformance/vectors/` stores the SHA-256 of every `core` input, and runners verify it at startup.

Inputs are generated in memory and never committed.

## Tasks

A result table only compares implementations performing the same task. An implementation that cannot perform a task is listed as unsupported.

| Task | Types |
|---|---|
| `keys` | `u32`, `u64`, `u128`, `i64`, `f64` |
| `rows` | `{u32 key, u32 row}`, `{u64 key, u32 row}`, `{f64 key, u32 row}` |
| `records` | `{u64 key, u64 payload}` (16 bytes), `{u64 key, [u8; 120]}` (128 bytes) |
| `owned` | `{u64 key, String}` |

Payloads carry the record's original index so that permutation and stability can be checked.

## Suites

| Suite | Contents | Use |
|---|---|---|
| `smoke` | A few types, distributions and lengths | Every change and CI; currently implemented as the [continuous suite](continuous-benchmarks.md) |
| `core` | All tasks × all core distributions × all core lengths | Releases and regression A/B |
| `extended` | More seeds, float edge cases, compound keys, wide records, cold-cache mode | Investigations |

Core distributions: random, uniform over 256 values, uniform over 16 values, Zipf (s = 1.1 and 2.0), sorted, reverse, 95 % and 99 % sorted, runs, all equal.

Core lengths: 8, 16, 32, 64, 256, 1K, 4K, 16K, 64K, 256K, 1M, 10M, plus points around engine thresholds (the insertion limit and the tile capacity for each record size), computed from the crate's constants.

## Timing

Each sample follows one procedure:

1. Outside the timed region, restore every buffer in the batch to the original input.
2. Start the clock.
3. Sort the batch of independent buffers.
4. Stop the clock and record the batch's nanoseconds and number of sorts.
5. Outside the timed region, check each buffer's output hash (keys and original indices) against a reference computed with a stable standard-library sort.

- The batch size is chosen from a minimum sample duration and a cap on prepared memory; both are recorded. The provisional defaults are 5 ms and 256 MiB.
- Variants run in a random order within each block. A block runs every variant once; a run has 9 blocks by default.
- The baseline appears twice in each block, which gives an A/A noise range.
- The allocation mode is a separate dimension: `api` calls the free functions and includes scratch allocation; `workspace` uses a prepared workspace. Variants without a workspace only appear in `api` tables.
- Cold-cache measurements are a separate mode in the `extended` suite: before each sort, the runner walks a buffer twice the size of the last-level cache.

## Statistics

- The main statistic is the median over blocks of `variant / mean(baseline A1, baseline A2)`, reported with the A/A ratio `A2 / A1`.
- A result is reported as faster or slower only if the median lies outside the A/A range and at least 8 of 9 blocks agree on the direction. Otherwise it is reported as within noise.
- Comparisons across builds or source copies also include a placement control, a second copy of the same source, whose deviation counts as noise like the A/A range. On the development machine, placement alone has moved results by up to 41% at n ≤ 128 and up to 1.9× on ordered inputs ([port record](../design/port.md#rebuild-and-branch-alignment-check)).
- Reports also give nanoseconds per element, recomputed from batch time and element count.

## Raw data

`samples.jsonl` holds one sample per line:

```json
{"schema":1,"run_id":"2026-11-20-a","case_id":"u64/random/4096/s1","task":"keys",
 "variant":"tessera","kernel":"tessera-stable","alloc_mode":"api","block":2,"slot":3,
 "batch":64,"elements":262144,"elapsed_ns":1523840,"verified":true}
```

`manifest.json` records each variant's `implementation`, `package_version`, full `git_commit`, actual `kernel` and, for uncommitted code, source hashes; the runner source and binary hashes, `rustc -vV`, `RUSTFLAGS`, LTO, CPU model and pinning, kernel version, the case plan and the input hash check.

Reports are regenerated from raw data. The report script fails on missing or duplicate samples, unverified samples, manifest mismatches and input hash mismatches. Release results are attached to GitHub releases as compressed archives; the repository keeps the generated report.

## Build profiles

| Profile | Flags | Role |
|---|---|---|
| `default` | `cargo build --release` | Main report: what crate users get |
| `native` | `-C target-cpu=native`, fat LTO | Additional column |
| `branch-aligned` | `default` plus `-C llvm-args=-x86-branches-within-32B-boundaries` (x86-64 only) | A/B comparisons on CPUs with Intel's JCC erratum (Skylake-derived cores); not a report column |

On the development machine, the `branch-aligned` profile removed placement swings of up to 1.9× on ordered inputs, but not the swings at n ≤ 128, so the placement control stays required. It changes absolute times by up to −30% and +69%, so `default` remains what reports show.

## Memory

Memory is measured in a separate run with a counting allocator: allocation count, requested bytes, peak extra bytes and `Workspace::retained_bytes()`. Fixed stack storage is documented from the crate's constants. Unmeasured quantities are reported as unmeasured, never estimated from process RSS.

## Harness self-tests

- A deliberately wrong variant that also mutates its input proves that every variant, block and buffer receives the original input, and that validation catches the error.
- A round trip through `samples.jsonl` and the report script proves that durations, units and sort counts survive.

## C++ runner

The C++ runner implements the same input specification and writes the same `samples.jsonl` format. It has its own timing loop rather than Google Benchmark's summaries, and fetches its baselines (`std::sort`, `std::stable_sort`, IPS²Ra, IPS⁴o, VQSort, x86-simd-sort, Fluxsort, Dovetail) at pinned commits. Cross-language comparisons never time per-call FFI: each runner sorts and times whole batches inside its own language.

## Baselines across releases

Baseline releases are referenced by their language tag, for example `rust/v0.1.0` (see [`RELEASING.md`](../../RELEASING.md)). A cross-language report states whether it compares one repository commit or each language's latest release, and names the kernel each interface actually ran.

The one-time port acceptance used the research repository's harness, which stays outside this repository; its results are in the [port record](../design/port.md).
