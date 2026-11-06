# Contributing to Tessera

Thank you for your interest. Tessera is in pre-release, and the most useful contributions right now are bug reports, benchmark results from other machines, and reports from real workloads.

Everyone taking part in the project follows the [code of conduct](CODE_OF_CONDUCT.md).

## Before you start

For anything larger than a small fix, open an issue first. Changes to the engine affect memory safety and performance at the same time, so it helps to agree on the approach before code is written.

## Development

The crate builds on stable Rust; the minimum supported version is in `Cargo.toml`.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
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

A claim that a change is faster or slower needs measurements that follow [`docs/development/benchmarking.md`](docs/development/benchmarking.md): paired with the previous version in one process, with the A/A range reported. Please include the raw output or a link to it.

## Documentation

Public items document their behavior completely: a one-line summary, the contract (ordering, stability, what happens to records), and `# Panics`, `# Safety`, `# Memory` and `# Examples` sections where they apply, with examples as compiled doctests. Internal comments explain why the code is the way it is: invariants, hazards and the measurements behind constants. [`docs/README.md`](docs/README.md) says where each kind of document belongs.

## Semantics

Changes to ordering, stability, panic behavior or space bounds must update [`conformance/semantics.md`](conformance/semantics.md) in the same pull request.

## AI-assisted contributions

Contributions written with AI tools are welcome. They are held to the same standard as any other contribution, and a person is accountable for each one.

- **You are the author.** Read and understand every line you submit. You should be able to explain the change and answer review questions yourself, without relaying them to a tool.
- **Disclose the tools.** The pull request template asks which tools you used and for what: code, tests, documentation or review. Commit trailers such as `Assisted-by:` or `Co-authored-by:` are welcome but not required.
- **A sign-off covers the whole change.** It certifies the [DCO](#sign-off-dco) for everything in the commit, including that generated code is not copied from code under an incompatible license.
- **Verify, do not estimate.** Run the tests, and Miri for engine changes. Performance numbers must come from measurements, never from a tool's prediction.
- **Reproduce before reporting.** Run the reproduction of a bug or a security issue yourself before filing it, even when a tool found it.

Pull requests that show no sign of human review, such as unrelated edits, calls to APIs that do not exist, or untested code, may be closed without a detailed review.

Coding agents working in this repository should read [`AGENTS.md`](AGENTS.md).

## Sign-off (DCO)

Every commit in a pull request must carry a `Signed-off-by` line that certifies the [Developer Certificate of Origin 1.1](https://developercertificate.org/): that you wrote the change or otherwise have the right to submit it under the project license.

```sh
git commit -s
```

The name and email in the sign-off must match the commit author. CI checks every commit of a pull request. To add a missing sign-off to the commits already on your branch:

```sh
git rebase --signoff origin/main
git push --force-with-lease
```

## License

By contributing, you agree that your contributions are licensed under the [BSD 2-Clause License](LICENSE). New source files start with `// SPDX-License-Identifier: BSD-2-Clause`.
