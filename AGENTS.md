# Instructions for coding agents

This file is for AI coding agents. Human contributors should read [`CONTRIBUTING.md`](CONTRIBUTING.md); these rules follow it. `CLAUDE.md` is a link to this file.

## Project

Tessera is a stable radix sort with O(√n) auxiliary memory. Everything in the repository is written in English.

| Path | Contents |
|---|---|
| `rust/tessera-sort/` | The Rust crate: `lib.rs` (public API), `key.rs` (key encoding), `workspace.rs`, `engine.rs` (routing and tiles), `block.rs` (block partition) |
| `conformance/semantics.md` | Ordering rules shared by every language |
| `docs/` | Guides, the algorithm explanation (`explain/`), design, development process, decision records and proposals; index in [`docs/README.md`](docs/README.md) |
| `docs/.vitepress/` | Documentation site: VitePress config, Vue figures, and TypeScript models of the engine in `theme/sim/` ([guide](docs/development/docs-site.md)) |
| `ROADMAP.md` | Planned work, completion criteria and open questions |
| `RELEASING.md` | Versions, tags, compatibility and CI/CD layout |
| `.github/` | Per-language CI and release workflows; `scripts/ci_changes.py` selects languages from changed paths |

## Commands

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test --workspace --doc
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo +1.85 test --workspace                # MSRV
cargo +nightly miri test -p tessera-sort    # after any engine, block or key change
python3 -m unittest discover -s .github/scripts -p "test_*.py"   # after changing CI routing
npm ci && npm run docs:test && npm run docs:typecheck && npm run docs:build   # after changing docs or the site
```

## Code rules

- **Stable Rust.** The crate builds with the `rust-version` in `Cargo.toml`, without nightly features. Default and `no_std` builds have no runtime dependencies; the optional `rayon` feature adds parallel support.
- **License.** Every new source file starts with `// SPDX-License-Identifier: BSD-2-Clause`. Do not copy code from other projects, including `sort-research-rs` test tools and benchmark harness ([decision 0001](docs/decisions/0001-single-bsd-2-clause-license.md)). `block.rs` keeps Robert Clausecker's notice.
- **Unsafe code.** Every `unsafe` block has a `// SAFETY:` comment directly above it, and every `unsafe fn` has a `# Safety` section. Keep the tests for panicking key functions, inconsistent keys, owned records and `&mut` records passing.
- **Engine decisions.** The engine reproduces the research prototype's decisions ([port record](docs/design/port.md)). Do not change routing, thresholds, digit planning or inlining without measurements.
- **Formatting and lints.** `rustfmt.toml` uses stable options only. Lints are configured in the workspace `Cargo.toml`; fix the cause instead of adding `allow`.

## Documentation rules

Public items are documented to the standard of NumPy's reference documentation, written in rustdoc's conventions:

1. A one-line summary in the imperative or descriptive form (`Sorts keys in ascending order, ...`).
2. A paragraph on behavior and contract: ordering, stability, what happens to records, related items as intra-doc links.
3. Sections, in this order, when they apply: `# Key function`, `# Panics`, `# Safety`, `# Memory`, `# Examples`.
4. Examples are complete, compiled doctests with assertions, not fragments.

Internal comments explain why, not what: the invariant an operation relies on, the hazard a line avoids, or the measurement behind a constant (with its numbers). Write them for a reader of the current code. Do not narrate history, requests or reviews ("restored", "as in the original", "at the user's request"); such history belongs in `docs/design/port.md` or a decision record.

Put new documentation where [`docs/README.md`](docs/README.md) says. The pages in `docs/explain/` and their models in `docs/.vitepress/theme/sim/` follow the engine: a change to routing, thresholds, digit planning or the block partition updates them in the same change. Their text must say everything a figure shows, because agents read the Markdown without the figures. A change to ordering, stability, panic behavior or space bounds updates `conformance/semantics.md` in the same change. A settled design choice gets a new decision record; planned work goes in `ROADMAP.md`.

## Measurements

Do not say that a change is faster or slower without measurements that follow [`docs/development/benchmarking.md`](docs/development/benchmarking.md), and report the A/A range. A slowdown outside the controls is investigated, not assumed to be noise. Raw benchmark data, profiles and the research harness stay out of the repository.

## Commits

Write commit messages in English, with a summary line that says what changed. Sign off every commit (`git commit -s`) with the configured Git identity. Disclose your assistance with an `Assisted-by:` or `Co-authored-by:` trailer.
