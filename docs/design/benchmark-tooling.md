# Benchmark tooling

`bun run tools` runs working-tree and commit screens, algorithm comparisons, main/PR feedback and shared CI operations. The [protocol](../development/benchmarking.md) owns commands and measurement contracts; this page says which module owns each rule and where to change it.

## Ownership

| Source | Owns |
|---|---|
| `tools/` | Task-oriented CLI, exact-ref status, operation logs and workflow dispatch |
| `.github/scripts/` | Dependency-free CI bootstrap adapters and shared repository policy |
| `bench/catalog.toml`, `catalog.ts` | Identities, labels, families, record widths, build names, suite order, paired-block and revision-sampling budgets |
| `bench/plans/`, `plan.ts`, `config.ts` | Presets, strict schema, CLI overrides and syntax decoding |
| `bench/workspace.ts` | Source snapshots, builds, temporary worktrees and cleanup |
| `bench/local/` | Commit comparison, adjacent sample scheduling, prepared/measured results (`model.ts`), layout and counter studies, report |
| `bench/release.ts`, `bench/session.ts` | Preparation, retained builds and session recovery |
| `bench/rust/` | Timing, allocation measurement and resumable journals: `case.rs` (seed-one generator shared with the continuous suite), `plan.rs`, `record.rs` (record semantics and immutable originals), `variant.rs` (capabilities and adapters), `check.rs`, `run.rs` (clock boundaries, calibration, block order), `host.rs`, `comparison/mod.rs` (CLI, provenance, persistence) |
| `bench/rust/native/` | Foreign signatures, layouts, pinned archives, source verification and offline compilation. Compiler inputs stay relative to the adapter directory and vendor sources are reached through the workspace link, so isolated source snapshots reuse the compiled objects; source, layout, compiler-version and flag changes invalidate them |
| `bench/report/` | Persisted-result validation, statistics, figures and BMF |
| `bench/continuous/` | Paired-shard validation, JSON-only Cloud upload, PR history and GitHub publication |
| `bench/tracking/` | CI-only absolute Criterion algorithm selection, complete-case remote shards, report validation and shared JSONL/index/preview publication; ordinary Git owns publication |
| `bench/plot/` | Host-independent figure templates, visual encoding and theme |

