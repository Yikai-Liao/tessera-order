# Roadmap

Each milestone lists what it delivers and how we know it is done. Decisions that shaped the plan are recorded in [`docs/decisions/`](docs/decisions/README.md).

## Done

- **Repository setup.** License, ordering semantics, design and benchmark protocol, CI, contribution policy.
- **Stable Rust port.** `stable_sort`, `stable_sort_by_key` and `Workspace<T>` on Rust 1.85, with rewritten tests, Miri and AddressSanitizer.
- **`no_std` with `alloc`.** `default-features = false` builds on `core` and `alloc`. An unwind guard aborts on key-function panics during block partitioning, regardless of the host panic strategy; memory bounds and successful results are unchanged ([decision 0005a](docs/decisions/0005a-no-std-with-alloc.md), [decision 0005b](docs/decisions/0005b-no-std-unwind-guard.md)).
- **Multi-platform CI.** Stable and MSRV on Linux (x86-64, AArch64), macOS (AArch64) and Windows (x86-64), plus Miri and AddressSanitizer; `main` requires it to pass.
- **Optional SIMD backend.** The `portable-simd` feature vectorizes the insertion and ascending-order scans and the counting-pass prefix sums ([decision 0008a](docs/decisions/0008a-optional-simd-backend.md)).
- **Continuous benchmarks.** Default and `portable-simd` builds on Bencher Bare Metal, with reports on every pull request ([setup](docs/development/continuous-benchmarks.md)).
- **Fuzzing and documentation site.** A `cargo-fuzz` target, and a VitePress site with the algorithm explanation.
- **Port acceptance.** Measured against the research prototype in one binary with A/A and code-placement controls. Median ratios are 0.98–1.00; the slowest cases, rerun at higher precision, come from the planned bucket-ID caching or from code placement, which a rebuild reverses and branch alignment removes on ordered inputs. See the [port record](docs/design/port.md).

## Rust 0.1

The first crates.io release. It provides the stable sort only.

| Deliverable | Done when |
|---|---|
| Benchmark runner in `bench/rust` with `list`, `validate`, `measure` and `memory` ([protocol](docs/development/benchmarking.md)) | The harness self-tests and `validate --suite smoke` pass in CI |
| Input generator specification and golden hashes in `conformance/vectors/` | The runner verifies every `core` input hash at startup |
| First `core` report, `default` and `native` builds, on the Stable toolchain | The report regenerates from raw data; it becomes the regression baseline |
| Release | Robert Clausecker has been told before publication; `rust/v0.1.0` follows [`RELEASING.md`](RELEASING.md) |

Open questions for this milestone:

- **Sample duration and prepared-memory cap.** Fix the defaults from the test machine's memory and the time a 10M-record case takes.
- **Cost of cached bucket IDs on 8-byte rows.** `{u32 key, u32 row}` records have not been measured. If the cost is large, consider an `unsafe` entry point that skips the cache ([decision 0001b](docs/decisions/0001b-cache-bucket-ids-for-every-record.md)).

## Python binding

An independent track that binds the Rust crate and does not wait for C++. See the [Python binding proposal](docs/proposals/python-binding.md).

## C++

1. **C++ runner.** The same case specification and JSONL output as the Rust runner, with the standard library and pinned third-party sorts as baselines. Done when its generator matches the golden hashes.
2. **C++17 header-only implementation** for trivially copyable records, with CMake install and a consumer test. Done when it passes the shared vectors and has a direct comparison with Rust.

## Integrations

Rerun the StarRocks and ClickHouse benchmarks against a released crate and link the results from the README. These benchmarks contain upstream Apache-2.0 code, so they live outside this repository ([decision 0001a](docs/decisions/0001a-single-bsd-2-clause-license.md)).

## Not planned

- A dedicated unstable kernel before 0.1.
- vcpkg and Conan packages: after the first releases, on demand.
- A `core`-only build that takes its scratch from the caller instead of `alloc`: only if a real use case without a global allocator appears.
- Go and parallel sorting: only if a real use case appears.
