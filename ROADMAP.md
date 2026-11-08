---
type: Proposal
title: Roadmap
description: Select unfinished work and check milestone acceptance conditions.
tags: [planning, release]
status: draft
---

# Roadmap

This page tracks unfinished work and its acceptance conditions. Current behavior belongs in the [user guide](docs/guide/getting-started.md) and [engine reference](docs/design/engine.md).

## Rust 0.1

The first crates.io release. It provides the stable sort only.

| Deliverable | Done when |
|---|---|
| Comparison runner, Rust and native competitors, memory measurement and offline report ([protocol](docs/development/benchmarking.md)) | Harness self-tests, native smoke validation and report tests pass in CI |
| First release comparison, default and Portable SIMD builds, on Stable | The complete local session regenerates from its raw directory; README and Performance page show the figures; the release attaches the archive |
| Release | Robert Clausecker has been told before publication; `rust/v0.1.0` follows [`RELEASING.md`](RELEASING.md) |

Open questions for this milestone:

- **Sample duration and prepared-memory cap.** Fix the defaults from the test machine's memory and the time a 10M-record case takes.
- **Cost of cached bucket IDs on 8-byte rows.** `{u32 key, u32 row}` records have not been measured. If the cost is large, consider an `unsafe` entry point that skips the cache ([rationale](docs/design/engine.md#record-handling)).

## Python binding

An independent track that binds the Rust crate and does not wait for C++. See the [Python binding proposal](docs/proposals/python-binding.md).

## C++

1. **C++ runner.** Measure the C++ implementation with the shared input specification and native-buffer timing. Add cross-language golden hashes in `conformance/vectors/` and verify them in both generators.
2. **C++17 header-only implementation** for trivially copyable records, with CMake install and a consumer test. Done when it passes the shared vectors and has a direct comparison with Rust.

## Benchmark investigations

- Extend the comparison to workspace and cold-cache modes, additional seeds and threshold cases when an investigation needs them.
- Add native-CPU and branch-aligned placement controls for source A/B studies.
- Add hosted comparison groups using the existing Bencher queue after the local release report; retain complete per-type length groups for absolute scaling.

## Integrations

Rerun the StarRocks and ClickHouse benchmarks against a released crate and link the results from the README. These benchmarks contain upstream Apache-2.0 code, so they live outside this repository ([rationale](CONTRIBUTING.md#license)).

## Not planned

- A dedicated unstable kernel before 0.1.
- vcpkg and Conan packages: after the first releases, on demand.
- A `core`-only build that takes its scratch from the caller instead of `alloc`: only if a real use case without a global allocator appears.
- Go and parallel sorting: only if a real use case appears.
