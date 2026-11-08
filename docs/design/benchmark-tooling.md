---
type: Reference
title: Benchmark tooling
description: Locate ownership before changing plans, builds, reports or figures.
tags: [bench, architecture]
---

# Benchmark tooling

`pnpm bench` owns three complete workflows: dirty-tree/commit screening, algorithm comparisons and continuous PR feedback. Commands and measurement contracts are in the [protocol](../development/benchmarking.md); this page identifies where to change behavior.

## Ownership

| Source | Owns |
|---|---|
| `bench/index.ts` | CLI validation and workflow dispatch (Commander) |
| `bench/catalog.toml`, `catalog.ts` | Identities, labels, families, record widths, build names and suite order |
| `bench/plans/`, `plan.ts`, `config.ts` | Presets, strict schema, CLI overrides and syntax decoding |
| `bench/workspace.ts` | Source snapshots, builds, temporary worktrees and cleanup |
| `bench/local/` | Commit comparison, process-level blocks, A/A analysis and report |
| `bench/release.ts`, `bench/rust/` | Algorithm session orchestration, verified timing and allocation measurement |
| `bench/report/` | Persisted-result validation, statistics, figures and BMF |
| `bench/continuous/` | Bencher alerts/history, GitHub publication and worker scheduling |
| `bench/plot/` | Host-independent figure templates, visual encoding and theme |
| `bench/fetch.py` | Pinned native-source acquisition |

Callers request a whole build/run/report operation, not a sequence they must clean up themselves. Report regeneration reads persisted results without rebuilding or contacting measurement services. Process-level screens, in-process paired measurements and Bencher alerts remain distinct even when they share presentation.

## One catalog and plan contract

The catalog is the language-neutral authority for identities and display names. TypeScript and Rust validate schema version 1; reports use its labels rather than local dictionaries or string heuristics. Historical IDs have readable fallbacks. Executable adapters, capability checks and input generation remain typed Rust code; the catalog does not store functions. Deployment matrices and artifact/security boundaries remain explicit in workflows.

Repository-authored selections use TOML; custom plans accept JSON too. Decode syntax once, then use one validated model: products within inclusion rules, union across rules, exclusions and deduplication. TypeScript owns input validation, CLI overrides and editor JSON Schema; Rust expands cases, including boundaries resolved from the measured crate. Do not recreate the case inventory in the CLI.

`local` and `continuous` use Criterion; `smoke` and `release` use the paired algorithm harness. This is a capability boundary, not a second selection language. Continuous defaults compile into the executable so trusted cloud supervision never evaluates a PR's configuration. Local results retain the normalized plan. [Plan tests](../../bench/plan.test.ts) and Rust tests protect selection counts and boundary behavior.

## Source comparison boundary

The workspace materializes current staged, unstaged and untracked sources without changing the caller's checkout. Internal links become regular snapshot contents; dangling/escaping links and directory cycles fail before building. Baseline links resolve within that commit's archive, never against the live checkout. Source hashing, backup and compilation see the same materialized inputs.

Both revisions use the current harness, lockfile, workspace and toolchain; only the baseline library is replaced. Incompatible historical APIs or selected timed IDs fail instead of silently substituting a baseline. Cargo JSON selects executables and matching build-script provenance, including configured targets and compiler overrides; incompatible settings fail before timing.

Temporary sources and builds belong to the workspace; result directories retain provenance, observations, summaries and figures. Interrupted local runs are inspectable, not resumable or reportable as complete. The [algorithm harness](comparison-benchmarks.md) separately supports committed-group resume.

## Figure boundary

Templates accept chart meaning: names, values, bounds, groups, columns and legends, not arbitrary CSS or coordinate callbacks. Reuse a template for new cases; add a complete template for a new visual structure. Promote shared operations only when consumers need the same concept. See the [figure guide](../development/benchmark-figures.md).

```text
catalog → typed presentation → report values
Observable Plot + font metrics + theme → templates → report writers / Vue components
```

Renderers return deterministic SVG strings, with no filesystem, network or host-global state. Each owns a private lightweight SVG document rather than modifying the browser's DOM. Observable Plot owns scales, ticks, axes, clipping and marks; linkedom supplies documents/serialization and string-pixel-width supplies Inter metrics. Keep Bencher semantics in `continuous/`; chart templates receive formatted values and verdicts.

## Dependencies and verification

Node 24 runs TypeScript directly. The pinned native TypeScript compiler checks the tooling; Vue's content mapper checks SFCs through the same compiler. Commander owns CLI parsing, Zod the strict plan schema and editor schema, smol-toml/JSON syntax decoding, and Criterion continuous calibration. Do not maintain parallel parsers, registries, drawing engines or statistical definitions.

Tests cover result compatibility, statistics and semantic SVG geometry rather than library attribute order. Visual acceptance renders old and new figures from the same recorded results in light and dark appearance; synthetic fixtures cover edge cases, not measured performance. Raw results, vendor trees and generated reports stay out of Git.
