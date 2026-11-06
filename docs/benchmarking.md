# Benchmarking

This document defines how Tessera is measured. The runner described here is being built; until then, published numbers come from the research prototype and say so.

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

Cases are selected by exact `case_id`, for example `u64/random/4096/s1`, never by pattern matching.

Variants include the working-tree crate, a baseline release linked under a renamed dependency, `slice::sort_by_key`, `slice::sort_unstable_by_key`, and other radix sorts from crates.io where they support the task.

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
| `smoke` | A few types, distributions and lengths | Every change and CI |
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

- The batch size is chosen from a minimum sample duration and a cap on prepared memory; both are recorded.
- Variants run in a random order within each block. A block runs every variant once; a run has 9 blocks by default.
- The baseline appears twice in each block, which gives an A/A noise range.
- The allocation mode is a separate dimension: `api` calls the free functions and includes scratch allocation; `workspace` uses a prepared workspace. Variants without a workspace only appear in `api` tables.
- Cold-cache measurements are a separate mode in the `extended` suite.

## Statistics

- The main statistic is the median over blocks of `variant / mean(baseline A1, baseline A2)`, reported with the A/A ratio `A2 / A1`.
- A result is reported as faster or slower only if the median lies outside the A/A range and at least 8 of 9 blocks agree on the direction. Otherwise it is reported as within noise.

## Raw data

`samples.jsonl` holds one sample per line:

```json
{"schema":1,"run_id":"2026-11-20-a","case_id":"u64/random/4096/s1","task":"keys",
 "variant":"tessera","kernel":"tessera-stable","alloc_mode":"api","block":2,"slot":3,
 "batch":64,"elements":262144,"elapsed_ns":1523840,"verified":true}
```

`manifest.json` records variant versions and commits, runner source and binary hashes, `rustc -vV`, `RUSTFLAGS`, LTO, CPU model and pinning, kernel version, the case plan and the input hash check.

Reports are regenerated from raw data. The report script fails on missing or duplicate samples, unverified samples, manifest mismatches and input hash mismatches. Release results are attached to GitHub releases as compressed archives; the repository keeps the generated report.

## Build profiles

| Profile | Flags | Role |
|---|---|---|
| `default` | `cargo build --release` | Main report: what crate users get |
| `native` | `-C target-cpu=native`, fat LTO | Additional column |

## Memory

Memory is measured in a separate run with a counting allocator: allocation count, requested bytes, peak extra bytes and `Workspace::retained_bytes()`. Fixed stack storage is documented from the crate's constants. Unmeasured quantities are reported as unmeasured, never estimated from process RSS.

## Harness self-tests

- A deliberately wrong variant that also mutates its input proves that every variant, block and buffer receives the original input, and that validation catches the error.
- A round trip through `samples.jsonl` and the report script proves that durations, units and sort counts survive.
