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

Use the `software-design-philosophy` skill for structural engine changes. Judge maintainability by change amplification, cognitive load and hidden dependencies, not by line or file count.

- **Trace real changes.** Before choosing a boundary, follow tasks such as changing sampling, tile capacity, partition selection, block layout or task size. Each rule has one owner; consumers do not reconstruct it. Use the [engine vocabulary](docs/design/engine.md#vocabulary): subproblems, buckets, tiles, blocks, primitives, cost models and scheduling are distinct.
- **Deep operations, not wrappers.** A boundary hides decisions or owns a complete invariant-preserving operation. Do not add a type, trait or forwarding layer merely to name a concept.
- **One analysis for serial and parallel.** Partition choice, bucket meaning and finishing rules have one authority; parallel code differs only in scheduling and ownership transfer.
- **Generalize, don't delete.** Retire a fast path only when a more general mechanism keeps its savings. Do not discard known ranges, bounds or classifications, and audit every added scan, allocation or pass.
- **Derive constants.** Separate representation limits, budgets and tuned parameters; derive values from existing constraints instead of adding distribution-specific exceptions.
- **Report trade-offs.** A generalization may accept measured local losses if representative coverage improves; list the losses individually. Keep structural and performance commits separate, commit each verifiable stage, and record which checks ran and what acceptance is outstanding.

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
