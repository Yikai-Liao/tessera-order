# Documentation

These files are also published as the documentation site, built from this directory ([how the site works](development/docs-site.md)). The API reference is the crate documentation (`cargo doc --open`, or docs.rs once the crate is published). The ordering rules shared by every language are in [`conformance/semantics.md`](../conformance/semantics.md).

| Directory | For | Contents |
|---|---|---|
| [`guide/`](guide/) | Library users | [Getting started](guide/getting-started.md) and [reusing scratch memory with `Workspace`](guide/workspace.md) |
| [`explain/`](explain/index.md) | Anyone learning the algorithm | [How Tessera works](explain/index.md), in reading order, with interactive figures |
| [`design/`](design/) | Readers of the engine source | [Engine design](design/engine.md), the [port from the research prototype](design/port.md), and [comparison benchmarks](design/comparison-benchmarks.md) |
| [`development/`](development/) | Contributors | [Benchmark protocol](development/benchmarking.md), [continuous benchmarks](development/continuous-benchmarks.md) and the [documentation site](development/docs-site.md) |
| [`decisions/`](decisions/README.md) | Anyone asking why | Decision records: context, decision and consequences |
| [`proposals/`](proposals/) | Reviewers of planned work | [Python binding](proposals/python-binding.md) |

Planned work is tracked in [`ROADMAP.md`](../ROADMAP.md), and release rules are in [`RELEASING.md`](../RELEASING.md).

A proposal moves out of `proposals/` when it is implemented: its user-facing part becomes a guide, its internals become a design document, and the choices it settled become decision records.
