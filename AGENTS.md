# Working on Tessera

Build a small, reliable ordering engine for high-performance algorithms and ordered containers, with a shared benchmark system for local revision comparisons, PR feedback and comparisons with other sorting implementations. Preserve the existing figures. Prefer fewer concepts, states and independently maintained rules over shorter syntax or more abstraction. Code and documentation are in English.

## Start with the task

| Task | Read |
|---|---|
| Library behavior or engine changes | [Semantics](conformance/semantics.md), [engine reference](docs/design/engine.md) and the [explanation page](docs/explain/index.md#reading-order) of the changed stage, [engine validation](CONTRIBUTING.md#engine-changes) |
| Performance-sensitive engine changes | The engine references above and [measurement procedure](docs/development/benchmarking.md#run-an-experiment); consult [engine evidence](docs/design/evidence.md) for the affected choice |
| Benchmark tooling | [Ownership and boundaries](docs/design/benchmark-tooling.md), then the affected [command contract](docs/development/benchmarking.md#command-capabilities) |
| Figures | [Figure guide](docs/development/benchmark-figures.md) |
| PR feedback or history | [Continuous benchmarks](docs/development/continuous-benchmarks.md) |
| Documentation/site, CI, release | [Site guide](docs/development/docs-site.md), [CI](docs/development/ci.md) or [release](RELEASING.md), as applicable |
| Commit or PR | [Contributing](CONTRIBUTING.md) |

Read the relevant sections as needed. [The task index](docs/README.md) covers other topics.

## Development loop

Use the maintained CLI from the repository root. Its compact output serves agents first; reports and SVGs provide the detailed human view.

- Preview selection: `bun run --silent tools plan NAME`.
- Compare working tree with a commit: `bun run --silent tools compare commit COMMIT`.
- Compare sorting implementations: `bun run --silent tools compare implementations`.
- Regenerate retained results: `bun run --silent tools report DIR`.
- Read feedback: `bun run --silent tools status --pr NUMBER` or `bun run --silent tools status main`. Match the exact head; inspect benchmark verdicts separately from workflow states. PR comparisons run automatically; independent main tracking is opt-in ([contract](docs/development/continuous-benchmarks.md#main-long-term-tracking)).

Start performance work with a small representative selection, then expand evidence to match the claim. Extend the shared pipeline when an experiment needs a missing capability. Keep raw measurements outside Git. Read-only feedback requests stay read-only; confirm host-wide system changes, credentials, publication and unusually expensive experiments with the operator.

## Engine design and simplification

Use the `software-design-philosophy` skill for structural engine changes. Judge maintainability by change amplification, cognitive load and hidden dependencies, not by line or file count.

- **Derive algorithms and containers from a shared representation.** Rust’s `HashSet<K>` illustrates the design preference. With `()` as the value, `HashMap<K, ()>` provides set membership through the same table operations. For each Tessera consumer, first ask which positions it needs ordered and whether it needs source indices or records in place. Keep the public adapter thin and let the engine own the ordering and movement rules. Prefer a representation that can skip work the consumer does not need.
- **Optimize the shared ordering engine.** Trace redundant key reads, classifications or record moves to the primitive that owns them. Choose an improvement other consumers can reuse. A speedup confined to a bespoke implementation of one API does not justify adding it to the engine. Measure complete operations against the standard library and competitive alternatives, and guard full-sort performance when improving another operation.
- **Trace real changes.** Before choosing a boundary, follow tasks such as changing sampling, tile capacity, partition selection, block layout or task size. Each rule has one owner; consumers do not reconstruct it. Use the [engine vocabulary](docs/design/engine.md#vocabulary): subproblems, buckets, tiles, blocks, primitives, cost models and scheduling are distinct.
- **Deep operations, not wrappers.** A boundary hides decisions or owns a complete invariant-preserving operation. Do not add a type, trait or forwarding layer merely to name a concept.
- **One analysis for serial and parallel.** Partition choice, bucket meaning and finishing rules have one authority; parallel code differs only in scheduling and ownership transfer.
- **Generalize, don't delete.** Retire a fast path only when a more general mechanism keeps its savings. Do not discard known ranges, bounds or classifications, and audit every added scan, allocation or pass by its cost to the complete operation. Parallel work may read past a serial early exit when the extra work is bounded and its mechanism documents the bound.
- **Derive constants.** Separate representation limits, budgets and tuned parameters; derive values from existing constraints instead of adding distribution-specific exceptions.
- **Report trade-offs.** A generalization may accept measured local losses if representative coverage improves; list the losses individually. Keep structural and performance changes separately reviewable; PRs are squash-merged, so split them into separate PRs when each needs its own verdict. Record which checks ran and what acceptance is outstanding.

## Quality boundaries

- Preserve ordering, stability, record ownership, panic recovery and the documented memory contract. Every unsafe operation explains its safety invariant. Keep the library's panic, inconsistent-key, owned-record and mutable-reference tests.
- Preserve stable Rust and the MSRV in `Cargo.toml`; agree dependency, MSRV or platform-support changes with the operator. New source files carry BSD-2-Clause SPDX headers; retain Clausecker's notice in `block.rs`. Third-party algorithms remain dependencies or verified external sources.
- Separate structural cleanup from changes to measured algorithms, inputs and sampling. Accept performance-sensitive engine changes and attribute their gains as [performance changes](CONTRIBUTING.md#performance-changes) describes.
- Use domain names and units: records, keys, extractor, `_bytes`, `_bits` and `_len`. Public APIs explain their contract; internal comments explain non-obvious invariants and decisions.
- Keep current rationale in its owning topic. Every fact has one owner in the [ownership table](docs/README.md#who-owns-what): before adding a page or a record, check it, and link to the owner instead of restating its numbers or rules. A conclusion later work depends on (a judgment with its premises, a model, or why a direction was dropped) goes to its owner in the repository; a PR description or conversation is never its only copy. Record conclusions and reasoning, not figures the code determines, such as line counts or derived sizes; raw runs, commands and experiment chronology stay outside Git. Update semantics for contract changes and explanations/simulation models for algorithm changes. Track unfinished work in [ROADMAP.md](ROADMAP.md).
- Every PR receives an independent subagent review before merge. Review affected runtime paths, trust boundaries and shared-rule consumers; resolve accepted findings and re-review material fixes.

## Commands

Run checks for the affected boundary:

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets && cargo test --workspace --doc
bun install --frozen-lockfile && bun run format
bun run tools:test && bun run tools:typecheck
bun run docs:test && bun run docs:typecheck && bun run docs:build
```

Engine feature/Miri checks are in [validation](CONTRIBUTING.md#validation); platform and MSRV lanes run in CI. Report checks actually run and remaining gaps. Use Conventional Commits (64-character title limit), sign off with the configured identity, and include an `Assisted-by:` or `Co-authored-by:` trailer. Describe user-visible and compatibility changes in the PR.
