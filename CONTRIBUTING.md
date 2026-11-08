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

TypeScript checks use the pinned official TypeScript 7.1 beta native compiler. Vue single-file components use the official `@vue/content-mapper` alpha through `contentMappers` and `--runExternalCode`; `vue-tsc` and a legacy TypeScript installation are not required. Node.js 24 executes the tooling's `.ts` files directly; VitePress owns site transformation and bundling.

The documentation site and CI scripts are formatted and linted with [Biome](https://biomejs.dev/) (TypeScript, Vue, CSS, JSON) and [Ruff](https://docs.astral.sh/ruff/) (Python); CI checks both:

```sh
pnpm install --frozen-lockfile && pnpm format
uvx ruff@0.17.4 format && uvx ruff@0.17.4 check
```

Engine changes also need Miri, which requires nightly:

```sh
rustup +nightly component add miri
cargo +nightly miri test -p tessera-sort
```

CI runs the default and `--no-default-features` configurations in parallel. Within each configuration, [cargo-nextest](https://nexte.st/docs/integrations/miri/) runs each test in its own Miri process so tests can use multiple CPU cores. Doctests run separately with `cargo miri test --doc`. To use the same runner locally:

```sh
cargo install --locked cargo-nextest --version 0.9.151
cargo +nightly miri nextest run -p tessera-sort
cargo +nightly miri test -p tessera-sort --doc
cargo +nightly miri nextest run -p tessera-sort --no-default-features
cargo +nightly miri test -p tessera-sort --doc --no-default-features
```

## Unsafe code

The engine moves records through uninitialized scratch memory. Every `unsafe` block states the invariant it relies on in a `SAFETY:` comment. A change that adds or alters unsafe code must explain why the invariant still holds, and must keep the tests for panicking key functions, non-`Copy` records and records holding `&mut` references passing.

## Performance changes

Use `pnpm bench compare COMMIT` for a working-tree development screen, `pnpm bench run` for algorithm comparisons and `pnpm bench pr NUMBER` to read current-head continuous feedback. See the [benchmark guide](docs/development/benchmarking.md). Select coverage with `--plan NAME-OR-FILE` and CLI axis overrides; repository defaults live in `bench/plans/`. Tool dependencies use the pinned pnpm version; register display identities in `bench/catalog.json` and reuse the [shared figure templates](docs/development/benchmark-figures.md).

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

## Commit messages

Commit messages and pull request titles follow [Conventional Commits 1.0](https://www.conventionalcommits.org/en/v1.0.0/). Pull requests are squash-merged, so the title becomes the commit on `main`; CI checks it, and commits inside the branch should follow the same form.

```
type(scope)!: summary

Body: why the change is needed, one line per paragraph.

Signed-off-by: Your Name <you@example.com>
```

| Type | For |
|---|---|
| `feat` | A new public API or user-visible capability |
| `fix` | A correction of wrong behavior |
| `perf` | A performance change, with measurements per [benchmarking](docs/development/benchmarking.md) |
| `refactor` | Code restructured without a change in behavior |
| `docs` | Documentation, comments and the documentation site |
| `test` | Tests only |
| `ci` | Workflows and `.github/scripts` |
| `build` | Cargo, pnpm and tool configuration, such as the MSRV, Biome or Ruff |
| `chore` | Other maintenance, such as releases |
| `revert` | A revert of an earlier commit |

- **Scope** is optional and one of `engine`, `block`, `key`, `simd`, `site`, `bench`, `fuzz` or `deps`; language packages add their own name when they land.
- **Summary** is an imperative English phrase that says what changed, starting in lowercase unless it begins with a name, without a final period, and at most 64 characters so that the squash commit with ` (#123)` stays within 72.
- **Breaking changes** add `!` after the type or scope and explain the migration in a `BREAKING CHANGE:` paragraph.
- **One type per pull request.** A change that needs two types is usually two pull requests.

Branches do not need to be up to date with `main` to merge; rebase only to resolve a conflict ([decision 0032](docs/decisions/0032-parallel-pull-requests.md)).

## Sign-off (DCO)

Every commit in a pull request must carry a `Signed-off-by` line that certifies the [Developer Certificate of Origin 1.1](https://developercertificate.org/): that you wrote the change or otherwise have the right to submit it under the project license.

```sh
git commit -s
```

The name and email in the sign-off must match the commit author. CI checks every commit of a pull request. Dependabot uses a different email in its standard sign-off, so its current head commit is exempt only when the authenticated Dependabot account opened or updated its same-repository pull request and GitHub verifies the commit's signature and bot attribution. All other commits, including human additions to a bot pull request, still need matching sign-offs. Verification failures grant no exception. If a human updates a Dependabot branch, ask Dependabot to rebase it rather than rewriting the bot's sign-off.

To add a missing sign-off to the commits already on your branch:

```sh
git rebase --signoff origin/main
git push --force-with-lease
```

## License

By contributing, you agree that your contributions are licensed under the [BSD 2-Clause License](LICENSE). New source files start with `// SPDX-License-Identifier: BSD-2-Clause`.
