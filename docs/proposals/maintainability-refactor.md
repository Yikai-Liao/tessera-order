<!-- SPDX-License-Identifier: BSD-2-Clause -->

# Proposal: Maintainability refactor

Status: in progress. Date: 2026-11-09. Findings verified against `main` at `8bd64d9`, which includes the Rayon entry points.

This round makes the repository cheaper to read and change. It does not change algorithm logic or performance. Each item below names a rule that several places currently restate, or a boundary that one change has to cross more often than necessary, and moves it to one owner. Success means fewer duplicated rules, constraints checked by types or tests instead of by convention, and smaller change surfaces for common tasks. More files, layers or abstractions are not a success measure, and neither are shorter files.

## Frozen in this round

| Area | Rule |
|---|---|
| `rust/tessera-sort/src/` | No source changes. Moving, splitting or re-signaturing engine functions can change inlining and code placement, and the [engine debt](#engine-debt-for-a-measured-round) below needs measurements this round does not take |
| Timed regions | Code timed by `prepare_kernel`, `comparison/variant/parallel.rs` and Criterion `measure` calls the same library functions with the same record types and key extractors, with no added indirection. Scalars keep `stable_sort`/`par_stable_sort` and records keep the by-key entry points, because these select different extractor and caching paths |
| Inputs and identities | `case.rs` generation, record payload construction, case and benchmark IDs, testbeds, build features and Bencher thresholds stay unchanged |
| Scheduling | Two serial lanes, exclusive cases from 1,000,000 records, and the sequential exclusive `par-4` phase stay unchanged |
| Contracts | [Semantics](../../conformance/semantics.md) stays unchanged: O(√n) auxiliary space for the serial API, O(P√n) for the Rayon API, ownership and panic recovery |
| Infrastructure | No new required dependency, crate, CI job, Bencher cost or second command system. `pnpm tools` stays the only entry point. Trusted and untrusted workflow boundaries stay unchanged, including the Bencher image, which contains only `bench/continuous/worker.ts` and `tools/process.ts` from the default branch |

## Findings

Severity reflects how often the defect affects a real task and what a mistake costs. The IDs are referenced in the [sequence](#sequence).

| ID | Severity | Evidence | Problem | This round |
|---|---|---|---|---|
| F1 | High | `bench/rust/src/comparison/native.rs:28`, `bench/rust/native/registry.cpp:25-48`, `variant.rs:389-417` | `native::prepare<T>` takes a `ty: u32` that is independent of `T`. The macro call sites pass the literals 0–5. `bench_layout` uses a second index space (0–2 for `Row<u32>`, `Row<u64>` and `WideRecord`) and has no bounds check. The algorithm order is a Rust string list that has to match the C++ table rows. A wrong index makes a native kernel read `len` records of another size, which is undefined behavior. No mismatch exists today | Yes |
| F2 | Medium | `variant.rs:15-45`, `:396-417` | `BenchmarkRecord` mixes record semantics (`order`, `same`, `index`, `extra_bytes`) with Tessera serial and parallel entry points and glidesort adapters. Capability exceptions are spread over the `voracious` guard, the owned-record `radsort` arm, the parallel `supports` shortcut and native `None` returns | Yes |
| F3 | Medium | `smoke.rs:60,92,134`, `bench/rust/src/plan.rs:133`, `bench/report/model.ts:75,129`, `bench/continuous/worker.ts:18,22,58`, `runner.ts:12`, `bench/catalog.ts:109` | There are two ID grammars: Criterion `rust/<type>/<distribution>/<len>/<seed>/<mode>` and comparison `<type>/<distribution>/<len>/s1`. Each is formatted in several places and parsed by position, so the length is at index 3 in one grammar and index 2 in the other. Several modules match the `par-4` mode by suffix. A new mode or seed touches every one of them | Yes |
| F4 | Low | `tools/{index,continuous,repository,experiments}.ts` → `bench/workspace.ts:10`; `tools/status.ts:4` → `bench/continuous/github.ts:164` | `tools` is the CLI over `bench` and `bench` uses `tools/process.ts`, so imports in both directions are expected. Two edges are misplaced: every command imports the workspace builder for a three-line `repositoryRoot`, and the status command imports the GitHub publisher for a comment marker format. Neither import has a runtime side effect | Yes |
| F5 | Low | `bench/report/fixture.ts:27,108`, `bench/catalog.toml:37` | The report fixture hard-codes record widths and large-record type names, although the catalog already stores `bytes` | Yes |
| F6 | Low | `docs/design/comparison-benchmarks.md`, `docs/design/benchmark-tooling.md:36`, `docs/proposals/rayon-parallel-sort.md` | The comparison design still lists a `suite.rs` module that no longer exists; case identity now lives in `plan.rs`. `benchmark-tooling.md` still says Criterion IDs end in `/api` and there is no mode selector, which the `par-4` mode contradicts. The Rayon proposal says "implemented" and overlaps the [engine reference](../design/engine.md#parallel-root-completion) and semantics, which is the parallel decision record that [contributing](../../CONTRIBUTING.md#documentation) rules out | Yes |
| F7 | Low | `registry.cpp:7-20` | clang-format wraps the `DECLARE` macros across unrelated lines, which hides which kernels exist | With F1 |
| F8 | Low | `.github/workflows/bench-track.yml` | Two shell blocks build arguments and package images. They work, but they cannot be reproduced locally | Optional |

Two items from the earlier review draft were checked and dropped:

- **Sharing the 128-byte record.** `smoke.rs` `Wide` and comparison `WideRecord` are different representations. Only `WideRecord` is `repr(C)` for the native kernels, `WideRecord` fills its payload with a pattern while `Wide` stores only the index, and the IDs (`rec128_u64`, `record128`) differ. Unifying construction would change continuous inputs.
- **Splitting continuous publication into decision and side-effect layers.** `publishReport` already reads measurements through a pure `readMeasurement` and isolates history and image failures. `bench/continuous/index.test.ts` covers missing and mismatched baselines, unavailable history, image failure, cancellation, per-build isolation and malformed reports. If the code is touched, the check conclusion at `index.ts:166-171` can become a named function. No dedicated PR is planned.

`bench/report/charts.ts` and `bench/continuous/markdown.ts` are long but have no confirmed duplicate rule. They change only if F3 or F5 exposes one.

## Design of the changes

### F1: native record identity

```rust
// Shape only. Discriminants are the C++ table's column and row order.
#[repr(u32)]
enum NativeType { U32, U64, F64, Row32, Row64, Record128 }

/// # Safety
/// `Self` has the C layout that the native registry uses for `TYPE`.
unsafe trait NativeRecord: BenchmarkRecord {
    const TYPE: NativeType;
}

fn prepare<T: NativeRecord>(algorithm: NativeAlgorithm, stable: bool, input: Rc<PreparedRecords<T>>)
    -> Option<Box<dyn Kernel>>;
```

- `NativeAlgorithm` is an enum whose discriminants document the C++ row order. Catalog names are converted to it once.
- One C++ type list generates both the kernel columns and the layout rows, so kernel lookup and layout lookup share the `NativeType` index. `bench_layout` covers all six types and rejects out-of-range indices.
- The C++ registry also exports each row's algorithm name, and a test compares it with the `NativeAlgorithm` name for every row. Layout and output checks cannot catch a swapped algorithm row, because both algorithms sort correctly and the measurement would be filed under the wrong name. The name check runs in tests, not per sort.
- Tests assert `size_of`, alignment and payload offset for every `T::TYPE`, and run every registered native kernel on small checked inputs, generalizing `duckdb_sorts_boundaries`. A null-pattern test alone is not enough: `u32`/`u64` and `row64`/`record128` have identical patterns, and several rows support all six types, so swaps must be caught by layout and output checks. These tests check the registry. It does not add a runtime support table, because `Variant::prepare` stays the only capability check, as the [comparison design](../design/comparison-benchmarks.md) requires.
- The timed closure (`sort(values.as_mut_ptr().cast(), values.len())`) is unchanged. Lookup happens before timing.

### F2: records versus adapters

- `BenchmarkRecord` keeps record semantics. The glidesort functions have only default bodies, so they become free generic functions.
- Entry points that really differ per type, such as Tessera direct versus by-key, stay explicit per type, so the timed call is the same function as today.
- Capability exceptions move into `Variant::prepare` next to each other. The parallel `supports` shortcut keeps its reason: preparing a parallel kernel would warm and discard a real pool.
- Keep `CaseInput` as the single enum dispatch and `Box<dyn Kernel>` as the only type erasure. Do not add a plugin registry or a new crate.

### F3: one owner per ID grammar

- TypeScript: one module formats and parses both grammars and the mode set from the catalog. Scheduling uses strict parsing, which rejects unknown IDs. History display keeps the lenient fallback that `caseParts` has today.
- Rust: one formatter for Criterion IDs, used by `smoke.rs`. `InputCase::id` keeps the comparison grammar.
- `bench/continuous/worker.ts` stays built-ins-only, because the trusted image copies it without dependencies or the catalog, and PR checks run the default branch's copy. `partition` keeps its local length parse, and unit tests check it against the owning module. Its lane assignment is shown by unit tests, not by PR checks.
- Cross-language agreement reuses the existing shared fixture pattern (`bench/plans/selection.fixture.toml` is read by both `bench/plan.test.ts` and `plan.rs`) instead of adding a new snapshot system.

### F4: dependency direction

`repositoryRoot` moves to `tools/tasks.ts`, which already derives result locations from the repository root for CLI tasks. Only `tools/` calls it, and `tools/process.ts` stays limited to running processes, logging and cancellation. The comment marker moves to a small module that both `status.ts` and `github.ts` import. Other code stays in place. Do not create `core/`, `shared/` or `utils/` layers.

## Sequence

Each PR changes one boundary, can be reverted on its own and passes an independent Agentic Review.

| PR | Scope | Verification class | Done when |
|---|---|---|---|
| 1 | This proposal, the refactoring, review, duplication and identity rules, and links | Docs | Merged |
| 2 | F4 | TypeScript | `tools` no longer imports `bench/workspace.ts` or `bench/continuous/github.ts`. CLI names, options, output and exit codes are unchanged |
| 3 | F1, F7 | Comparison harness | A record type and a native index can no longer be combined freely. Layout, kernel output and algorithm name tests pass. Native comparison output is unchanged |
| 4 | F2 | Comparison harness | Each design decision (record semantics, capability exceptions, entry point per type, FFI layout) has one authoritative location, and necessary cross-language adapters are checked by types or tests. Adding a record type or algorithm does not restate one rule in several places; it may still touch input generation, record semantics, dispatch and FFI layout. Comparison output and checks are unchanged |
| 5 | F3 | Continuous binary | For both builds, `TESSERA_BENCH_LIST=1` output is identical to the output before the change in each setting: without `TESSERA_BENCH_PARALLEL` it lists only the serial IDs, and with `TESSERA_BENCH_PARALLEL=4` it lists the same serial IDs plus the same `par-4` IDs, in the same order. `validateApiListings` passes unchanged. Unit tests show that `partition` assigns the continuous plan to the same lanes. Outside the owning modules, only `worker.ts` parses an ID, and it is tested against them |
| 6 | F5 | TypeScript | The report fixture reads widths from the catalog. Report tests, including legacy figure content, pass unchanged |
| 7 | F6 | Docs | Stale module names are removed. The Rayon proposal's API, memory and scheduling constraints that no other page states move into semantics, the engine reference or the roadmap; then the proposal is deleted and its links are updated |
| 8 | F8, only with a shown maintenance cost | CI | The same jobs, permissions, triggers and queue behavior remain |

## Verification

| Class | Touches | Evidence |
|---|---|---|
| Docs, TypeScript | No Rust source | `pnpm lint`, `pnpm tools:test`, `pnpm tools:typecheck`, and the site checks for docs. Benchmark checks run only when the diff matches the `.github/workflows/bench-build.yml` path filter. The filter includes TypeScript that builds or schedules the measured executable, such as `bench/continuous/runner.ts`, which sets build features and compiler flags, and `tools/tasks.ts`. When no check runs, the PR says so instead of citing one |
| Comparison harness | `bench/rust/src/comparison/`, `bench/rust/native/` | These modules are compiled only with the `compare` feature (`bench/rust/src/lib.rs`), so continuous executables do not link them. Workspace tests, doctests and comparison tests, with the diff showing that timed regions call the same library functions with the same record types and key extractors, with no added indirection |
| Continuous binary | `smoke.rs`, `plan.rs`, `case.rs` | `TESSERA_BENCH_LIST=1` output identical to the output before the change, per build and per `TESSERA_BENCH_PARALLEL` setting. `case.rs` generation and its identity test are untouched |

When benchmark checks run, read both before merging. Investigate any alert under the [alert procedure](../development/continuous-benchmarks.md#alerts) and classify it from that evidence, not from which languages the diff touches.

Use the [commands](../../AGENTS.md#commands) that apply to the touched area. No PR in this round adds measurements beyond the existing PR benchmark checks.

Every PR description states which boundary changed, how its [task walkthroughs](#task-walkthroughs) changed, what was deleted or merged, the net production line change, which invalid combination the types or tests now reject, and which checks ran or were left to CI. Reject a refactor PR that also changes behavior, adds an abstraction without removing a duplicate or encoding a constraint, or claims improvement only from shorter files.

## Engine debt for a measured round

These findings concern the parallel engine merged in `8bd64d9`. Each changes code placement or a hot path, so it needs the [measurement procedure](../development/benchmarking.md#run-an-experiment) and [source-attribution controls](../development/benchmarking.md#additional-source-attribution-controls). None is part of this round.

| ID | Evidence | Debt |
|---|---|---|
| E1 | `engine/parallel.rs:125-150`, `key.rs:438-461` | `Narrow` (owned) duplicates `KeyHalf` (borrowed), including the window shift, but lacks `KeyHalf`'s invariant comment |
| E2 | `engine/parallel.rs:15-51,73-122`, `engine.rs:396-476` | The parallel entry repeats tile geometry, the probe sequence and the 128-bit half dispatch. The separate `prepare` instantiation is deliberate, to preserve serial inlining contexts, so merging needs codegen evidence |
| E3 | `key.rs:315-371` versus `:373-405` | `Direct::try_count_window_parallel` reimplements the serial counting and fill |
| E4 | `key.rs:241`, `engine/parallel.rs:54-70,125-150` | Each new window method must be forwarded through both `KeyHalf` and `Narrow`, and `RootExtractor` needs a decision for each method, because it deliberately forwards only some of them |
| E5 | `block.rs:401,880` | Serial block storage carries the `rayon`-only `scratch_segments` field and a branch in `compact_with`. Both continuous builds enable `rayon` (`bench/catalog.toml:78-79`), so serial tracking measures this layout rather than the default feature set. Changing that is a protocol decision for the maintainer |
| E6 | `engine.rs` | About 1,240 production lines combine routing, sampling, digit planning, node and tile sorting, and scratch. Split only along mechanism boundaries that keep invariants local, and only with layout controls |

## Task walkthroughs

Before PR 2, walk through each common task on `main` without implementing it: add a record type, a distribution, an algorithm, a benchmark mode, a figure and a CI check. Work only from what the repository states, not from session context. For each task, record here the files and authorities a maintainer must read, the rules they must keep consistent by hand, and how a mistake would be caught. Separate distinct responsibilities, such as catalog metadata, case generation and plan coverage for a distribution, from restated rules.

Each refactor PR replays the tasks its boundary affects, updates their entries and states the difference in its description. A refactor that leaves its tasks' manual synchronization and required reading unchanged has not shown a benefit.

### Baseline at `826e6ec`

These are repository-only walkthroughs, not implemented additions. The CI-check entry is replayed for F4 and native compiler coverage; record/algorithm entries are replayed for F1/F2/F7; record/figure entries for F5. The [tooling ownership map](../design/benchmark-tooling.md#ownership) and [validation commands](../../AGENTS.md#commands) are the common starting points. The files below identify separate responsibilities as well as repeated decisions; touching several files is not by itself a defect.

#### Add a record type

- Start with [catalog type metadata and suite inventories](../../bench/catalog.toml), then [record semantics and shared input construction](../../bench/rust/src/comparison/record.rs). [Adapter entry points and dispatch](../../bench/rust/src/comparison/variant.rs) are a separate decision: scalar Tessera adapters use the direct-key API, while keyed records keep their explicit by-key entry points. A new scalar key also needs the [key and generator contract](../../bench/rust/src/case.rs). Comparison and continuous coverage are separate choices: [`smoke.rs`](../../bench/rust/benches/smoke.rs) has its own typed dispatch and wide-record payload; do not merge that representation with comparison `WideRecord`.
- If native support is intended, read [Rust preparation and layout checks](../../bench/rust/src/comparison/native.rs), [C layouts](../../bench/rust/native/layout.h), [registry bindings](../../bench/rust/native/registry.cpp) and the selected wrappers. Implementing unsafe `NativeRecord` binds the Rust representation to `NativeType`; callers no longer pass a numeric type argument. One C++ `NATIVE_RECORDS` list determines both kernel columns and layout rows, and `NATIVE_KERNELS` lists supported pairs. Rust/C layout and order still need cross-language agreement. The semantic trait no longer requires Tessera or glidesort methods; native representation safety does not depend on those adapters.
- A report-fixture extension also reaches [`fixture.ts`](../../bench/report/fixture.ts), which reads each record's `bytes` from the catalog and rejects missing metadata. Its separate type/length exclusion is fixture coverage policy, to be kept consistent with the intended [release coverage](../../bench/plans/release.toml); widths alone do not determine that policy.
- Layout checks cover all six existing native representations. Row-name checks cover every catalog-native algorithm, and native output tests check the independent support expectations and supported kernels on boundary inputs. Adding a type still requires extending Rust's layout-check calls and the intended support expectations; the trait does not discover those tests automatically. Plan count/selection tests and preview catch separate inventory mistakes.

#### Add a distribution

- Read [`case.rs::generate`](../../bench/rust/src/case.rs), [catalog distribution labels and suite inventories](../../bench/catalog.toml), and the intended [plans](../../bench/plans/). Generation, presentation and regression coverage are distinct decisions: adding a generator does not automatically add a continuous case.
- TypeScript [plan validation](../../bench/plan.ts) and [Rust expansion](../../bench/rust/src/plan.rs) consume the catalog and plans. For the [shared selection fixture](../../bench/plans/selection.fixture.toml), TypeScript validates the plan and Rust checks the listed selection expectations; it does not compare both implementations' selection results. No third distribution parser is needed.
- Manually choose which smoke/comparison inventories and plan inclusions exercise the distribution. Existing generator-identity assertions protect retained inputs, not an unselected new branch. Preview the selected inputs with `pnpm -s tools plan NAME` and validate their generated output through the maintained harness.

#### Add an algorithm

- Read [catalog algorithm metadata](../../bench/catalog.toml), [variant listing, feature selection and preparation](../../bench/rust/src/comparison/variant.rs), and [the comparison contract](../design/comparison-benchmarks.md). Metadata supplies identity, stability and adapter category; executable preparation supplies support, and the manifest derives its capability list from preparation.
- A new native algorithm additionally needs its wrappers and [pinned acquisition/offline build](../../bench/rust/native/build.ts), plus [Rust enum/name mapping](../../bench/rust/src/comparison/native.rs) and the [C++ registry lists](../../bench/rust/native/registry.cpp). Add one `NATIVE_ALGORITHMS` entry and its `NATIVE_KERNELS` pairs: each pair generates its C declaration and typed binding together. Duplicate or unknown pairs fail compilation. Row names derive from algorithm tokens, and tests check their agreement with Rust; wrappers still own correct buffer interpretation. Dispatch uses the catalog's native adapter category, so adding a native algorithm no longer extends a second name list in `variant.rs`.
- The `voracious` integer-only restriction and owned-record `radsort` path are adjacent in `Variant::prepare`. The parallel `supports` shortcut remains there with its pool-warmup reason; C++ null bindings remain authoritative for supported native pairs. Shared glidesort adapters are free generic functions, so adding an algorithm does not add methods to the record-semantic trait. Listing/feature tests, capability-versus-preparation tests, linkage and output verification detect different mistakes. Native output tests now assert the retained support matrix independently, so dropping a supported pair fails instead of silently reducing coverage. A new algorithm must extend those expectations deliberately.

#### Add a benchmark mode

- Read [catalog modes](../../bench/catalog.toml), [Criterion dispatch and ID construction](../../bench/rust/benches/smoke.rs), [continuous build/list validation](../../bench/continuous/runner.ts) and the [trusted worker](../../bench/continuous/worker.ts). A label does not implement a new API or grant it a worker lane.
- ID interpretation also reaches [catalog display helpers](../../bench/catalog.ts), [local report labels](../../bench/local/report.ts) and [comparison report IDs/length parsing](../../bench/report/model.ts). Criterion and comparison IDs are different grammars. Positional parsing and `api`/`par-4` suffix checks currently require coordinated edits (F3).
- The worker's dependency-free copy is an intentional trust boundary: [the tracker](../../.github/workflows/bench-track.yml) packages only default-branch worker/process code. Keep its scheduling rule checked against the ID owner rather than importing the catalog into the image.
- Runner tests protect additive listings; worker tests protect lane assignment. Build verification checks serial-only and parallel-enabled listings for both configurations. A mode change needs those checks together; an unchanged serial listing alone does not establish the new schedule.

#### Add a figure

- Start with the [figure guide](../development/benchmark-figures.md) and [figure boundary](../design/benchmark-tooling.md#figure-boundary). Reuse [row](../../bench/plot/rows.ts), [matrix](../../bench/plot/matrix.ts) or [trend](../../bench/plot/trends.ts) templates where the question fits; chart-value assembly stays in [offline reports](../../bench/report/charts.ts) or [continuous reports](../../bench/continuous/).
- Shared [theme](../../bench/plot/theme.ts) and [SVG envelope](../../bench/plot/figure.ts) own appearance/accessibility; catalog helpers own labels. The caller chooses values and output location. A site figure uses [`BenchmarkFigure.vue`](../.vitepress/theme/components/BenchmarkFigure.vue); these are separate responsibilities, not duplicated rendering rules.
- Content/geometry tests, retained SVG fixtures and site checks catch regression classes; light/dark rendering at actual width checks readability. The fixture reads synthetic record widths from the catalog, so a new record does not require a second byte-width constant. The retained large-record coverage exclusion is a separate policy decision. No new renderer, statistics definition or measurement is needed just to add a dataset.

#### Add a CI check

- Read [CI policy](../development/ci.md), [the aggregate workflow](../../.github/workflows/ci.yml) and [path routing with its tests](../../.github/scripts/ci_changes.ts). Add an existing language-cell command in its [composite action](../../.github/actions/rust-test/action.yml); the stable cells also compile the standalone native registry with GCC/Clang on Linux and both MSVC preprocessors on Windows. A genuinely new job needs explicit dependencies and aggregation in `ci-ok`. Routing, platform coverage and aggregate failure behavior are different obligations.
- Reuse [CLI task support](../../tools/tasks.ts), including checkout discovery, and [process/logging/cancellation](../../tools/process.ts) for shared operations. A check that needs the checkout root no longer needs to import the workspace builder; snapshot/build behavior remains owned by that builder.
- A benchmark check additionally reaches the separate [build path filter](../../.github/workflows/bench-build.yml) and [trusted tracker](../../.github/workflows/bench-track.yml). The [marker module](../../bench/continuous/marker.ts) owns report identity for both status and publication, so understanding the marker does not require reading the publisher's write policy. Credentials, image contents and event ownership are unchanged.
- Router tests catch routing mistakes; command tests exercise behavior; CI proves the actual platforms and `ci-ok` dependencies. Local success does not prove an unselected job ran. During this round use existing cells/jobs rather than expanding the CI budget.

## Stop condition

Stop when the [task walkthroughs](#task-walkthroughs) touch a predictable, small set of files and no rule is restated in several places. Stop earlier if further restructuring adds interfaces instead of removing them.
