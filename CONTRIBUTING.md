# Contributing to Tessera

Tessera is pre-release. Bug reports, real-workload experience and measurements on other machines are especially useful. Follow the [code of conduct](CODE_OF_CONDUCT.md); agree the scope of larger changes with the maintainer in an issue or development request.

## Development

Use stable Rust (MSRV in `Cargo.toml`) and the Bun version pinned in `package.json`. Install dependencies with `bun install --frozen-lockfile`; `bun.lock` is the only JavaScript dependency lockfile. Run the applicable [local checks](AGENTS.md#commands). [CI](docs/development/ci.md) covers platform, MSRV, Miri and sanitizer checks.

### Refactoring

A refactor reduces the concepts and coordinated edits needed to maintain the project: retire obsolete representations and their consumers together, keep algorithm logic, generated inputs and the benchmark protocol unchanged, and keep behavior and performance changes independently reviewable. Pre-release tooling interfaces and result formats may change when that removes real complexity; document the break instead of keeping an unused compatibility layer. Moving or splitting engine code can change inlining and code placement, so it follows the measurement rule for [engine changes](#engine-changes).

### Engine changes

Read the [engine reference](docs/design/engine.md), [shared semantics](conformance/semantics.md) and the [explanation page](docs/explain/index.md#reading-order) of the stage you change. Routing, thresholds, digit planning and inlining also require the [measurement procedure](docs/development/benchmarking.md#run-an-experiment), its [source-attribution controls](docs/development/benchmarking.md#additional-source-attribution-controls) and the relevant [engine evidence](docs/design/evidence.md) before implementation.

1. Identify the affected stage and invariant in the reference, source and tests. Reproduce a bug before changing its implementation.
2. Make the smallest complete change. Follow ownership through moves, panic recovery and inconsistent keys; preserve exactly one live owner. Explain each `unsafe` block with a `// SAFETY:` invariant and document `# Safety` on every `unsafe fn`.
3. Add focused regression coverage at the changed boundary. Preserve panic, inconsistent-key, owned-record and `&mut`-record tests; extend them where affected rather than weakening them.
4. Update semantics for intentional contract changes. A changed rule or constant updates its one [owner](docs/README.md#who-owns-what): the explanation page and its model in `docs/.vitepress/theme/sim/`, unless the figure replays the engine's [trace events](docs/development/docs-site.md#writing-pages).

### Validation

Keep tests focused on public contracts, unsafe ownership invariants, historical defects and external trust or recovery boundaries. Prefer one end-to-end check plus focused boundary cases to repeated assertions at every layer. Vary independent dimensions separately; combine them where their interaction can fail. Do not pin internal call counts, exact report wording, SVG snapshots or incidental layout. Keep ordering, stability, panic, inconsistent-key, owned-record and `&mut`-record coverage.

Miri fixtures check memory validity, aliasing and record ownership with small inputs that still cross the relevant storage, tail, panic and recovery boundaries. Keep exhaustive arithmetic searches and large combinatorial or stress coverage in native tests; use a documented `cfg_attr(miri, ignore = "...")` for tests that add no distinct Miri safety coverage. When reducing a fixture under `cfg!(miri)`, retain the production path and its boundary conditions rather than substituting a different implementation. Native tests retain the full cases, including panic, changing-key, owned-record and mutable-reference coverage. A slow test is a reason to inspect its purpose, not to suppress a timeout or silently drop a safety boundary.

Run the applicable [local checks](AGENTS.md#commands), including workspace tests and doctests. Engine changes exercise serial and Rayon builds and affected tests under Miri:

```sh
cargo test -p tessera-order
cargo test -p tessera-order --features rayon
rustup +nightly component add miri
cargo +nightly miri test -p tessera-order
cargo +nightly miri test -p tessera-order --lib --features rayon block::
```

Run `bun run docs:test` when simulation models, the `trace` hooks or the block partition change, and the [site checks](docs/development/docs-site.md#commands) when their pages change. CI owns the remaining platform, MSRV and sanitizer lanes; the [Rust workflow](.github/workflows/rust-ci.yml) is the command source for reproducing a lane. It uses cargo-nextest for test processes and runs doctests separately.

Report the invariant preserved or contract changed, regression coverage, commands actually run and anything left to CI or unavailable locally. For performance choices, include paired evidence with A/A ranges and investigate results outside the controls under the protocol below.

## Performance changes

Use the maintained benchmark CLI and its [protocol](docs/development/benchmarking.md#command-capabilities), which defines paired timing, A/A ranges and the extra controls needed to attribute a source-change gain rather than compare algorithms. Report measured coverage, environment, control ranges and a results link, and investigate changes outside the controls. Process-level screens and continuous alerts alone do not isolate source changes from code placement.

When a measured choice keeps a separate copy of engine code, for example to preserve serial inlining contexts, its comment names the evidence, tests constrain the invariants both copies share, and the roadmap states what evidence would allow merging them.

## Documentation

[`docs/README.md`](docs/README.md#who-owns-what) maps every fact to one owner; edit the owner, link from everywhere else and never restate its numbers or rules. Add a page only for a distinct contract, procedure, explanation or evidence, and delete obsolete instructions instead of annotating them. Git and PR history keep superseded decisions and the full measurements of accepted changes. The repository keeps every conclusion later work depends on, at its owner: a retained mechanism names the saving that justifies it, [engine evidence](docs/design/evidence.md) keeps failed experiments, open losses, placement lessons and the structure analysis, and the [roadmap](ROADMAP.md) keeps unfinished work. Record conclusions and reasoning, not figures the code determines. Markdown paragraphs occupy one line.

Keep a task's procedure and completion criteria in its topic, normal path first. `AGENTS.md` lists the required first reads for agents; link directly to sections, keep conditional references conditional and let fact lookups skip procedures. Check representative tasks for unnecessary reading, not just document counts, and run the [site checks](docs/development/docs-site.md#commands) after changing pages.

Public Rust items explain their contract with intra-doc links and applicable `# Key function`, `# Panics`, `# Safety`, `# Memory` and `# Examples` sections; examples are asserting doctests. Private items use descriptive names, and comments explain non-obvious invariants, hazards and measured choices.

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
