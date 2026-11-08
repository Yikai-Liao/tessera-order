# Documentation

Choose a task and read its topic directly. Multi-step workflows are also agent skills; their Markdown is shared with the public site, not a separate agent-only manual. API contracts also live in Rust doctests (`cargo doc --open`).

| Task | Start here |
|---|---|
| Use the library | [Getting started](guide/getting-started.md), [workspace reuse](guide/workspace.md) |
| Check ordering, panic or memory guarantees | [Shared semantics](../conformance/semantics.md) |
| Change the engine | [Workflow](../.agents/skills/tessera-engine/SKILL.md), [engine reference](design/engine.md), [semantics](../conformance/semantics.md) |
| Learn the algorithm | [Explanation and reading order](explain/index.md), with interactive figures |
| Measure or investigate performance | [Workflow](../.agents/skills/tessera-measure/SKILL.md), [benchmark protocol](development/benchmarking.md), relevant [measurement evidence](design/port.md) |
| Read existing PR performance feedback | [Read-only workflow](../.agents/skills/tessera-pr-performance/SKILL.md) |
| Maintain benchmark tracking | [Continuous benchmarks](development/continuous-benchmarks.md) |
| Change benchmark tooling | [Architecture](design/benchmark-tooling.md), [comparison harness](design/comparison-benchmarks.md), [figures](development/benchmark-figures.md) |
| Maintain documentation | [Contribution rules](../CONTRIBUTING.md#documentation), [site guide](development/docs-site.md) |
| Contribute, change CI or release | [Contributing](../CONTRIBUTING.md), [CI](development/ci.md), [releases](../RELEASING.md) |
| Work on future capabilities | [Roadmap](../ROADMAP.md), [Python proposal](proposals/python-binding.md) (not implemented) |

Current rules and their rationale live together in these topics. Superseded decisions remain in Git history, not the active reading path.
