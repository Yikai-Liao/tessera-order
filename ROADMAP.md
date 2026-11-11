# Roadmap

This page tracks unfinished work and its acceptance conditions. Current behavior belongs in the [user guide](docs/guide/getting-started.md) and [engine reference](docs/design/engine.md).

## Rust 0.1

The first crates.io release. It provides the stable sort only. Use the implemented [comparison runner](docs/development/benchmarking.md#command-capabilities) for the remaining release measurements.

| Deliverable | Done when |
|---|---|
| First release comparison of the always-enabled SIMD backend on Stable | The complete local session regenerates from its raw directory; README and Performance page show the figures; the release attaches the archive |
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
| E2 | Root preparation, node planning, child finishing and exact order confirmation are shared; staged finishing owns its capacity allowance and both entries narrow through one owned half-key extractor | Resolve the [shared-confirmation candidate](docs/design/port.md#shared-order-confirmation-candidate)'s short-input costs, then validate shared paths against the pre-consolidation baseline with A/A and placement controls |
| E3 | Serial and parallel direct-key counting now share a reducible histogram and destination-run construction | Establish serial and Rayon performance against the pre-consolidation SIMD baseline; preserve bitwise representative and Send-only tests |
| E4 | One owned half-key extractor serves serial and parallel roots; the root forwarding wrapper has been removed | Validate shared preparation code generation with the unified backend before performance acceptance |
| E5 | Serial and adopted partitions address one segmented scratch representation; serial passes have one segment | Establish serial and Rayon scatter and compaction performance against the pre-consolidation baseline; serial-only library builds remain a separate configuration |
| E6 | Planning owns strategy estimates; conditional sampled child work replaces the low-fanout override but is performance-unaccepted; tile finishing owns scratch, cached classification and restoration | Resolve the [conditional-work candidate](docs/design/port.md#conditional-tile-work-candidate)'s residual planning cost and regressions after restoring recursive-child estimates, then the [required-SIMD screen](docs/design/port.md#shared-planning-and-required-simd-screen)'s random-input losses; validate with A/A and source-attribution controls |
| E7 | Active measurement uses one backend and revision builds select historical SIMD explicitly; retained dual-build sessions remain readable | Local static artifacts, comparison harness and actual historical SIMD provenance pass validation; validate hosted tracker/artifact deployment before final engine performance acceptance: the trusted default-branch publisher must accept the single SIMD configuration and historical/current harness feature provenance; migration-PR raw artifacts alone do not satisfy publication acceptance |

## C++

1. **C++ runner.** Measure the C++ implementation with the shared input specification and native-buffer timing. Add cross-language golden hashes in `conformance/vectors/` and verify them in both generators.
2. **C++17 header-only implementation** for trivially copyable records, with CMake install and a consumer test. Done when it passes the shared vectors and has a direct comparison with Rust.

## Benchmark investigations

- Validate the hosted paired pipeline on Actions: retain cold/warm-cache full-suite duration, queue wait, paired median intervals, same-source false alerts and held-out detection evidence for every active configuration before claiming hosted stability or tuning the per-case sample budget. Verify Cloud upload, fork PR publication, partial-job reruns and the main first-parent comparison. The fresh-process calibration in PR #96 (Actions run `42904693411`) completed all 70 cases per configuration with no same-source alerts; its four measurement jobs took 346–406 seconds. One calibration does not establish long-term false-alert or detection rates.

- Support intended PR changes to continuous plans or resolved boundary inventories without trusting PR-declared completeness or executing PR code in the publisher. Current publication requires the trusted default-branch inventory; validate changed selections locally until that authority is updated. Keep Rust as the sole case-expansion owner.

- Restore PR trend figures only with an explicit baseline-compatible relative-history contract. Hosted ratios can use different merge bases; do not re-normalize them as absolute latency or a fixed-baseline time series. Retain raw evidence beyond Actions’ 14-day artifacts if longer-term regeneration becomes required.

- Consolidate distribution-sensitive routing into general mechanisms. Child-order reuse and the bucket-order split replace the capped end-run route; evaluate both against current-head Portable SIMD continuous feedback. Investigate remaining near-order, sparse-digit and skewed-key deficits without per-distribution tuning.

- Extend the comparison to cold-cache experiments, additional seeds and threshold cases when an investigation needs them.
- Add native-CPU placement controls for source A/B studies; existing CGU, alignment and engine-order screens remain process-level evidence.
- Validate continuous alignment’s dispersion across hosted CPU models. Revisit multi-layout sampling only with held-out false-alert, detection and measured whole-job cost evidence.
- Add hosted algorithm comparison groups after the local release report; retain complete per-type length groups for absolute scaling and keep them separate from revision-ratio Cloud history.

## Integrations

Rerun the StarRocks and ClickHouse benchmarks against a released crate and link the results from the README. These benchmarks contain upstream Apache-2.0 code, so they live outside this repository ([rationale](CONTRIBUTING.md#license)).

## Not planned

- A dedicated unstable kernel before 0.1.
- vcpkg and Conan packages: after the first releases, on demand.
- A `core`-only build that takes its scratch from the caller instead of `alloc`: only if a real use case without a global allocator appears.
- Go: only if a real use case appears.
