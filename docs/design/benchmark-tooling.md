# Benchmark tooling

`pnpm tools` exposes dirty-tree/commit screening, algorithm comparisons, exact main/PR feedback and shared CI operations. Commands and measurement contracts are in the [protocol](../development/benchmarking.md); this page identifies where to change behavior.

## Ownership

| Source | Owns |
|---|---|
| `tools/` | Task-oriented CLI, exact-ref status, operation logs and workflow dispatch |
| `.github/scripts/` | Dependency-free CI bootstrap adapters and shared repository policy |
| `bench/catalog.toml`, `catalog.ts` | Identities, labels, families, record widths, build names and suite order |
| `bench/plans/`, `plan.ts`, `config.ts` | Presets, strict schema, CLI overrides and syntax decoding |
| `bench/workspace.ts` | Source snapshots, builds, temporary worktrees and cleanup |
| `bench/local/` | Commit comparison, adjacent sample scheduling, legacy result reading and report |
| `bench/release.ts`, `bench/session.ts` | Preparation, retained builds, validation receipts and session recovery |
| `bench/rust/` | Verified timing, allocation measurement and resumable journals |
| `bench/report/` | Persisted-result validation, statistics, figures and BMF |
| `bench/continuous/` | Paired-shard validation, JSON-only Cloud upload, legacy Bencher history and GitHub publication |
| `bench/plot/` | Host-independent figure templates, visual encoding and theme |
| `bench/rust/native/` | Pinned archive acquisition, source verification and offline compilation |

Callers request a whole build/run/report operation, not a sequence they must clean up themselves. Report regeneration reads persisted results without rebuilding or contacting measurement services. Process-level screens, in-process paired measurements and Bencher alerts remain distinct even when they share presentation.

Status resolves the requested remote ref once and associates PR test-merge workflows separately from head-attached benchmark checks. Only published check identities associate the trusted tracker; missing feedback is not success. The [CLI reference](../development/tooling.md) defines compact output and diagnostics.

`local/model.ts` owns deterministic complete-case partitions; `continuous/paired.ts` owns the hosted shard count, complete-evidence validation and relative BMF adaptation. `statistics.ts` remains the paired-statistic owner. The catalog owns legacy absolute Testbeds and new `relativeTestbed` identities. Hosted jobs measure baseline and current together, so there is no baseline-readiness queue or cross-machine absolute-time calibration.

Continuous jobs reuse `compare commit` for snapshots, two-revision builds, adjacent A/B measurement and retained reports. Workflows own event attribution, runner matrices and permissions. Unprivileged jobs run PR code without secrets. The privileged tracker uses trusted default-branch code and reads JSON only, recomputes summaries and publishes existing figures/checks before separately archiving BMF. Artifact provenance is self-reported, not proof against fabricated PR measurements. Trusted-base DCO uses dependency-free adapters without installing PR code.

## One catalog and plan contract

The catalog is the language-neutral authority for identities and display names. TypeScript and Rust validate schema version 1; reports use its labels rather than local dictionaries or string heuristics. Historical IDs have readable fallbacks. Executable adapters, capability checks and input generation remain typed Rust code; the catalog does not store functions. Deployment matrices and artifact/security boundaries remain explicit in workflows. New consumers of an ID, mode or record width use the catalog and the owning formatter, not positional parsing or local constants. Rust and native registries that index the same records share one type list, and layout and output tests cover every entry.

Repository-authored selections use TOML; custom plans accept JSON too. Decode syntax once, then use one validated model: products within inclusion rules, union across rules, exclusions and deduplication. TypeScript owns input validation, CLI overrides and editor JSON Schema; Rust expands cases, including boundaries resolved from the measured crate. Do not recreate the case inventory in the CLI.

`local` and `continuous` share the smoke executable’s typed cases and paired worker; direct Criterion invocation remains available; `smoke` and `release` use the paired algorithm harness. This is a capability boundary, not a second selection language. The executable embeds continuous defaults; the shared CLI passes and retains the normalized plan explicitly. The trusted publisher validates that plan against its own continuous preset and builds/lists its trusted Rust harness to obtain the full expected inventory. It neither trusts shard-declared completeness nor executes PR configuration. [Plan tests](../../bench/plan.test.ts) and Rust tests protect selection counts and boundary behavior.

