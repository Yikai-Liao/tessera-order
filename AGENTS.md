# Instructions for coding agents

Tessera is a stable radix sort with O(√n) auxiliary memory. Keep code concise, readable and in English.

## Read only for your task

Choose the matching task below. Batch its skill and required documents in the first tool interaction; do not read a skill just to discover already-listed paths. Skip anything already loaded. Reading needs no index build or CLI setup.

For a fact lookup, open the relevant document directly and skip the workflow skill. For multi-step work, use the named skill; links to deeper references are conditional, not a request to read every linked file. Hosts without skill discovery can read `SKILL.md` as ordinary Markdown. The [human task index](docs/README.md) covers the remaining topics.

| Task | First read together |
|---|---|
| Check ordering, panic or memory guarantees | [Semantics](conformance/semantics.md) |
| Change the Rust engine, keys or block partition | [Engine skill](.agents/skills/tessera-engine/SKILL.md), [engine reference](docs/design/engine.md), [semantics](conformance/semantics.md) |
| Measure a change or investigate a performance alert | [Measurement skill](.agents/skills/tessera-measure/SKILL.md), [protocol](docs/development/benchmarking.md); include relevant [engine evidence](docs/design/port.md) for routing, thresholds, digit planning or inlining |
| Read existing PR performance feedback | [Read-only feedback skill](.agents/skills/tessera-pr-performance/SKILL.md); no other document required for the normal path |
| Benchmark tooling or figures | [Tooling boundaries](docs/design/benchmark-tooling.md); include [figure guide](docs/development/benchmark-figures.md) for rendering changes |
| Documentation or site | [Documentation rules](CONTRIBUTING.md#documentation); include [site guide](docs/development/docs-site.md) for pages or models |
| CI, release or planned APIs | [CI](docs/development/ci.md), [release](RELEASING.md) or [roadmap](ROADMAP.md), respectively |
| Open a PR or commit | [Contributing](CONTRIBUTING.md) |

Engine routing, thresholds, digit planning and inlining changes also need the measurement skill, protocol and relevant evidence in that first batch.

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

Run applicable checks locally. Miri, MSRV, platform and benchmark checks also run in CI; [CI](docs/development/ci.md) explains coverage. A green tracking workflow is not a benchmark verdict: read both current-head benchmark checks.
