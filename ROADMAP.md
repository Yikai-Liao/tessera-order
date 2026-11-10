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
| E1 | Owned `Narrow` and borrowed `KeyHalf` repeat window shifting | Share the representation only with preserved window invariants and paired source/placement evidence |
| E2 | Parallel preparation repeats serial tile geometry, probes and half dispatch | Consolidate only if separate serial inlining contexts retain their measured behavior |
| E3 | Parallel direct-key counting repeats serial counting and fill | Constrain shared semantics and establish both configurations' performance before merging mechanisms |
| E4 | Window operations require forwarding through `KeyHalf`, `Narrow` and `RootExtractor` | Reduce forwarding obligations without losing bit-offset or direct/user-extractor distinctions; validate code generation |
| E5 | Serial block storage contains Rayon scratch-segment state and a branch | Decide layout and tracking protocol explicitly: both continuous builds enable Rayon; the default feature set is a separate configuration |
| E6 | `engine.rs` combines routing, sampling, planning, node/tile sorting and scratch | Split along mechanism/invariant boundaries only with inlining and placement controls |

## C++

1. **C++ runner.** Measure the C++ implementation with the shared input specification and native-buffer timing. Add cross-language golden hashes in `conformance/vectors/` and verify them in both generators.
2. **C++17 header-only implementation** for trivially copyable records, with CMake install and a consumer test. Done when it passes the shared vectors and has a direct comparison with Rust.

## Benchmark investigations

- Validate the opt-in baseline-aware queue on hosted Actions without changing coverage or sampling. Retain arrival, queue-wait, measurement-duration and superseded-work evidence before claiming a latency improvement or choosing more measurement capacity. Queue recovery and archival must stay within Actions/API and artifact retention limits; no automatic sample reuse or cross-machine baseline equivalence is assumed.

- Consolidate distribution-sensitive routing into general mechanisms. Child-order reuse and the bucket-order split replace the capped end-run route; evaluate both against current-head Default and Portable SIMD continuous feedback. Investigate remaining near-order, sparse-digit and skewed-key deficits without per-distribution tuning.

- Extend the comparison to cold-cache experiments, additional seeds and threshold cases when an investigation needs them.
- Add native-CPU placement controls for source A/B studies; existing CGU, alignment and engine-order screens remain process-level evidence.
- Validate continuous alignment's dispersion and full-job budget on target Intel hardware. Revisit multi-layout cloud sampling only with held-out false-alert, detection and measured whole-job cost evidence.
- Add hosted comparison groups using the existing Bencher queue after the local release report; retain complete per-type length groups for absolute scaling.

## Integrations

Rerun the StarRocks and ClickHouse benchmarks against a released crate and link the results from the README. These benchmarks contain upstream Apache-2.0 code, so they live outside this repository ([rationale](CONTRIBUTING.md#license)).

## Not planned

- A dedicated unstable kernel before 0.1.
- vcpkg and Conan packages: after the first releases, on demand.
- A `core`-only build that takes its scratch from the caller instead of `alloc`: only if a real use case without a global allocator appears.
- Go: only if a real use case appears.
