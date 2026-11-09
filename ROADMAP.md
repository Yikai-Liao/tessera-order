# Roadmap

This page tracks unfinished work and its acceptance conditions. Current behavior belongs in the [user guide](docs/guide/getting-started.md) and [engine reference](docs/design/engine.md).

## Rust 0.1

The first crates.io release. It provides the stable sort only. Use the implemented [comparison runner](docs/development/benchmarking.md#command-capabilities) for the remaining release measurements.

| Deliverable | Done when |
|---|---|
| First release comparison, default and Portable SIMD builds, on Stable | The complete local session regenerates from its raw directory; README and Performance page show the figures; the release attaches the archive |
| Release | Robert Clausecker has been told before publication; `rust/v0.1.0` follows [`RELEASING.md`](RELEASING.md) |

Open questions for this milestone:

- **Sample duration and prepared-memory cap.** Fix the defaults from the test machine's memory and the time a 10M-record case takes.
- **Cost of cached bucket IDs on 8-byte rows.** Evaluate `{u32 key, u32 row}` results in the first complete release report. If the cost is large, consider an `unsafe` entry point that skips the cache ([rationale](docs/design/engine.md#record-handling)).

## Python binding

An independent track that binds the Rust crate and does not wait for C++. See the [Python binding proposal](docs/proposals/python-binding.md).

## Rayon parallel sorting

The optional feature and bounded completion are implemented; current contracts belong in [semantics](conformance/semantics.md#rust-rayon-entry-points) and the [engine reference](docs/design/engine.md#parallel-root-completion). The [eight-core results](docs/design/port.md#eight-core-parallel-completion) compare whole calls, not isolated source revisions or universal fivefold scaling. Remaining acceptance and refactors require the [measurement protocol](docs/development/benchmarking.md#run-an-experiment), including A/A and source-attribution controls.

| Deliverable | Done when |
|---|---|
| Broader acceptance | Extend measured whole-call scaling and memory coverage to other hosts, wide records and expensive extraction, with source-attribution controls for causal claims |

### Measured engine refactors

These are the engine items E1–E6 deferred from the completed tooling refactor, not permission to change hot paths without measurements. Preserve ownership and panic recovery; keep invariant tests for any copies retained for code placement.

| ID | Debt | Acceptance |
|---|---|---|
| E2 | Parallel preparation repeats serial tile geometry, probes and half dispatch | Consolidate only if separate serial inlining contexts retain their measured behavior |
| E3 | Parallel direct-key counting repeats serial counting and fill | Constrain shared semantics and establish both configurations' performance before merging mechanisms |
| E4 | Half-key views share window operations, but `RootExtractor` retains independent forwarding | Reduce remaining forwarding only without losing backend choices or independent call contexts; validate code generation |
| E5 | Serial block storage contains Rayon scratch-segment state and a branch | Decide layout and tracking protocol explicitly: both continuous builds enable Rayon; the default feature set is a separate configuration |
| E6 | `engine.rs` combines routing, sampling, planning, node/tile sorting and scratch | Split along mechanism/invariant boundaries only with inlining and placement controls |

## C++

1. **C++ runner.** Measure the C++ implementation with the shared input specification and native-buffer timing. Add cross-language golden hashes in `conformance/vectors/` and verify them in both generators.
2. **C++17 header-only implementation** for trivially copyable records, with CMake install and a consumer test. Done when it passes the shared vectors and has a direct comparison with Rust.

## Benchmark investigations

- Consolidate distribution-sensitive routing into general mechanisms. Child-order reuse and the bucket-order split replace the capped end-run route; evaluate both against current-head Default and Portable SIMD continuous feedback. Investigate remaining near-order, sparse-digit and skewed-key deficits without per-distribution tuning.

- Extend the comparison to cold-cache experiments, additional seeds and threshold cases when an investigation needs them.
- Add native-CPU and branch-aligned placement controls for source A/B studies.
- Add hosted comparison groups using the existing Bencher queue after the local release report; retain complete per-type length groups for absolute scaling.

## Integrations

Rerun the StarRocks and ClickHouse benchmarks against a released crate and link the results from the README. These benchmarks contain upstream Apache-2.0 code, so they live outside this repository ([rationale](CONTRIBUTING.md#license)).

## Not planned

- A dedicated unstable kernel before 0.1.
- vcpkg and Conan packages: after the first releases, on demand.
- A `core`-only build that takes its scratch from the caller instead of `alloc`: only if a real use case without a global allocator appears.
- Go: only if a real use case appears.