Callers request a whole build/run/report operation, not a sequence they clean up. Report regeneration reads persisted results without rebuilding or contacting services. Process-level screens, in-process paired measurements and Bencher alerts stay distinct even when they share presentation. Workflows own event attribution, runner matrices, permissions and publication; prepare and measure run without secrets ([continuous pipeline](../development/continuous-benchmarks.md#pipeline)). `local/model.ts` owns deterministic case partitions, `continuous/paired.ts` the shard count, evidence validation and relative BMF, and `statistics.ts` the paired statistic. PR jobs measure baseline and current together, so there is no baseline queue or cross-machine calibration. [Main tracking](../development/continuous-benchmarks.md#main-long-term-tracking) reuses the smoke harness and builder but stores absolute Criterion estimates, without the paired statistic or verdict.

## Catalog and plan contract

The catalog is the language-neutral authority for identities and display names. TypeScript and Rust validate its schema version 1, and reports use its labels, not local dictionaries or string heuristics (historical IDs get readable fallbacks). Adapters, capability checks and input generation stay typed Rust code. New consumers of an ID, mode or record width use the catalog and its formatter, not positional parsing or local constants.

Plans decode once into one validated model ([plan grammar](../development/benchmarking.md#experiment-plans)). TypeScript owns validation, CLI overrides and the editor schema; Rust expands cases, including boundaries resolved from the measured crate, so the CLI never recreates the inventory. `local` and `continuous` share the smoke executable's typed cases and paired worker, `smoke` and `release` the algorithm harness. The CLI passes and retains the normalized plan, and the report checks every shard against that prepared plan and inventory. Criterion IDs are `rust/type/distribution/length/s1/mode` (`api` or `par-4`), comparison IDs `type/distribution/length/s1`; their formatters and decoders live in [`catalog.ts`](../../bench/catalog.ts) and [`plan.rs`](../../bench/rust/src/plan.rs) with a shared fixture, used by the runner, shard partitioner and publisher. [Plan tests](../../bench/plan.test.ts) protect selection counts and boundaries.

## Common changes

The [validation commands](../../AGENTS.md#commands) and [CI coverage](../development/ci.md) apply to the changed boundary.

- **Record type:** catalog metadata/inventories and [record semantics](../../bench/rust/src/comparison/record.rs); [adapters](../../bench/rust/src/comparison/variant.rs) pick direct-key versus by-key entries; a new scalar key also needs [generation](../../bench/rust/src/case.rs). The C-layout `case::Wide` is shared by both harnesses, but smoke `rec128_u64` keeps its zero-filled indexed payload while local `record128` retains its original patterned payload; tracking never substitutes one input for the other. Native support: unsafe `NativeRecord` binds Rust layout to type identity, C++ `NATIVE_RECORDS` owns layouts and `NATIVE_KERNELS` supported pairs; extend layout calls and support expectations deliberately. Type/length exclusions are [release coverage](../../bench/plans/release.toml) policy.
- **Distribution:** implement generation, catalog presentation and [plan](../../bench/plans/) coverage separately; a generator does not add a continuous case. Preview with `bun run --silent tools plan NAME` and exercise it through the maintained harness.
- **Algorithm:** catalog metadata names it and `Variant::with_kernel` owns adapter/capability decisions. Its typed visitor supplies one monomorphized sort to both local batch kernels and the tracking Criterion harness; callers own sampling, not another support table. Optional catalog `tracking_path` distinguishes public storage identities such as `rust-std-stable` without renaming local adapter IDs. The manual workflow choice list is checked against all production catalog entries. Native additions need [pinned acquisition/build](../../bench/rust/native/build.ts), wrappers, Rust enum/name mapping and C++ algorithm/pair entries (each pair generates declaration and binding together).
- **Mode or grammar:** catalog modes and the ID owner do not implement an API or grant a worker lane; Rust's typed dispatch, listing validation and the runner's exclusive four-thread phase are explicit decisions. For grammar changes compare complete serial-only and parallel listings for both builds and check shard partitioning and the shared fixture.
- **Figure:** follow the [figure guide](../development/benchmark-figures.md).
- **CI check:** follow [routing and cells](../development/ci.md). [`tools/tasks.ts`](../../tools/tasks.ts) owns checkout discovery and result locations, [`tools/process.ts`](../../tools/process.ts) execution/cancellation, [`marker.ts`](../../bench/continuous/marker.ts) report identity. Reuse existing jobs; credentials, artifact boundaries and publication permissions remain workflow responsibilities.

## Source comparison boundary

A prepared pair records its plan, provenance, inventory and frozen executable hashes in `prepared.json`, read by the local executor and the CI publisher through `local/model.ts`; `local.json` holds measurements only. `continuous measure` rejects measured inputs, incompatible harness/catalog sources, escaping paths and modified executables before the normal sample and report path, and shards consume complete cases without reconstructing build settings.

`bench/workspace.ts` implements the [working-tree snapshot](../development/benchmarking.md#working-tree-comparisons). Identical library hashes reuse one executable outside layout studies. Pre-parallel baselines get only the optional Rayon manifest entries, before hashing, so Cargo's inactive-feature validation passes without enabling Rayon. Resolved `library_features` come from Cargo artifacts separately from harness `features`, so a comparison cannot silently substitute a differently configured baseline. `compareLocal` probes `setarch -R` before building and applies it to both workers; protocol `paired-samples-v2` records fresh-process sample pairs.

Linked comparisons (`revisions.ts`) add named current/baseline copies to the algorithm-session pipeline without rewriting Rust source; wrappers do not prove distinct bodies. Layout sampling (`local/layout.ts`) uses the same workspace and runner, selects LLD and a section-shuffle seed through final rustc arguments, validates comparable sources and distinct hashes, and summarizes dispersion apart from paired verdicts; schema 2 wraps one complete run per seed, and continuous publication accepts only complete non-layout shards. CGU, alignment and engine-order controls live there too. `local/counters.ts` owns fixed-work Callgrind comparisons and PMU probes; counts regenerate offline and change no verdict or Bencher setting.

The builder holds a cache-scoped lease through compilation, provenance reads and freezing the workers into workspace-owned files, so concurrent experiments cannot replace each other's executables. For [resumed sessions](../development/benchmarking.md#resuming-algorithm-comparisons), TypeScript owns the session lease and Rust owns host/build compatibility, journal repair and worker kernel locks, so an orphaned worker keeps exclusivity after its parent's lease expires.

## Comparison pipeline

Adapters produce verified block samples, a separate process measures allocations and an offline TypeScript report computes every statistic and figure. Release sessions run on one maintainer machine: hosted limits must not decide groups or absolute-time comparisons.

- `Variant::with_kernel` gives a typed visitor its prepared sort or returns `None`; `Variant::prepare` uses it to create the local batch kernel. Capability listings, Criterion tracking and the manifest's case plan share this dispatch, with no second support table. A prepared input owns the original records and stable reference once per case, kernels share them immutably and own restorable batches, sorting is monomorphized per record type and dispatch is dynamic once per batch.
- The checker requires exact equality for scalar bits and stable outputs. Unstable records are checked against the original indexed by payload with a seen-index set, permitting reordered ties while catching lost, duplicated or damaged records.
- A group (one type and distribution across lengths) commits with its final observation. Resume keeps committed groups, drops an incomplete group and unfinished final line, and measures the rest; host fingerprint and manifest must match before any append, and report merging rejects overlapping samples.
- Memory runs first because Tessera's measured peak sets the `glidesort-sqrt` buffer. Its manifest is separate (same source and configuration, different binary hash) and timing checks it. A result has one build manifest and one executable pair.
- SVGs and Bencher Metric Format come from the same summaries, so ratios, bounds and A/A noise have one definition; raw samples suffice to recompute them.
- Native: fetch verifies pinned archives and records tree hashes, the build verifies them and downloads nothing, trees and outputs are ignored, and repository shims include upstream sources rather than copy them. C and C++ see complete native buffers with C layouts and compare there. Clausecker's 32-bit pair has a supplementary task; SIMD key-value interfaces with other layouts stay unsupported, and float interfaces participate only if they preserve `total_cmp` order and all payload bits. The memory executable wraps C allocation symbols and defines C++ `new`/`delete`, closing the shared-library boundary without instrumenting the timed executable.

## Figure boundary

Templates accept chart meaning (names, values, bounds, groups, columns, legends), not CSS or coordinate callbacks. Reuse a template for new cases and add a complete template for a new visual structure; promote shared operations only when consumers need the same concept.

```text
catalog → typed presentation → report values
Observable Plot + font metrics + theme → templates → report writers / Vue components
```

Renderers return deterministic SVG strings with no filesystem, network or global state, each in a private lightweight document. Observable Plot owns scales, ticks, axes, clipping and marks, linkedom documents and serialization, string-pixel-width Inter metrics. Bencher semantics stay in `continuous/`; templates receive formatted values and verdicts.

## Dependencies and verification

Bun, pinned in `package.json`, owns dependency installation (`bun.lock`), direct TypeScript execution, tests and package scripts, with no Node fallback or emitted-JavaScript tree. Direct execution avoids a build step and stale output, and controlled CLI comparisons found it faster to start than Node running precompiled TypeScript 7.1 output; [local AOT](../development/benchmarking.md#optional-local-aot-cli) is an optional snapshot. The native TypeScript compiler checks tooling; [site checks](../development/docs-site.md#commands) own Vue SFC validation. Commander parses the CLI, Zod owns the strict plan schema and smol-toml decodes TOML; archives are hash-verified before extraction with the tar library and again during offline compilation. Tooling uses no Python. Do not maintain parallel parsers, registries, drawing engines or statistical definitions.

Tests cover result compatibility, statistics, report generation, external trust and recovery boundaries, input restoration after destructive sorts, stable and unstable payload checks, float edge cases, journal recovery and allocation accounting, plus the paired timing, 8-of-9 decision, A/A noise and standard-sort anchor. They do not pin SVG geometry or library attribute order. Visual acceptance renders old and new figures from the same recorded results in light and dark; synthetic fixtures cover edge cases, not performance. Raw results, vendor trees and generated reports stay out of Git.
