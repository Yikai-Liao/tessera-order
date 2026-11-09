# Documentation

Choose a task and read its topic directly. Procedures, contracts and explanations share the same sources for repository readers, agents and the public site. API contracts also live in Rust doctests (`cargo doc --open`).

| Task | Start here |
|---|---|
| Use the library | [Getting started](guide/getting-started.md) |
| Check ordering, panic or memory guarantees | [Shared semantics](../conformance/semantics.md) |
| Change the engine | [Procedure and validation](../CONTRIBUTING.md#engine-changes), [engine reference](design/engine.md), [semantics](../conformance/semantics.md) |
| Learn the algorithm | [Explanation and reading order](explain/index.md), with interactive figures |
| Compare source revisions or algorithms | [Command capabilities and measurement procedure](development/benchmarking.md#command-capabilities); [engine evidence](design/port.md) for performance-sensitive engine decisions |
| Regenerate or interpret a report | [Report command](development/benchmarking.md#command-capabilities), [timing](development/benchmarking.md#timing-and-statistics) and [result contracts](development/benchmarking.md#results-and-provenance) when needed |
| Read current-head PR performance feedback | [PR feedback](development/continuous-benchmarks.md#read-feedback-from-a-pr) |
| Investigate main/history or maintain tracking | [Existing evidence](development/continuous-benchmarks.md#read-main-and-history), tracking internals on the same page |
| Use repository and CI commands | [CLI reference](development/tooling.md) |
| Change benchmark tooling | [Architecture](design/benchmark-tooling.md); [comparison harness](design/comparison-benchmarks.md) or [figures](development/benchmark-figures.md) when relevant |
| Maintain documentation | [Contribution rules](../CONTRIBUTING.md#documentation), [site guide](development/docs-site.md) |
| Contribute, change CI or release | [Contributing](../CONTRIBUTING.md), [CI](development/ci.md), [releases](../RELEASING.md) |
| Work on future capabilities | [Roadmap](../ROADMAP.md), [Python proposal](proposals/python-binding.md) (not implemented), [Rayon proposal](proposals/rayon-parallel-sort.md) (experimental), [maintainability refactor](proposals/maintainability-refactor.md) (proposed) |

Current rules and their rationale live together in these topics. Superseded decisions remain in Git history, not the active reading path.
