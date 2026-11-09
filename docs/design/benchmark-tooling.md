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
| `bench/local/` | Commit comparison, process-level blocks, A/A analysis and report |
| `bench/release.ts`, `bench/session.ts` | Preparation, retained builds, validation receipts and session recovery |
| `bench/rust/` | Verified timing, allocation measurement and resumable journals |
| `bench/report/` | Persisted-result validation, statistics, figures and BMF |
| `bench/continuous/` | Bencher alerts/history, GitHub publication and worker scheduling |
| `bench/plot/` | Host-independent figure templates, visual encoding and theme |
| `bench/rust/native/` | Pinned archive acquisition, source verification and offline compilation |

Callers request a whole build/run/report operation, not a sequence they must clean up themselves. Report regeneration reads persisted results without rebuilding or contacting measurement services. Process-level screens, in-process paired measurements and Bencher alerts remain distinct even when they share presentation.

Status resolves the requested remote ref once and associates PR test-merge workflows separately from head-attached benchmark checks. Only published check identities associate the trusted tracker; missing feedback is not success. The [CLI reference](../development/tooling.md) defines compact output and diagnostics.

Continuous build, scheduling and submission operations are shared between CI and local callers. Workflows retain event attribution, image publication, credentials and queue ownership. Unprivileged builds run PR code; the privileged tracker uses trusted default-branch code and packages uploaded executables without running them locally. Trusted-base DCO uses dependency-free adapters without installing PR code.

## One catalog and plan contract

The catalog is the language-neutral authority for identities and display names. TypeScript and Rust validate schema version 1; reports use its labels rather than local dictionaries or string heuristics. Historical IDs have readable fallbacks. Executable adapters, capability checks and input generation remain typed Rust code; the catalog does not store functions. Deployment matrices and artifact/security boundaries remain explicit in workflows. New consumers of an ID, mode or record width use the catalog and the owning formatter, not positional parsing or local constants. Code copied into the trusted image stays built-ins-only; where it repeats such a rule, unit tests check it against the owner. Rust and native registries that index the same records share one type list, and layout and output tests cover every entry.

Repository-authored selections use TOML; custom plans accept JSON too. Decode syntax once, then use one validated model: products within inclusion rules, union across rules, exclusions and deduplication. TypeScript owns input validation, CLI overrides and editor JSON Schema; Rust expands cases, including boundaries resolved from the measured crate. Do not recreate the case inventory in the CLI.

`local` and `continuous` use Criterion; `smoke` and `release` use the paired algorithm harness. This is a capability boundary, not a second selection language. Continuous defaults compile into the executable so trusted cloud supervision never evaluates a PR's configuration. Local results retain the normalized plan. [Plan tests](../../bench/plan.test.ts) and Rust tests protect selection counts and boundary behavior.

Selection axes are type, distribution and length; there is no mode selector. Criterion IDs end in `/api`; comparison input IDs use `type/distribution/length/s1`. The [continuous plan](../../bench/plans/continuous.toml) owns regression coverage; comparison presets retain 125 smoke inputs and 381 release inputs.

## Source comparison boundary

The workspace materializes current staged, unstaged and untracked sources without changing the caller's checkout. Internal links become regular snapshot contents; dangling/escaping links and directory cycles fail before building. Baseline links resolve within that commit's archive, never against the live checkout. Source hashing, backup and compilation see the same materialized inputs.

Both revisions use the current harness, lockfile, workspace and toolchain; only the baseline library is replaced. Pre-parallel serial baselines receive only the current optional Rayon manifest entries when absent, before source hashing. This keeps Cargo's inactive-feature validation and optional dependency edges compatible with the shared harness and lockfile; it does not enable Rayon or replace the baseline's defaults, selected APIs or backends. The retained baseline hash describes these compiled inputs. Incompatible historical APIs or selected timed IDs fail instead of silently substituting a baseline. Cargo JSON selects executables and matching build-script provenance, including configured targets and compiler overrides; incompatible settings fail before timing.

