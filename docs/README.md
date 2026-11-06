# Documentation

The API reference is the crate documentation (`cargo doc --open`, or docs.rs once the crate is published). The ordering rules shared by every language are in [`conformance/semantics.md`](../conformance/semantics.md).

| Directory | For | Contents |
|---|---|---|
| [`guide/`](guide/) | Library users | [Reusing scratch memory with `Workspace`](guide/workspace.md) |
| [`design/`](design/) | Readers of the engine source | [Engine design](design/engine.md) and the [port from the research prototype](design/port.md) |
| [`development/`](development/) | Contributors | [Benchmark protocol](development/benchmarking.md) and [continuous benchmarks](development/continuous-benchmarks.md) |
| [`decisions/`](decisions/README.md) | Anyone asking why | Decision records: context, decision and consequences |
| [`proposals/`](proposals/) | Reviewers of planned work | [Python binding](proposals/python-binding.md) |

Planned work is tracked in [`ROADMAP.md`](../ROADMAP.md), and release rules are in [`RELEASING.md`](../RELEASING.md).

A proposal moves out of `proposals/` when it is implemented: its user-facing part becomes a guide, its internals become a design document, and the choices it settled become decision records.
