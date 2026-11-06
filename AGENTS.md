# Instructions for coding agents

This file is for AI coding agents working in this repository. Human contributors should read [`CONTRIBUTING.md`](CONTRIBUTING.md), which these rules follow.

## Project

Tessera is a stable radix sort with O(√n) auxiliary memory. The Rust crate is in `rust/tessera-sort/`. Ordering rules are in `conformance/semantics.md`, the algorithm in `docs/design.md`, and the benchmark protocol in `docs/benchmarking.md`. The repository is written in English.

## Commands

```sh
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets
cargo fmt --all
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo +nightly miri test -p tessera-sort   # engine changes
```

## Rules

- **Stable Rust.** The crate must build with the `rust-version` in `Cargo.toml`. Do not add nightly features or runtime dependencies.
- **License.** Every new source file starts with `// SPDX-License-Identifier: BSD-2-Clause`. Do not copy code from other projects or vendor third-party sources. Third-party code enters only as published crates used by `bench/`. The block partition in `block.rs` keeps Robert Clausecker's copyright notice.
- **Unsafe code.** Every `unsafe` block has a `SAFETY:` comment stating the invariant it relies on. Keep the tests for panicking key functions, non-`Copy` records and `&mut` records passing.
- **Semantics.** A change to ordering, stability, panic behavior or space bounds updates `conformance/semantics.md` in the same change.
- **Performance.** Do not state that a change is faster or slower without measurements that follow `docs/benchmarking.md`. Report the A/A range with any result.
- **Commits.** Do not add `Signed-off-by`; the human author adds it after reviewing the change. Disclose your assistance with an `Assisted-by:` or `Co-authored-by:` trailer.
