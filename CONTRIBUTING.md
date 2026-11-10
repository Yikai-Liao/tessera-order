# Contributing to Tessera

Tessera is pre-release. Bug reports, real-workload experience and measurements on other machines are especially useful. Follow the [code of conduct](CODE_OF_CONDUCT.md); discuss changes larger than a small fix in an issue first.

## Development

Use stable Rust (MSRV in `Cargo.toml`), Node.js 24 and the pnpm version pinned in `package.json`. Run the applicable [local checks](AGENTS.md#commands). [CI](docs/development/ci.md) covers platform, MSRV, Miri and sanitizer checks.

### Refactoring

A refactor serves maintainability, readability and lower change cost: fewer duplicated rules and smaller change surfaces, not more files, layers or abstractions. It keeps algorithm logic, generated inputs and the benchmark protocol unchanged; timed regions call the same library functions with the same record types and key extractors, with no added indirection. Ship behavior or performance changes in separate PRs. Moving or splitting engine code can change inlining and code placement, so it follows the measurement rule for [engine changes](#engine-changes).

### Engine changes

Read the [engine reference](docs/design/engine.md) and [shared semantics](conformance/semantics.md) together. Routing, thresholds, digit planning and inlining also require the [measurement procedure](docs/development/benchmarking.md#run-an-experiment), its [source-attribution controls](docs/development/benchmarking.md#additional-source-attribution-controls) and relevant [engine evidence](docs/design/port.md) before implementation.

1. Identify the affected stage and invariant in the reference, source and tests. Reproduce a bug before changing its implementation.
2. Make the smallest complete change. Follow ownership through moves, panic recovery and inconsistent keys; preserve exactly one live owner. Explain each `unsafe` block with a `// SAFETY:` invariant and document `# Safety` on every `unsafe fn`.
3. Add focused regression coverage at the changed boundary. Preserve panic, inconsistent-key, owned-record and `&mut`-record tests; extend them where affected rather than weakening them.
4. Update semantics for intentional contract changes. Algorithm changes also update the explanation and the models in `docs/.vitepress/theme/sim/`.

### Validation

Keep tests focused on public contracts, unsafe ownership invariants, historical defects and external trust or recovery boundaries. Prefer one end-to-end check plus focused boundary cases to repeated assertions at every layer. Vary independent dimensions separately; combine them where their interaction can fail. Do not pin internal call counts, exact report wording, SVG snapshots or incidental layout. Keep ordering, stability, panic, inconsistent-key, owned-record and `&mut`-record coverage.

Run the applicable [local checks](AGENTS.md#commands), including workspace tests and doctests. Engine changes also exercise both feature configurations and affected tests under Miri:

```sh
cargo test -p tessera-sort --no-default-features
cargo test -p tessera-sort --features portable-simd
rustup +nightly component add miri
cargo +nightly miri test -p tessera-sort
cargo +nightly miri test -p tessera-sort --no-default-features
```

Run `pnpm docs:test` when simulation models change and the [site checks](docs/development/docs-site.md#commands) when their pages change. CI owns the remaining platform, MSRV and sanitizer lanes; the [Rust workflow](.github/workflows/rust-ci.yml) is the command source for reproducing a lane. It uses cargo-nextest for test processes and runs doctests separately.

Report the invariant preserved or contract changed, regression coverage, commands actually run and anything left to CI or unavailable locally. For performance choices, include paired evidence with A/A ranges and investigate results outside the controls under the protocol below.

## Performance changes

Use the maintained benchmark CLI and the [measurement procedure](docs/development/benchmarking.md#command-capabilities). The protocol defines paired timing, A/A ranges and the extra controls needed to attribute a source-change gain rather than compare algorithms. Include measured coverage, environment, control ranges and a results link; investigate changes outside the controls. Process-level screens and continuous alerts alone do not isolate source changes from code placement.

When a measured choice keeps a separate copy of engine code, for example to preserve serial inlining contexts, its comment names the evidence, tests constrain the invariants both copies share, and the roadmap states what evidence would allow merging them.

## Documentation

Use the [task index](docs/README.md) to find the current topic. Edit an existing topic before adding a document; a new page needs a distinct contract, procedure, explanation or evidence. Delete obsolete instructions, keep current rules and non-obvious rationale together, and retain reproducible measurement conditions. Git/PR history preserves superseded decisions; do not add a parallel decision record. Markdown paragraphs occupy one line.

Keep a task's procedure and completion criteria in its existing topic, with the normal path before implementation details and exceptional branches. `AGENTS.md` lists the required first-read sections for agents; the [task index](docs/README.md) and site link to the same sources. Link directly to the relevant section, keep conditional references conditional, and let fact lookups skip procedures. Check representative tasks for unnecessary reading and discovery steps, not just document counts. Run the [site checks](docs/development/docs-site.md#commands) after changing pages or shared documents.

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

Scope is optional: `engine`, `block`, `key`, `simd`, `site`, `bench`, `fuzz` or `deps`; new language packages add their name. Use one type per PR. The imperative English title starts lowercase unless it starts with a name, has no final period, and is at most 64 characters. Breaking changes add `!` and a `BREAKING CHANGE:` paragraph describing migration. Type/scope changes update both these rules and [the checker](.github/scripts/check_pr_title.ts).

Branches need not be up to date with `main`; rebase only for conflicts. Describe user-visible changes in the PR: [release notes](RELEASING.md#release-checklist) are written from merged PRs, without a shared changelog.

## Sign-off (DCO)

Every commit needs an author-matching `Signed-off-by` line certifying the [DCO 1.1](https://developercertificate.org/). Use the configured identity:

```sh
git commit -s
# Add missing sign-offs to an existing branch:
git rebase --signoff origin/main
git push --force-with-lease
```

Only the current Dependabot head can be exempt: the authenticated bot must open/update a same-repository PR, and GitHub must verify its signature and bot attribution. Other commits, including human additions to bot PRs, still need matching sign-offs. Missing provenance or API failures grant no exception. The [verification helper](.github/scripts/verify_dependabot.ts) runs from the target branch without executing PR contents. Ask Dependabot to rebase human-updated branches rather than rewriting its signature.

## License

Contributions use [BSD-2-Clause](LICENSE). New source files start with the language-appropriate `SPDX-License-Identifier: BSD-2-Clause` comment; `block.rs` retains Clausecker's complete notice. Do not copy third-party code, including Apache-2.0 harness/tests from `sort-research-rs`, into this repository. Rust competitors are dependencies; native sources are fetched at pinned revisions into ignored directories, with only our adapters committed. Benchmarks embedding upstream projects live outside this repository.
