# Contributing to Tessera

Thank you for your interest. Tessera is in pre-release, and the most useful contributions right now are bug reports, benchmark results from other machines, and reports from real workloads.

## Before you start

For anything larger than a small fix, open an issue first. Changes to the engine affect memory safety and performance at the same time, so it helps to agree on the approach before code is written.

## Development

The crate builds on stable Rust; the minimum supported version is in `Cargo.toml`.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all
```

Engine changes also need Miri, which requires nightly:

```sh
rustup +nightly component add miri
cargo +nightly miri test -p tessera-sort
```

## Unsafe code

The engine moves records through uninitialized scratch memory. Every `unsafe` block states the invariant it relies on in a `SAFETY:` comment. A change that adds or alters unsafe code must explain why the invariant still holds, and must keep the tests for panicking key functions, non-`Copy` records and records holding `&mut` references passing.

## Performance changes

A claim that a change is faster or slower needs measurements that follow [`docs/benchmarking.md`](docs/benchmarking.md): paired with the previous version in one process, with the A/A range reported. Please include the raw output or a link to it.

## Semantics

Changes to ordering, stability, panic behavior or space bounds must update [`conformance/semantics.md`](conformance/semantics.md) in the same pull request.

## License

By contributing, you agree that your contributions are licensed under the [BSD 2-Clause License](LICENSE). New source files start with `// SPDX-License-Identifier: BSD-2-Clause`.
