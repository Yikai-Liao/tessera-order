---
type: Policy
title: Agent entry point
description: Start a repository task with safety rules and focused reading paths.
tags: [agent, docs]
---

# Instructions for coding agents

Tessera is a stable radix sort with O(√n) auxiliary memory. Keep code concise, readable and in English.

## Read only for your task

Start with the matching row, not the whole documentation tree. Each document has an OKF description and tags; [the index](docs/index.md) and `pnpm knowledge:build` expose them progressively. Without the CLI, read the linked file directly. With `okf` 0.5.2 installed, run `pnpm knowledge:build`, then `okf search .knowledge --tag engine --type Reference` for candidate IDs and `okf show .knowledge ID` only for the selected topic; generated directory indexes carry descriptions.

| Task | Read before changing |
|---|---|
| Rust engine, keys or block partition | [Engine](docs/design/engine.md), [semantics](conformance/semantics.md); [measurements](docs/design/port.md) when changing routing, constants or inlining |
| Measure a change | [Benchmark protocol](docs/development/benchmarking.md) |
| Read PR performance feedback | [Continuous benchmarks](docs/development/continuous-benchmarks.md#read-feedback-from-a-pr) |
| Benchmark tooling or figures | [Tooling boundaries](docs/design/benchmark-tooling.md), then [figures](docs/development/benchmark-figures.md) if needed |
| Documentation or retrieval | [Knowledge workflow](docs/development/knowledge.md); [site](docs/development/docs-site.md) for pages or models |
| CI, release or planned APIs | [CI](docs/development/ci.md), [release](RELEASING.md) or [roadmap](ROADMAP.md), respectively |
| Open a PR or commit | [Contributing](CONTRIBUTING.md) |

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
pnpm knowledge:test && pnpm knowledge:build                  # Markdown / retrieval
pnpm docs:test && pnpm docs:typecheck && pnpm docs:build     # site / models
pnpm bench:test && pnpm bench:typecheck                    # benchmark tooling
python3 -m unittest discover -s bench/continuous -p "test_*.py"
python3 -m unittest discover -s .github/scripts -p "test_*.py"
```

Run applicable checks locally. Miri, MSRV, platform and benchmark checks also run in CI; [CI](docs/development/ci.md) explains coverage. A green tracking workflow is not a benchmark verdict: read both current-head benchmark checks.
