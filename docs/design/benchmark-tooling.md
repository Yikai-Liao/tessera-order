# Benchmark tooling

The benchmark subsystem serves three workflows: working-tree comparisons during development, published algorithm comparisons, and continuous regression tracking. It is exposed through the repository CLI, `pnpm tools`; its browser-compatible presentation modules can also be imported by VitePress.

## Ownership

| Boundary | Owns | Does not own |
|---|---|---|
| CLI (`tools/`) | Task groups, argument validation, compact results and dispatch | Case lists, experiment statistics, chart layout or CI event policy |
| Status (`tools/status.ts`) | Exact remote target resolution, workflow/check association and bounded default feedback | Submitting jobs or inferring a benchmark verdict from workflow success |
| Continuous execution | Static artifacts, the local/cloud scheduling protocol and Bencher submission policy | Secrets, image publication, event trust or queue ownership |
| Repository checks (`.github/scripts/`) | Routing, title, DCO and release-input policy shared with the CLI | Runner provisioning or registry publication |
| Plan | Format-independent contract, presets, overrides, products, exclusions and native case expansion | Measurement scheduling or report layout |
| Workspace | Immutable source snapshot, build location, temporary worktree and cleanup | Result retention policy |
| Local comparison | Two revisions built with one current harness; process-level blocks and A/A screening | In-process placement controls |
| Rust comparison | Cases, supported adapters, verified timing, allocation measurement and resumable raw groups | Display labels |
| Algorithm report | Raw-result validation, paired summaries and presentation of a release session | Running executables |
| Continuous reporting | Bencher alert semantics, PR history and GitHub publication | Local A/A verdicts |
| Figure templates | Geometry, visual encoding, typography, theme and accessible SVG | Filesystem access, GitHub or benchmark statistics |

The CLI delegates complete operations rather than asking callers to pair build, run, report and cleanup steps. Experiment grammar, continuous tasks and repository checks are separate command groups under `tools/`; command callbacks do not own build or measurement policy. Subprocess logging has one operation-scoped owner, so commands return compact JSON without dropping diagnostics. Status defaults to a projection of the exact target, latest workflows and published benchmark verdicts; full summaries and comments are opt-in rather than serialized GitHub payloads. Report regeneration consumes persisted results and does not rebuild code or contact a measurement service.

The two A/A workflows and Bencher's alert model remain distinct. Their summaries can use the same presentation foundations without treating different measurement methods as interchangeable.

## One catalog

[`bench/catalog.toml`](../../bench/catalog.toml) is the language-neutral authority for algorithm IDs, display labels, stability, sort families, record representations, distributions, API modes, build names and comparison suite order. Schema version 1 is checked by the TypeScript accessor, Rust parser and catalog tests. All repository-authored benchmark configurations and selection fixtures use TOML; JSON remains an accepted input and a machine-result format.

The TypeScript accessor supplies readable fallbacks for historical IDs. Reports do not need their own dictionaries or string heuristics. A distribution can have a compact label for dense PR tables and a full label for release figures; both belong to its one catalog entry. Rust parses and caches the catalog once per executable. Plan validation and adapter selection consume that same typed representation, without a generated Rust registry or a second stability/name table. Record widths also belong to the catalog and resolve code-relative tile-boundary cases.

Executable adapters and deterministic input generation remain typed Rust code. The catalog describes identities and presentation, not executable functions. Rust tests and smoke validation check that registered adapters support their advertised tasks.

Build execution and artifact publication still have two explicit CI configurations. Changing that deployment matrix requires updating the workflow as well as the catalog; adding a display label does not. Artifact names and the privileged/unprivileged workflow boundary are deployment contracts, not chart configuration.

## Experiment selection

Plans in `bench/plans/` are the authority for workflow coverage. Repository presets use TOML; custom files accept JSON and TOML. Syntax decoding is confined to the input boundary, followed by one validated plan model. Reports and measurement scheduling do not branch on the source format; persisted effective plans use JSON. They share one schema and selection algebra: products within an inclusion rule, union across inclusion rules, then exclusions and deduplication. `bench/plan.ts` owns user-facing validation, editor schema and CLI overrides. `bench/rust/src/plan.rs` owns expansion, including boundaries resolved from the measured library. The CLI never recreates a native case inventory or relies on a regex to represent the plan.

`local` and `continuous` use the Criterion harness; `smoke` and `release` use the paired algorithm harness. This is a capability boundary, not a second selection language. Numeric lengths are open, record and distribution identities come from the catalog, and each harness rejects unsupported selectors. Continuous defaults are compiled into each executable, so the trusted cloud supervisor does not evaluate a PR's configuration language or parse the plan itself. Local runs persist their effective plan and pass its path to both executables.