Selection axes are type, distribution and length; separate `parallel` rules add four-thread inputs without changing serial coverage. Criterion IDs use `rust/type/distribution/length/s1/mode` (`api` or `par-4`); comparison input IDs use `type/distribution/length/s1`. TypeScript's formatters, strict decoders and permissive historical labels live in [`catalog.ts`](../../bench/catalog.ts); Rust's formatters live in [`plan.rs`](../../bench/rust/src/plan.rs). Explicit IDs in the shared fixture check both languages. The local runner, shard partitioner and publisher use that decoder; other consumers do not parse positions or mode suffixes. The [continuous plan](../../bench/plans/continuous.toml) owns regression coverage; comparison presets retain 125 smoke inputs and 381 release inputs.

## Common changes

Use these paths to separate decisions that need coordination from rules already owned by a module. The [validation commands](../../AGENTS.md#commands) and [CI coverage](../development/ci.md) apply to the changed boundary.

- **Record type:** start with catalog metadata/inventories and [record semantics/input construction](../../bench/rust/src/comparison/record.rs). [Adapters](../../bench/rust/src/comparison/variant.rs) separately select direct-key versus by-key entry points; a new scalar key also needs [generation](../../bench/rust/src/case.rs). Continuous `Wide` is not comparison `WideRecord`: keep their distinct payloads and IDs. For native support, unsafe `NativeRecord` binds Rust layout to its type identity; C++ `NATIVE_RECORDS` owns columns/layouts and `NATIVE_KERNELS` owns supported pairs. Extend layout calls and independent support expectations deliberately; the trait does not discover tests. Report fixtures read catalog widths, while type/length exclusions remain separate [release coverage](../../bench/plans/release.toml) policy.
- **Distribution:** implement generation, catalog presentation and chosen [plan](../../bench/plans/) coverage separately; a label or generator does not add a continuous case. TypeScript validates selections and Rust expands them. The shared fixture checks explicit ID formatting in both languages; its selection examples are checked by Rust, not an assertion of identical TS/Rust expansion. Preview with `pnpm -s tools plan NAME` and exercise generated output through the maintained harness.
- **Algorithm:** catalog metadata names the algorithm; `Variant::prepare` owns capability decisions, including voracious restrictions and owned-record radsort. Parallel support retains its no-pool-warmup shortcut. Native additions also need [pinned acquisition/build](../../bench/rust/native/build.ts), wrappers, Rust enum/name mapping and C++ algorithm/supported-pair entries. Each pair generates declaration and binding together. Layout, row-name and output/support tests cover different mistakes; add new expectations explicitly. Standalone registry compilation covers GCC, Clang and both MSVC preprocessors.
- **Mode or grammar:** catalog modes and the ID owner above do not implement an API or grant a worker lane. Rust’s typed dispatch, same-revision listing validation and the local runner’s exclusive four-thread phase remain explicit decisions. Local execution delegates group extraction to the owner; report labels retain historical fallbacks. For grammar-only changes, compare complete ordered serial-only and parallel-enabled listings for both builds, and check shard partitioning and the shared fixture.
- **Figure:** follow the [figure guide](../development/benchmark-figures.md). Callers assemble values; row/matrix/trend templates own rendering, and theme/envelope own appearance and accessibility. Fixtures cover synthetic edge cases, not measurements. Check numeric content through report integration tests and inspect before/after figures in light and dark appearance without creating another renderer or statistics definition.
- **CI check:** follow [routing, cells and aggregation](../development/ci.md); benchmark measurement filters and trusted tracking are separate from language routing. [CLI task support](../../tools/tasks.ts) owns checkout discovery and result locations, [process support](../../tools/process.ts) owns execution/cancellation, and [marker](../../bench/continuous/marker.ts) owns exact report identity without publisher policy. Reuse existing jobs; platform execution, credentials, artifact boundaries and publication serialization remain workflow responsibilities.

## Source comparison boundary

The workspace materializes current staged, unstaged and untracked sources without changing the caller's checkout. Internal links become regular snapshot contents; dangling/escaping links and directory cycles fail before building. Baseline links resolve within that commit's archive, never against the live checkout. Source hashing, backup and compilation see the same materialized inputs.

Both revisions use the current harness, lockfile, workspace and toolchain; only the baseline library is replaced. `buildRevisions` fixes Linux x86-64 executable base addresses with a retained `-no-pie` linker argument. For identical library source hashes outside layout studies it reuses the first executable rather than rebuilding identical inputs. Pre-parallel serial baselines receive only the current optional Rayon manifest entries when absent, before source hashing. This keeps Cargo's inactive-feature validation and optional dependency edges compatible with the shared harness and lockfile; it does not enable Rayon or replace the baseline's defaults, selected APIs or backends. The retained baseline hash describes these compiled inputs. Incompatible historical APIs or selected timed IDs fail instead of silently substituting a baseline. Cargo JSON selects executables and matching build-script provenance, including configured targets and compiler overrides; incompatible settings fail before timing.

Optional local layout sampling uses the same workspace and adjacent-pair runner. Final benchmark rustc arguments select LLD and a fixed function-section shuffle seed without changing dependency compilation flags. `local/layout.ts` validates comparable sources and distinct executable hashes and summarizes observed cross-layout dispersion separately from paired sample verdicts. Link maps retain address evidence. Each seed keeps a complete local run; schema-1 reports remain readable, while schema 2 wraps the selected runs and linker provenance. Continuous publication accepts only complete non-layout revision shards; layout screens retain their separate descriptive contract.

Optional local CGU, explicit alignment and engine-order controls reuse the revision builder and retain effective flags, maps and ordering evidence. `local/layout.ts` owns control identities and engine-symbol selection; CLI choices and the runner consume them.


`local/counters.ts` owns fixed-work Callgrind comparisons and per-event PMU probes, reusing revision snapshots, builds and case selection. The opt-in Rust marker delimits a verified, prepared serial batch. Counts and simulator events retain provenance and regenerate offline; they do not change timing verdicts or Bencher settings.

`continuous/volatility.ts` reuses public history loading and report normalization to describe adjacent completed main samples. The catalog owns configuration/Testbed identities; histories retain raw responses for offline regeneration and keep same-commit pairs separate from mixed-source observations.

Temporary sources and default builds belong to the workspace; an explicit `TESSERA_BENCH_TARGET_DIR` belongs to the caller and survives cleanup; result directories retain provenance, observations, summaries and figures. Interrupted local runs are inspectable, not resumable or reportable as complete. Algorithm sessions retain their executable pairs, selection and settings before measuring; `compare resume` uses these exact artifacts without rebuilding. TypeScript owns the session lease and validation receipts. Rust owns host/build compatibility, journal repair and worker kernel locks, so an orphaned worker cannot lose exclusivity merely because its parent lease expires. [The recovery procedure](../development/benchmarking.md#resuming-algorithm-comparisons) defines partial-state handling.

## Figure boundary

Templates accept chart meaning: names, values, bounds, groups, columns and legends, not arbitrary CSS or coordinate callbacks. Reuse a template for new cases; add a complete template for a new visual structure. Promote shared operations only when consumers need the same concept. See the [figure guide](../development/benchmark-figures.md).

```text
catalog → typed presentation → report values
Observable Plot + font metrics + theme → templates → report writers / Vue components
```

Renderers return deterministic SVG strings, with no filesystem, network or host-global state. Each owns a private lightweight SVG document rather than modifying the browser's DOM. Observable Plot owns scales, ticks, axes, clipping and marks; linkedom supplies documents/serialization and string-pixel-width supplies Inter metrics. Keep Bencher semantics in `continuous/`; chart templates receive formatted values and verdicts.

## Dependencies and verification

Node 24 runs TypeScript directly. The pinned native TypeScript compiler checks the tooling; Vue's content mapper checks SFCs through the same compiler. Commander owns CLI parsing, Zod the strict plan schema and editor schema, smol-toml/JSON syntax decoding, and `catalog.revisionSampling` the per-case revision budget. Native archives are hash-verified before extraction with the tar library and source stamps are verified again during offline compilation. Repository tooling uses no Python runtime. Do not maintain parallel parsers, registries, drawing engines or statistical definitions.

Tests cover result compatibility, statistics, report generation and external trust or recovery boundaries; they do not pin SVG geometry or library attribute order. Visual acceptance renders old and new figures from the same recorded results in light and dark appearance; synthetic fixtures cover edge cases, not measured performance. Raw results, vendor trees and generated reports stay out of Git.
