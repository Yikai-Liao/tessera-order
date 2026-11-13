# Documentation

Every fact has one owner. Other pages link to it and never restate its numbers or rules, so a change touches its owner and the code, not a chain of pages. Pick your task below; API contracts also live in Rust doctests (`cargo doc --open`).

## Start here

| Task | Read |
|---|---|
| Use the library | [Getting started](guide/getting-started.md) |
| Check ordering, panic or memory guarantees | [Shared semantics](../conformance/semantics.md) |
| Learn the algorithm | [Explanation](explain/index.md), with interactive figures |
| Change the engine | [Engine reference](design/engine.md), then the [explanation page](explain/index.md#reading-order) for the stage you change; [validation](../CONTRIBUTING.md#engine-changes) |
| Measure a change or compare algorithms | [Benchmark protocol](development/benchmarking.md#run-an-experiment); [engine evidence](design/evidence.md) before repeating an experiment |
| Read PR or main performance feedback | [PR feedback](development/continuous-benchmarks.md#read-feedback-from-a-pr) |
| Change benchmark tooling or figures | [Tooling design](design/benchmark-tooling.md), [figure guide](development/benchmark-figures.md) |
| Change CI, the site or a release | [CI](development/ci.md), [site guide](development/docs-site.md), [releases](../RELEASING.md) |
| Contribute | [Contributing](../CONTRIBUTING.md) |
| Plan future work | [Roadmap](../ROADMAP.md), [algorithm extensions proposal](design/algorithm-extensions-proposal.md), [Python binding design](design/python-binding.md) (both unimplemented) |

## Who owns what

| Fact | Owner | Update it when |
|---|---|---|
| Ordering, stability, panic and memory guarantees | [semantics](../conformance/semantics.md) | The public contract changes |
| Routing rules, thresholds, digit planning, block size and the reasons for them | The [explanation page](explain/index.md#reading-order) of that stage, with its model in `docs/.vitepress/theme/sim/` | An algorithm rule or constant changes |
| Code boundaries, invariants, ownership and recovery, parallel completion | [engine reference](design/engine.md) | A boundary or invariant changes, not a threshold |
| Failed experiments, open measured losses, placement lessons, structure analysis | [engine evidence](design/evidence.md) | An experiment is rejected, an accepted change keeps a loss, or a structural conclusion or its premise changes |
| Measurements of an accepted change | Its PR description; a retained mechanism names the saving that justifies it at its owner | The change merges |
| Unfinished work and its acceptance | [roadmap](../ROADMAP.md) | Work starts, lands or is rejected |
| Unimplemented algorithm interfaces and dependency migration choices | [algorithm extensions proposal](design/algorithm-extensions-proposal.md) | A proposed interface or migration decision changes; move implemented facts to their current owners |
| Measurement commands, statistics, inputs, result formats | [benchmark protocol](development/benchmarking.md) | A command, input or statistic changes |
| Hosted benchmark pipeline, PR reports, alerts | [continuous benchmarks](development/continuous-benchmarks.md) | The hosted pipeline changes |
| Benchmark module ownership and change paths | [tooling design](design/benchmark-tooling.md) | A module boundary moves |
| CI lanes and routing, release procedure | [CI](development/ci.md), [releasing](../RELEASING.md) | A workflow changes |

An algorithm change normally edits the Rust code, its explanation page and that page's model, plus semantics only when a guarantee changes. If a page needs a number it does not own, it links to the owner.
