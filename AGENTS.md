# Instructions for coding agents

For AI coding agents; humans read [`CONTRIBUTING.md`](CONTRIBUTING.md), and these rules follow it. `CLAUDE.md` links here.

Tessera is a stable radix sort with O(√n) auxiliary memory, built as a mature open-source project, not an experiment. Code is concise, readable and easy for a newcomer to follow. Documentation and comments follow each language's conventions at the standard of NumPy's reference documentation. Leave every file you touch cleaner than you found it. Everything is in English.

## Layout

| Path | Contents |
|---|---|
| `rust/tessera-sort/` | The crate: `lib.rs` (API), `key.rs` (encoding), `engine.rs` (routing, tiles), `block.rs` (block partition) |
| `conformance/semantics.md` | Ordering rules shared by every language |
| `docs/` | Guides, explanation (`explain/`), design, development, decisions, proposals; [index](docs/README.md) |
| `docs/.vitepress/` | Documentation site; TypeScript models of the engine in `theme/sim/` |

## Commands

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets && cargo test --workspace --doc
npm ci && npm run format && uvx ruff@0.17.4 format && uvx ruff@0.17.4 check
npm run docs:test && npm run docs:typecheck && npm run docs:build   # site changes
python3 -m unittest discover -s .github/scripts -p "test_*.py"     # CI routing changes
```

Run these locally. Miri, the MSRV and benchmarks run in CI on every pull request, together with one test cell per operating system; `main` also runs the remaining platforms and their MSRV ([details](docs/development/ci.md)). Push commits and read the results there.

## Code

- **Simple first.** Write the smallest change that reads clearly. Delete dead code and helpers or aliases that add a jump without a concept; extract a function when it names a complete stage.
- **Stable Rust**, no default runtime dependencies, optional Rayon parallel support, MSRV from `Cargo.toml`.
- **License.** New files start with an `SPDX-License-Identifier: BSD-2-Clause` comment. Never copy code from other projects, including `sort-research-rs` ([decision 0001a](docs/decisions/0001a-single-bsd-2-clause-license.md)); `block.rs` keeps Robert Clausecker's notice.
- **Unsafe.** A `// SAFETY:` comment above every `unsafe` block, a `# Safety` section on every `unsafe fn`. Keep the panic, inconsistent-key, owned-record and `&mut`-record tests passing.
- **Engine decisions.** Routing, thresholds, digit planning and inlining change only with measurements ([port record](docs/design/port.md)).
- **Tools own format and lints**: rustfmt, Biome, Ruff, configured in `Cargo.toml`, `biome.json`, `ruff.toml`. Fix the cause; suppress only at the site, with the reason.

## Naming

- One concept, one name, across code, docs and models, from the [engine vocabulary](docs/design/engine.md): record, key, digit, bucket, node, tile, block, cursor.
- Names say what a value is: `records`, `keys`, `extractor`, `len`; never `arr`, `tmp` or `data`. Units go in the name: `_bytes`, `_bits`, `_len`.
- Functions are verbs; `try_` may decline, `is_`/`has_` return `bool`. Types are nouns, unique across modules.
- Test names state the behavior in two to four words (`scatter_panic_keeps_records`); details go in a comment.

## Documentation

- Public Rust items: a one-line summary, a paragraph on the contract with intra-doc links, then `# Key function`, `# Panics`, `# Safety`, `# Memory`, `# Examples` as they apply, with examples as asserting doctests. Internal items get at least the summary.
- Comments explain why — the invariant, the hazard, the measurement behind a constant with its numbers — for a reader of the current code. History belongs in `docs/design/port.md` or a decision record.
- Markdown paragraphs are one line each.
- Update in the same change: `docs/explain/` and `theme/sim/` for routing, thresholds, digit planning or the block partition (their text says all a figure shows); `conformance/semantics.md` for ordering, stability, panics or space bounds; a decision record, numbered by its pull request, for a settled choice; `ROADMAP.md` for planned work; the pull request description for user-visible changes, which release notes are written from.

## Measurements

Never call a change faster or slower without measurements per [`docs/development/benchmarking.md`](docs/development/benchmarking.md), with the A/A range. A slowdown outside the controls is investigated, not assumed to be noise. Raw data, profiles and the research harness stay out of the repository.

## Commits

Commits and pull request titles follow [Conventional Commits](CONTRIBUTING.md#commit-messages): `type(scope)!: summary`, at most 64 characters; CI checks the title, which becomes the squash commit. Sign off (`git commit -s`) with the configured identity and add an `Assisted-by:` or `Co-authored-by:` trailer.
