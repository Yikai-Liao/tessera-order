# Working on Tessera

Build a mature, small and reliable sorting library, with a shared benchmark system for local revision comparisons, PR feedback and comparisons with other sorting algorithms. Preserve the existing figures. Prefer fewer concepts, states and independently maintained rules over shorter syntax or more abstraction. Code and documentation are in English.

## Start with the task

| Task | Read |
|---|---|
| Library behavior or engine changes | [Semantics](conformance/semantics.md), [engine reference](docs/design/engine.md), [engine validation](CONTRIBUTING.md#engine-changes) |
| Performance-sensitive engine changes | The engine references above and [measurement procedure](docs/development/benchmarking.md#run-an-experiment); consult [engine evidence](docs/design/port.md) for the affected choice |
| Benchmark tooling | [Ownership and boundaries](docs/design/benchmark-tooling.md), then the affected [command contract](docs/development/benchmarking.md#command-capabilities) |
| Figures | [Figure guide](docs/development/benchmark-figures.md) |
| PR feedback or history | [Continuous benchmarks](docs/development/continuous-benchmarks.md) |
| Documentation/site, CI, release | [Site guide](docs/development/docs-site.md), [CI](docs/development/ci.md) or [release](RELEASING.md), as applicable |
| Commit or PR | [Contributing](CONTRIBUTING.md) |

Read the relevant sections as needed. [The task index](docs/README.md) covers other topics.

## Development loop

Use the maintained CLI from the repository root. Its compact output serves agents first; reports and SVGs provide the detailed human view.

- Preview selection: `pnpm -s tools plan NAME`.
- Compare working tree with a commit: `pnpm -s tools compare commit COMMIT`.
- Compare sorting algorithms: `pnpm -s tools compare algorithms`.
- Regenerate retained results: `pnpm -s tools report DIR`.
- Read feedback: `pnpm -s tools status --pr NUMBER` or `pnpm -s tools status main`. Match the exact head; inspect benchmark verdicts separately from workflow states. Automatic measurements run on PRs, not main.

Start performance work with a small representative selection, then expand evidence to match the claim. Extend the shared pipeline when an experiment needs a missing capability. Keep raw measurements outside Git. Read-only feedback requests stay read-only; confirm host-wide system changes, credentials, publication and unusually expensive experiments with the operator.

## Quality boundaries

- Preserve ordering, stability, record ownership, panic recovery and the documented memory contract. Every unsafe operation explains its safety invariant. Keep the library's panic, inconsistent-key, owned-record and mutable-reference tests.
- Preserve stable Rust and the MSRV in `Cargo.toml`; agree dependency, MSRV or platform-support changes with the operator. New source files carry BSD-2-Clause SPDX headers; retain Clausecker's notice in `block.rs`. Third-party algorithms remain dependencies or verified external sources.
- Separate structural cleanup from changes to measured algorithms, inputs and sampling. Verify performance-sensitive engine changes using the [measurement contract](docs/development/benchmarking.md#additional-source-attribution-controls).
- Use domain names and units: records, keys, extractor, `_bytes`, `_bits` and `_len`. Public APIs explain their contract; internal comments explain non-obvious invariants and decisions.
- Keep current rationale in its owning topic. Update semantics for contract changes and explanations/simulation models for algorithm changes. Track unfinished work in [ROADMAP.md](ROADMAP.md).
- Every PR receives an independent subagent review before merge. Review affected runtime paths, trust boundaries and shared-rule consumers; resolve accepted findings and re-review material fixes.

## Commands

Run checks for the affected boundary:

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets && cargo test --workspace --doc
pnpm install --frozen-lockfile && pnpm format
pnpm tools:test && pnpm tools:typecheck
pnpm docs:test && pnpm docs:typecheck && pnpm docs:build
```

Engine feature/Miri checks are in [validation](CONTRIBUTING.md#validation); platform and MSRV lanes run in CI. Report checks actually run and remaining gaps. Use Conventional Commits (64-character title limit), sign off with the configured identity, and include an `Assisted-by:` or `Co-authored-by:` trailer. Describe user-visible and compatibility changes in the PR.
