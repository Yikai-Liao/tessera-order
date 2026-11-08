# Documentation

Choose a task, then open only its topic. Documents carry OKF 0.2 metadata; [the knowledge workflow](development/knowledge.md) builds a disposable CLI bundle with generated per-directory indexes. Markdown remains the only maintained content. API contracts also live in Rust doctests (`cargo doc --open`).

| Task | Start here |
|---|---|
| Use the library | [Getting started](guide/getting-started.md), [workspace reuse](guide/workspace.md) |
| Check ordering, panic or memory guarantees | [Shared semantics](../conformance/semantics.md) |
| Change the engine | [Engine reference](design/engine.md); [measurement evidence](design/port.md) for performance choices |
| Learn the algorithm | [Explanation and reading order](explain/overview.md), with interactive figures |
| Measure or investigate performance | [Benchmark protocol](development/benchmarking.md) |
| Read current PR feedback or maintain tracking | [Continuous benchmarks](development/continuous-benchmarks.md) |
| Change benchmark tooling | [Architecture](design/benchmark-tooling.md), [comparison harness](design/comparison-benchmarks.md), [figures](development/benchmark-figures.md) |
| Maintain documentation or retrieval | [Knowledge workflow](development/knowledge.md), [site](development/docs-site.md) |
| Contribute, change CI or release | [Contributing](../CONTRIBUTING.md), [CI](development/ci.md), [releases](../RELEASING.md) |
| Work on future capabilities | [Roadmap](../ROADMAP.md), [Python proposal](proposals/python-binding.md) (not implemented) |

Current rules and their rationale live together in these topics. Superseded decisions remain in Git history, not the active reading path.
