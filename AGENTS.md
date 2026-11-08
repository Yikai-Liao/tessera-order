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

Use `pnpm bench` for covered tasks; do not recreate its build/run/report orchestration, timing, A/A, verification or provenance in ad-hoc scripts. Run from the repository root:

| Task | Command |
|---|---|
| Working tree versus a commit | `pnpm bench compare COMMIT` |
| Paired sorting-algorithm comparison | `pnpm bench run` |
| Preview inputs without measuring | `pnpm --silent bench plan NAME` |
| Regenerate retained results | `pnpm bench report DIR` |
| Read current-head PR feedback | `pnpm --silent bench pr NUMBER --json` |

Replace uppercase placeholders. The [capability table](docs/development/benchmarking.md#command-capabilities) owns defaults, coverage options and evidence limits. A read-only feedback request does not authorize new measurements. Before a custom experiment, identify the concrete unsupported requirement and obtain operator agreement; prefer extending the maintained pipeline.

## Always preserve

- Stable Rust, no required dependencies; MSRV is in `Cargo.toml`.
- Never copy third-party code into the repository. Keep the BSD-2-Clause SPDX header on new files and Clausecker's notice in `block.rs`; see [license policy](CONTRIBUTING.md#license).
- Every `unsafe` block has a `// SAFETY:` comment; every `unsafe fn` has a `# Safety` section. Preserve panic, inconsistent-key, owned-record and `&mut`-record tests.
- Routing, thresholds, digit planning and inlining change only with measurements under the benchmark protocol, including A/A ranges. Investigate changes outside the controls. Raw measurements and research harnesses stay outside Git.
- Use the [engine vocabulary](docs/design/engine.md#vocabulary). Names describe values and units: `records`, `keys`, `extractor`, `_bytes`, `_bits`, `_len`; not `arr`, `tmp` or `data`.
- Rust items have a summary; public items document contracts and applicable panic, safety, memory and asserting-doctest examples. Comments explain invariants, hazards and measured choices, not editing history.
- Update semantics with contract changes; engine explanations and `docs/.vitepress/theme/sim/` with algorithm changes. Keep current rationale in its topic, not a parallel decision record. Update the roadmap for planned work and describe user-visible changes in the PR.
- Formatters and linters own style. Fix causes; suppress only locally with a reason. Follow Conventional Commits (64-character title limit), sign off with the configured identity, and add an `Assisted-by:` or `Co-authored-by:` trailer.

## Commands

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets && cargo test --workspace --doc
pnpm install --frozen-lockfile && pnpm format
uvx ruff@0.17.4 format && uvx ruff@0.17.4 check
pnpm docs:test && pnpm docs:typecheck && pnpm docs:build     # site / models
pnpm bench:test && pnpm bench:typecheck                    # benchmark tooling
python3 -m unittest discover -s bench/continuous -p "test_*.py"
python3 -m unittest discover -s .github/scripts -p "test_*.py"
```

Run applicable checks locally; engine feature and Miri checks are in [contributing](CONTRIBUTING.md#validation). Miri, MSRV, platform and benchmark checks also run in CI; [CI](docs/development/ci.md) explains coverage. A green tracking workflow is not a benchmark verdict: read both current-head benchmark checks.