Optional local layout sampling uses the same workspace and Criterion block runner. Final benchmark rustc arguments select LLD and a fixed function-section shuffle seed without changing dependency compilation flags. `local/layout.ts` validates comparable sources and distinct executable hashes and summarizes observed cross-layout dispersion separately from process-level A/A verdicts. Link maps retain address evidence. Each seed keeps a complete local run; schema-1 reports remain readable, while schema 2 wraps the selected runs and linker provenance. Continuous scheduling and verdicts do not consume this local result format.

`local/protocol-runner.ts` reuses those source, build, case-selection and process operations for a candidate-budget study. The opt-in `layout-study` Rust feature exposes effective timing settings and divided windows without changing ordinary smoke timing. `local/protocol.ts` owns nine held-out cohorts, independent source/layout assignments, fixed alert limits and cost accounting; diagnostic control processes do not enter the scores or projected candidate job costs. Schema 3 retains full arm wall times separately, and `local/protocol-report.ts` regenerates the observed counts and costs. A nominally matched window is not a measured budget guarantee or a cloud verdict.

`local/counters.ts` owns fixed-work Callgrind comparisons and per-event PMU availability probes. It reuses revision snapshots, builds and case selection; the Rust harness verifies outputs around a prepared batch whose stable marker delimits collection. `continuous/volatility.ts` reuses public history loading and report normalization to describe adjacent completed `main` measurements without attributing their changes to layout. Both retain raw observations for offline report regeneration. Optional local CGU/alignment/symbol-order controls use the same revision builder, with effective flags and ordering evidence retained. None of these paths changes Bencher Testbeds or verdicts.

Temporary sources and builds belong to the workspace; result directories retain provenance, observations, summaries and figures. Interrupted local runs are inspectable, not resumable or reportable as complete. Algorithm sessions retain their executable pairs, selection and settings before measuring; `compare resume` uses these exact artifacts without rebuilding. TypeScript owns the session lease and validation receipts. Rust owns host/build compatibility, journal repair and worker kernel locks, so an orphaned worker cannot lose exclusivity merely because its parent lease expires. [The recovery procedure](../development/benchmarking.md#resuming-algorithm-comparisons) defines partial-state handling.

## Figure boundary

Templates accept chart meaning: names, values, bounds, groups, columns and legends, not arbitrary CSS or coordinate callbacks. Reuse a template for new cases; add a complete template for a new visual structure. Promote shared operations only when consumers need the same concept. See the [figure guide](../development/benchmark-figures.md).

```text
catalog → typed presentation → report values
Observable Plot + font metrics + theme → templates → report writers / Vue components
```

Renderers return deterministic SVG strings, with no filesystem, network or host-global state. Each owns a private lightweight SVG document rather than modifying the browser's DOM. Observable Plot owns scales, ticks, axes, clipping and marks; linkedom supplies documents/serialization and string-pixel-width supplies Inter metrics. Keep Bencher semantics in `continuous/`; chart templates receive formatted values and verdicts.

## Dependencies and verification

Node 24 runs TypeScript directly. The pinned native TypeScript compiler checks the tooling; Vue's content mapper checks SFCs through the same compiler. Commander owns CLI parsing, Zod the strict plan schema and editor schema, smol-toml/JSON syntax decoding, and Criterion continuous calibration. Native archives are hash-verified before extraction with the tar library and source stamps are verified again during offline compilation. Repository tooling uses no Python runtime. Do not maintain parallel parsers, registries, drawing engines or statistical definitions.

Tests cover result compatibility, statistics and semantic SVG geometry rather than library attribute order. Visual acceptance renders old and new figures from the same recorded results in light and dark appearance; synthetic fixtures cover edge cases, not measured performance. Raw results, vendor trees and generated reports stay out of Git.
