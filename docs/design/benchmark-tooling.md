# Benchmark tooling

The benchmark subsystem serves three workflows: working-tree comparisons during development, published algorithm comparisons, and continuous regression tracking. Its public command is `pnpm bench`; its browser-compatible presentation modules can also be imported by VitePress.

## Ownership

| Boundary | Owns | Does not own |
|---|---|---|
| CLI | Commands, argument validation and workflow dispatch | Case lists, experiment statistics or chart layout |
| Plan | JSON contract, presets, overrides, products, exclusions and native case expansion | Measurement scheduling or report layout |
| Workspace | Immutable source snapshot, build location, temporary worktree and cleanup | Result retention policy |
| Local comparison | Two revisions built with one current harness; process-level blocks and A/A screening | In-process placement controls |
| Rust comparison | Cases, supported adapters, verified timing, allocation measurement and resumable raw groups | Display labels |
| Algorithm report | Raw-result validation, paired summaries and presentation of a release session | Running executables |
| Continuous reporting | Bencher alert semantics, PR history and GitHub publication | Local A/A verdicts |
| Figure templates | Geometry, visual encoding, typography, theme and accessible SVG | Filesystem access, GitHub or benchmark statistics |

The CLI delegates complete operations rather than asking callers to pair build, run, report and cleanup steps. Report regeneration consumes persisted results and does not rebuild code or contact a measurement service.

The two A/A workflows and Bencher's alert model remain distinct. Their summaries can use the same presentation foundations without treating different measurement methods as interchangeable.

## One catalog

[`bench/catalog.json`](../../bench/catalog.json) is the language-neutral authority for algorithm IDs, display labels, stability, sort families, record representations, distributions, API modes, build names and comparison suite order. Schema version 1 is checked by the Rust generator and catalog tests.

The TypeScript accessor supplies readable fallbacks for historical IDs. Reports do not need their own dictionaries or string heuristics. A distribution can have a compact label for dense PR tables and a full label for release figures; both belong to its one catalog entry. Rust's build script generates the supported-type and adapter registry into `OUT_DIR`, so adding an algorithm's metadata does not require a second stability/name table. Record widths also belong to the catalog and resolve code-relative tile-boundary cases.

Executable adapters and deterministic input generation remain typed Rust code. The catalog describes identities and presentation, not executable functions. Rust tests and smoke validation check that registered adapters support their advertised tasks.

Build execution and artifact publication still have two explicit CI configurations. Changing that deployment matrix requires updating the workflow as well as the catalog; adding a display label does not. Artifact names and the privileged/unprivileged workflow boundary are deployment contracts, not chart configuration.

## Experiment selection

Plans in `bench/plans/` are the authority for workflow coverage. They share one schema and selection algebra: products within an inclusion rule, union across inclusion rules, then exclusions and deduplication. `bench/plan.ts` owns user-facing validation, editor schema and CLI overrides. `bench/rust/src/plan.rs` owns expansion, including boundaries resolved from the measured library. The CLI never recreates a native case inventory or relies on a regex to represent the plan.

`local` and `continuous` use the Criterion harness; `smoke` and `release` use the paired algorithm harness. This is a capability boundary, not a second selection language. Numeric lengths are open, record and distribution identities come from the catalog, and each harness rejects unsupported selectors. Continuous defaults are compiled into each executable, so the trusted cloud supervisor does not evaluate a PR's configuration language or need Node to parse the plan. Local runs persist their effective plan and pass its path to both executables.

Preset changes do not require editing case-registration functions. The continuous preset preserves the 50 input cases and 56 timed IDs; the algorithm presets preserve 70 smoke and 260 release inputs. Tests check these counts, custom products, exclusions, deduplication and code-relative boundaries. Required controls are retained when opponents are overridden; per-type adapter capabilities remain executable Rust knowledge.

## Source comparisons

A local comparison copies the current crate and harness, including staged, unstaged and untracked source files, into an isolated worktree. The caller's index and working tree are not changed. Both executables use the current harness, workspace settings, lockfile and toolchain; only the library crate is replaced by the requested commit's version for the baseline build.

The selected timed IDs must match in both executables. A historical commit must contain the crate and support the current harness's API and dependency constraints; incompatible commits fail at build or case selection instead of silently using another baseline. Threshold changes may change boundary IDs, so select common cases when comparing across such a change.

Nine blocks rotate through the six orders of baseline, baseline control and current. Every process has a fresh Criterion directory. Its mean nanoseconds per sort is the observation. A block ratio divides current by the mean of the two baseline observations. A direction needs eight agreeing blocks and a median outside the paired A/A range. This screens execution changes but does not control different binary layouts.

The workspace owns temporary sources, executables, Criterion files and build products. Result directories retain provenance, completed block observations, summaries and figures. An interrupted local run can be inspected but cannot be resumed or reported as complete. The lower-level Rust algorithm runner retains its existing group-resume contract when its executable and manifests remain available.

## Extending figures

The normal interface accepts chart meaning: names, values, bounds, groups, columns and legends. It does not accept arbitrary CSS, fonts or coordinate callbacks. Existing row templates cover rankings, input fields, element types, memory ranges and local comparisons. PR comparisons and history use specialized templates with the same visual tokens and figure surface.

New data or algorithms use the catalog and an existing template. A new visual structure gets a complete template under `bench/plot/`, or remains a domain-specific template that imports its shared foundations. It does not add unrelated mode switches to the row template. Promote an operation into the shared layer when more than one template needs the same concept.

The host-independent dependency direction is:

```text
catalog.json → catalog.ts → result presentation
theme.ts → SVG surface and layout → figure templates → report writers / Vue components
```

Figure renderers are deterministic string-producing functions without Node, DOM, network or mutable global state. Offline reports write the strings to files; the VitePress adapter renders the same strings as SVG images.

## Dependencies

The official TypeScript 7.1 beta native compiler checks the tooling and the host-independent plot boundary. Vue's official content mapper checks SFC scripts and templates through the same compiler; external code is enabled only by the explicit documentation typecheck command. Exact prerelease versions are pinned in `package.json` and `pnpm-lock.yaml`.

Node's filesystem, process and test APIs provide resource handling and verification. Zod owns the plan's strict runtime schema and editor JSON Schema; handwritten field-validation and schema-generation mechanisms are not maintained alongside it. Commander owns command parsing, help, option validation and errors; the CLI does not maintain its own parser. Criterion already owns calibration and continuous-run estimates.

The custom SVG templates retain the project's label columns, overlaid SIMD rings, collision lanes, accessibility descriptions and matching dark theme. Replacing a logarithmic interpolation formula with a library would not materially shrink this code. A full chart-library migration would need to remove the template implementation, including its layout adaptations, rather than wrapping it with another layer. Current SVG fixtures provide a comparison point for evaluating that migration.
