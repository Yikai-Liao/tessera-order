---
type: Policy
title: Contributing
description: Prepare a change, validate it, and submit a signed-off pull request.
tags: [ci, docs, release]
---

# Contributing to Tessera

Tessera is pre-release. Bug reports, real-workload experience and measurements on other machines are especially useful. Follow the [code of conduct](CODE_OF_CONDUCT.md); discuss changes larger than a small fix in an issue first.

## Development

Use stable Rust (MSRV in `Cargo.toml`), Node.js 24 and the pnpm version pinned in `package.json`. Run the applicable [local checks](AGENTS.md#commands). [CI](docs/development/ci.md) covers platform, MSRV, Miri and sanitizer checks.

Engine changes must preserve panic, inconsistent-key, owned-record and `&mut`-record tests. Every `unsafe` block explains its invariant in a `// SAFETY:` comment; every `unsafe fn` documents `# Safety`. For local Miri:

```sh
rustup +nightly component add miri
cargo +nightly miri test -p tessera-sort
cargo +nightly miri test -p tessera-sort --no-default-features
```

CI uses cargo-nextest to parallelize test processes and runs doctests separately; its pinned commands are in the [Rust workflow](.github/workflows/rust-ci.yml).

## Performance changes

Use `pnpm bench compare COMMIT` for dirty-tree development screens, `pnpm bench run` for algorithm comparisons and `pnpm bench pr NUMBER --json` for current-head feedback. Read the [benchmark protocol](docs/development/benchmarking.md) before changing routing, thresholds, digit planning or inlining. Performance claims need paired measurements, A/A ranges and placement controls; process-level screens and continuous alerts alone do not establish a speedup. Include results or a link, and investigate changes outside the controls.

## Documentation

Use the [task index](docs/index.md) to find the current topic. [Knowledge maintenance](docs/development/knowledge.md#maintain-the-corpus) defines metadata, ownership and the test for adding a document. Keep rules and rationale together instead of adding a parallel decision record.

Rust items have a one-line summary. Public items explain their contract with intra-doc links and applicable `# Key function`, `# Panics`, `# Safety`, `# Memory` and `# Examples` sections; examples are asserting doctests. Internal comments explain invariants, hazards and measured choices. Update [shared semantics](conformance/semantics.md) with ordering, stability, panic or space changes, and explanations and simulation models with algorithm changes.

## AI-assisted contributions

AI assistance is welcome; a person remains accountable for the whole submission.

- Read and understand every submitted line and be able to explain it in review.
- Disclose tools and their role in the PR template. Tool attribution trailers are welcome; coding agents follow [their instructions](AGENTS.md).
- Verify code with tests and engine changes with Miri. Reproduce bugs before reporting them; never estimate performance results.
- A sign-off certifies the whole change, including generated code and license compatibility.

Untested changes, nonexistent APIs or unrelated edits without evidence of human review may be closed without detailed review.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/): `type(scope)!: summary`. PRs are squash-merged; CI checks the title that becomes the main commit.

| Type | Change |
|---|---|
| `feat` | Public API or capability |
| `fix` | Incorrect behavior |
| `perf` | Measured performance change |
| `refactor` | Structure without behavior changes |
| `docs` | Documentation, comments or site |
| `test` | Tests only |
| `ci` | Workflows or CI scripts |
| `build` | Toolchain, dependencies or configuration |
| `chore`, `revert` | Other maintenance or a revert |

Scope is optional: `engine`, `block`, `key`, `simd`, `site`, `bench`, `fuzz` or `deps`; new language packages add their name. Use one type per PR. The imperative English title starts lowercase unless it starts with a name, has no final period, and is at most 64 characters. Breaking changes add `!` and a `BREAKING CHANGE:` paragraph describing migration. Type/scope changes update both these rules and [the checker](.github/scripts/check_pr_title.py).

Branches need not be up to date with `main`; rebase only for conflicts. Describe user-visible changes in the PR: [release notes](RELEASING.md#release-checklist) are written from merged PRs, without a shared changelog.

## Sign-off (DCO)

Every commit needs an author-matching `Signed-off-by` line certifying the [DCO 1.1](https://developercertificate.org/). Use the configured identity:

```sh
git commit -s
# Add missing sign-offs to an existing branch:
git rebase --signoff origin/main
git push --force-with-lease
```

Only the current Dependabot head can be exempt: the authenticated bot must open/update a same-repository PR, and GitHub must verify its signature and bot attribution. Other commits, including human additions to bot PRs, still need matching sign-offs. Missing provenance or API failures grant no exception. The [verification helper](.github/scripts/verify_dependabot.py) runs from the target branch without executing PR contents. Ask Dependabot to rebase human-updated branches rather than rewriting its signature.

## License

Contributions use [BSD-2-Clause](LICENSE). New source files start with the language-appropriate `SPDX-License-Identifier: BSD-2-Clause` comment; `block.rs` retains Clausecker's complete notice. Do not copy third-party code, including Apache-2.0 harness/tests from `sort-research-rs`, into this repository. Rust competitors are dependencies; native sources are fetched at pinned revisions into ignored directories, with only our adapters committed. Benchmarks embedding upstream projects live outside this repository.
