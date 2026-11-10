# Instructions for coding agents

Tessera is a stable radix sort with O(√n) auxiliary memory. Keep code concise, readable and in English.

## Read only for your task

Read the matching sections below; batch known required reads in the first tool interaction and skip content already loaded. When an entry links to a section, locate its heading and use line-range reads for that section and needed subsections. Read other sections only as the task requires, rather than loading the whole document by default. Follow deeper links only when needed. A fact lookup needs its contract or reference, not a development procedure. The [human task index](docs/README.md) covers other topics.

| Task | First read together |
|---|---|
| Check ordering, panic or memory guarantees | [Semantics](conformance/semantics.md) |
| Change the Rust engine, keys or block partition | [Engine procedure and validation](CONTRIBUTING.md#engine-changes), [engine reference](docs/design/engine.md), [semantics](conformance/semantics.md) |
| Change routing, thresholds, digit planning or inlining | The engine-change set above, [measurement procedure](docs/development/benchmarking.md#run-an-experiment), [source-attribution controls](docs/development/benchmarking.md#additional-source-attribution-controls), relevant [engine evidence](docs/design/port.md) |
| Compare source revisions or sorting algorithms | [Command capabilities and procedure](docs/development/benchmarking.md#command-capabilities), then the selected command's contract on that page |
| Regenerate a retained report | [Command capabilities](docs/development/benchmarking.md#command-capabilities); statistics and result contracts on that page only if interpreting or changing them |
| Read current-head PR performance feedback | [PR feedback](docs/development/continuous-benchmarks.md#read-feedback-from-a-pr); normal reading needs no measurement protocol |
| Investigate main/history performance or tracking | [Existing evidence](docs/development/continuous-benchmarks.md#read-main-and-history); tracking internals on that page when needed |
| Change benchmark tooling or figures | [Tooling boundaries](docs/design/benchmark-tooling.md); the affected command's contract and [figure guide](docs/development/benchmark-figures.md) only when relevant |
| Documentation or site | [Documentation rules](CONTRIBUTING.md#documentation); [site guide](docs/development/docs-site.md) for pages or models |
| CI, release or planned APIs | [CI](docs/development/ci.md), [release](RELEASING.md) or [roadmap](ROADMAP.md), respectively |
| Open a PR or commit | [Contributing](CONTRIBUTING.md) |

## Use the benchmark CLI

Use `pnpm tools` for covered tasks; do not recreate its build/run/report orchestration, timing, A/A, verification or provenance in ad-hoc scripts. Run from the repository root:

| Task | Command |
|---|---|
| Working tree versus a commit | `pnpm tools compare commit COMMIT` |
| Paired sorting-algorithm comparison | `pnpm tools compare algorithms` |
| Preview inputs without measuring | `pnpm -s tools plan NAME` |
| Regenerate retained results | `pnpm tools report DIR` |
| Read current-head PR feedback | `pnpm -s tools status --pr NUMBER` |

Replace uppercase placeholders. The [capability table](docs/development/benchmarking.md#command-capabilities) owns defaults, coverage options and evidence limits. A read-only feedback request does not authorize new measurements. Before a custom experiment, identify the concrete unsupported requirement and obtain operator agreement; prefer extending the maintained pipeline.

Read main feedback with `pnpm -s tools status main`. Status is compact JSON: match `target.head`, read all active `benchmarks[]` verdicts and `workflows[].state`; use `--details` for check summaries and the matching report body. A successful read is not a passing benchmark. The [CLI reference](docs/development/tooling.md) owns output and logging contracts.

## Engine design and simplification

Use the `software-design-philosophy` skill for structural engine changes. Judge maintainability by change amplification, cognitive load and hidden dependencies, not by deduplication, file count or added abstraction alone.

- **Evaluate real changes.** Before choosing a boundary, trace representative tasks such as changing sampling, tile capacity, partition selection, block layout or parallel task size. Compare what a maintainer must understand, which places must change together and which cross-module invariants must remain synchronized. Give each rule one owner; consumers should not need to reconstruct its implementation.
- **Model concepts hierarchically.** Distinguish the sorting subproblem and remaining candidate bits, logical buckets, resource-bounded tiles, physical blocks, sorting/partition primitives, cost models and execution scheduling. A bucket may span blocks, fit a tile or share a task with other buckets; these are not interchangeable identities. Put representation limits and ownership invariants with the mechanisms that enforce them, and strategy choices with the planner. Use the engine reference for precise vocabulary rather than inventing parallel definitions.
- **Use deep operations, not wrappers.** A boundary must hide decisions or own a complete invariant-preserving operation, reducing caller knowledge, sequencing and cleanup obligations. Do not create a type, trait, intermediate representation or forwarding layer merely to name a concept or shorten a function. Count interface, navigation, state and coordination costs alongside removed duplication and total code size. Remove abstractions whose net benefit does not hold.
- **Share serial and parallel semantics.** Input analysis, partition choice, bucket meaning and finishing rules have one authority. Keep scheduling and necessary ownership-transfer differences at their execution boundary; a parallel fallback consumes existing analysis rather than independently rediscovering it. Any implementation copies retained for code generation need measured justification and shared invariant coverage under the contributing rules.
- **Replace specialization by generalization.** An effective specialized mechanism may be retired only when a more general mechanism replaces multiple specialized branches or generalizes that mechanism. Identify the information and work savings the original provides, their replacement owner and the broader cases served. Simply deleting a fast path or keeping only the old generic fallback is not simplification under this goal.
- **Preserve useful information and bounded work.** Sharing an implementation must not discard known active ranges, bounds, classifications or reuse opportunities. Retain effective bucket limits instead of traversing full-capacity tables. Audit added scans, allocations, copies and passes explicitly; increased work is a known cost even before its whole-sort timing impact is measured. Do not hide such losses inside a structural refactor.
- **Model costs and constants.** Separate hard representation limits, resource budgets and empirical tuning parameters. Derive values from existing constraints when possible, rather than adding independent constants or distribution-specific exceptions. A cost model should describe the work of the actual primitives and preserve the benefits of the heuristics it replaces. Removing a constant or branch is not a benefit if it merely relocates the same rule or adds compensating special cases.
- **Accept measured trade-offs, not deletion-only regressions.** A modest loss on particular distributions may be acceptable when a genuine generalization reduces concepts or maintenance cost and improves overall performance on representative coverage. Report the individual losses and the structural and performance gains; do not hide losses in an aggregate. Candidates may be developed and correctness-tested before regression work, but remain performance-unaccepted until the benchmark protocol is satisfied. Keep structural and algorithm/performance changes separately identifiable.

Complete each independently verifiable stage with a local commit for rollback and revision comparisons. Record checks actually run and outstanding review or performance acceptance; a checkpoint is not an acceptance verdict. Keep unfinished experiments out of unrelated commits.

## Always preserve

- Stable Rust; MSRV is in `Cargo.toml`. Changes to MSRV, required dependencies or std/no_std support require explicit operator approval and corresponding contract, documentation and CI updates.
- Never copy third-party code into the repository. Keep the BSD-2-Clause SPDX header on new files and Clausecker's notice in `block.rs`; see [license policy](CONTRIBUTING.md#license).
- Every `unsafe` block has a `// SAFETY:` comment; every `unsafe fn` has a `# Safety` section. Preserve panic, inconsistent-key, owned-record and `&mut`-record tests.
- Routing, thresholds, digit planning and inlining change only with measurements under the benchmark protocol, including A/A ranges. Investigate changes outside the controls. Raw measurements and research harnesses stay outside Git.
- Refactors follow the [refactoring rule](CONTRIBUTING.md#refactoring): no change to algorithm logic, inputs or benchmark protocol, and timed regions call the same library functions without added indirection; behavior and performance changes ship separately.
- Use the [engine vocabulary](docs/design/engine.md#vocabulary). Names describe values and units: `records`, `keys`, `extractor`, `_bytes`, `_bits`, `_len`; not `arr`, `tmp` or `data`.
- Rust items have a summary; public items document contracts and applicable panic, safety, memory and asserting-doctest examples. Comments explain invariants, hazards and measured choices, not editing history.
- Update semantics with contract changes; engine explanations and `docs/.vitepress/theme/sim/` with algorithm changes. Keep current rationale in its topic, not a parallel decision record. Update the roadmap for planned work and describe user-visible changes in the PR.
- Formatters and linters own style. Fix causes; suppress only locally with a reason. Follow Conventional Commits (64-character title limit), sign off with the configured identity, and add an `Assisted-by:` or `Co-authored-by:` trailer.

## Independent review

Every PR must pass an independent subagent Agentic Review before merge. Use `yume/gpt-6.1-sol` for review; report unavailability rather than silently substituting a model. Structural engine reviews must evaluate the design principles above against representative modification tasks, identify which old mechanisms disappear and where their benefits are preserved, and assess the net understanding and change surface—not just correctness or duplicate lines. Resolve accepted findings and re-review material fixes. Tests and CI do not replace review of runtime paths, trust boundaries and documentation contracts. Review names each new consumer of a shared rule (case ID or mode, catalog field, native registry index, threshold) with its owner. Accepted debt that is not fixed in the PR goes to the [roadmap](ROADMAP.md).

## Commands

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets && cargo test --workspace --doc
pnpm install --frozen-lockfile && pnpm format
pnpm docs:test && pnpm docs:typecheck && pnpm docs:build     # site / models
pnpm tools:test && pnpm tools:typecheck                    # benchmark tooling
```

Run applicable checks locally; engine feature and Miri checks are in [contributing](CONTRIBUTING.md#validation). Miri, MSRV, platform and benchmark checks also run in CI; [CI](docs/development/ci.md) explains coverage. A green tracking workflow is not a benchmark verdict: read the current-head Portable SIMD benchmark check.