Preset changes do not require editing case-registration functions. The continuous preset preserves the 50 input cases and 56 timed IDs; the algorithm presets preserve 70 smoke and 260 release inputs. Tests check these counts, custom products, exclusions, deduplication and code-relative boundaries. Required controls are retained when opponents are overridden; per-type adapter capabilities remain executable Rust knowledge.

## Source comparisons

A local comparison copies the current crate and harness, including staged, unstaged and untracked source files, into an isolated worktree. The caller's index and working tree are not changed. Repository-internal file and directory links are materialized into regular snapshot contents; links escaping the repository, dangling links and directory cycles fail before building. The baseline is materialized in a separate archive of its own repository, so library links to files outside the crate resolve against the selected commit, not the current checkout. Links outside that archive, including absolute links into the live checkout, are rejected. Only the materialized library is installed into the current harness. Source backups follow the same isolation rule, so hashing and compilation see the same inputs. Both executables use the current harness, workspace settings, lockfile and toolchain; only the library crate is replaced by the requested commit's version for the baseline build.

The selected timed IDs must match in both executables. A historical commit must contain the crate and support the current harness's API and dependency constraints; incompatible commits fail at build or case selection instead of silently using another baseline. Threshold changes may change boundary IDs, so select common cases when comparing across such a change.

Nine blocks rotate through the six orders of baseline, baseline control and current. Every process has a fresh Criterion directory. Its mean nanoseconds per sort is the observation. A block ratio divides current by the mean of the two baseline observations. A direction needs eight agreeing blocks and a median outside the paired A/A range. This screens execution changes but does not control different binary layouts.

Cargo JSON messages select each executable and its matching build-script provenance, including configured targets, compiler overrides and target-specific flags. The local runner retains both builds' provenance and rejects incompatible compiler settings instead of reconstructing them from its environment. The workspace owns temporary sources, executables, Criterion files and build products. Result directories retain provenance, completed block observations, summaries and figures. An interrupted local run can be inspected but cannot be resumed or reported as complete. The lower-level Rust algorithm runner retains its existing group-resume contract when its executable and manifests remain available.

## Extending figures

The normal interface accepts chart meaning: names, values, bounds, groups, columns and legends. It does not accept arbitrary CSS, fonts or coordinate callbacks. Existing row templates cover rankings, input fields, element types, memory ranges and local comparisons. PR comparisons and history use specialized templates with the same visual tokens and figure surface.

New data or algorithms use the catalog and an existing template. A new visual structure gets a complete template under `bench/plot/`, or remains a domain-specific template that imports its shared foundations. It does not add unrelated mode switches to the row template. Promote an operation into the shared layer when more than one template needs the same concept.

The host-independent dependency direction is:

```text
catalog.toml → catalog.ts → result presentation
Observable Plot + font metrics + theme → figure templates → report writers / Vue components
```

Figure renderers are deterministic string-producing functions without Node, network access or host-global state. Each render creates its own lightweight SVG document; it does not read or alter a browser’s live DOM. Offline reports write the strings to files; the VitePress adapter renders the same strings as SVG images.

## Dependencies

The official TypeScript 7.1 beta native compiler checks the tooling and the host-independent plot boundary. Vue's official content mapper checks SFC scripts and templates through the same compiler; external code is enabled only by the explicit documentation typecheck command. Exact prerelease versions are pinned in `package.json` and `pnpm-lock.yaml`.

Node's filesystem, process and test APIs provide resource handling and verification. Repository scripts, native build orchestration and the continuous supervisor use TypeScript on Node.js 24, with no Python tooling runtime. The tar library owns secure archive extraction after the pinned SHA-256 has been checked; the tooling does not implement an archive parser. Dependency-free CI bootstrap adapters share routing and DCO functions with the repository CLI while keeping the trusted-base execution boundary. The JSON runtime parser and smol-toml decode document syntax; Rust uses serde_json and toml. Zod owns the plan's strict runtime schema and editor JSON Schema; handwritten field-validation and schema-generation mechanisms are not maintained alongside it. Commander owns command parsing, help, option validation and errors; the CLI does not maintain its own parser. Criterion already owns calibration and continuous-run estimates.

Observable Plot owns quantitative scales, tick placement, axes, symbols, collision placement, line clipping and SVG mark generation. String-pixel-width supplies Inter font metrics; linkedom’s browser-compatible worker export supplies a private SVG document and serialization in both Node and VitePress. The templates retain the project’s table-like compositions: label and value columns, grouped rows, highlighted SIMD rings and per-row trend scales. They do not maintain a parallel drawing engine. Bencher selection and alert semantics stay in `continuous/`; the change and trend templates accept only formatted names, values and visual verdicts.

The original SVG fixtures remain unchanged as content and canvas references. Tests compare their titles, descriptions, labels and dimensions with the new output, and check semantic geometry separately rather than depending on a library’s attribute order. Visual acceptance additionally requires rendering old and new versions from the same recorded results in light and dark appearance; synthetic fixtures cover boundary cases, not measured performance.
