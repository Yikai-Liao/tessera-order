# Roadmap

This page tracks unfinished work and its acceptance conditions. Current behavior belongs in the [user guide](docs/guide/getting-started.md), the [engine reference](docs/design/engine.md) and [shared semantics](conformance/semantics.md); delete an item when it lands or is rejected, after moving any conclusion later work depends on to its owner.

Pull-request performance acceptance is governed by hosted PR paired benchmarks ([performance changes](CONTRIBUTING.md#performance-changes)), while `main` tracking provides long-term absolute trends ([continuous benchmarks](docs/development/continuous-benchmarks.md#main-long-term-tracking)). Do not keep retroactive acceptance gates against historical pre-0.1 commits; record historical caveats and open trade-offs in [engine evidence](docs/design/evidence.md), and fix remaining deficits against current `main`.

## Engine

<a id="operation-extension-experiment"></a>

### 0.2 release surface and follow-ups

The [0.2 scope decision](docs/design/operation-extensions-proposal.md#release-surface-alignment) shipped in 0.2.0: full and cached-key sorting, optional Rayon full sorting and argsort (`par_argsort`, `par_argsort_by_key`), serial argsort, single-rank selection, and fixed-width keys including `bool`, `char`, `NonZero`, byte arrays and tuples up to 128 bits.

Post-0.2 cleanup and capabilities (new capabilities require a concrete consumer and per-PR acceptance):

- **Parallel argsort at 10M:** compare whole `par_argsort` and `par_argsort_by_key` calls, including output allocation, against serial `argsort` and `par_sort` at 10M records in a focused local plan beyond the [hosted cases](docs/development/continuous-benchmarks.md#cases), with serial `argsort` regression checks.
- Remove the revision benchmarks' use of the deprecated 0.1 `stable_*` names once no compared baseline needs them.
- **Exact cached-key facts:** reuse order/variation facts produced during `sort_by_cached_key` collection without regressing `sort`'s root codegen (closure-routed reuse was [closed](docs/design/evidence.md#rejected-or-on-hold)) while preserving descending/near-order completion and the exactly-once contract.
- **Full-key counts:** let complete-key histograms support unique/count outputs without refilling duplicate keys first, preserving outside-bit proofs and float encoding semantics.
- **Read-only small results:** evaluate kth and small-k index outputs with bounded tile-local candidates before adding multiple-prefix scheduling or parallel rank APIs.

### Engine optimizations

- **Dense interior `select_nth`:** reduce payload movement without losing existing root fast paths, stability, panic/inconsistent-key safety or the workspace bound (a pre-root 11-bit histogram + three-way `block::partition` was [rejected](docs/design/evidence.md#rejected-or-on-hold)). Done when dense interior selection improves on narrow and wide records without regressing endpoints or full-sort controls.
- **`argsort` packing and extraction:** evaluate packing the full remaining key width within two-pass tile LSD so narrow keys avoid tie lookups, and separately evaluate whether decoupling deterministic uncached tile extraction from bijective count/refill ([X2 rationale](docs/design/evidence.md#x1-x2-reconstruct-direct-keys-from-counts)) pays off on packed index words. (Direct prefill-free preparation was [closed](docs/design/evidence.md#rejected-or-on-hold).)
- **Tiny scalar leaves (`len <= 32`):** close the remaining [short insertion leaf losses](docs/design/evidence.md#short-insertion-leaves) on tiny scalar inputs only with a general mechanism that preserves `random/48` radix wins and `sorted99`/`equal` fast paths.
- **Direct SIMD / forwarding for `NonZero<T>` and restricted-validity keys:** evaluate transparent integer-view forwarding separately from SIMD support, preserving the nonzero invariant without weakening `SimdKey` safety obligations.
- **Cached-key block-sized distribution:** revisit a movement-only block path only if timing or allocation evidence justifies removing the remaining no-op finisher / per-bucket ends.
- **Near-order, sparse-digit, skewed and cross-architecture parity:** investigate remaining deficits (including `par_argsort` skewed huge-tie overhead and cross-architecture SIMD stream-count parity) with general mechanisms against current-head continuous feedback.

### Simplification candidates

Candidates replace specialized mechanisms with more general operations; they are not promises to remove every listed path. The [structure analysis](docs/design/evidence.md#structure-analysis) and [post-ledger summary](docs/design/evidence.md#post-ledger-simplification-proposal) own the rationale and historical studies (S1, P1, N1/N3/N4, N6, X1/X2, P4 are merged and shipped; A/B and T2 are rejected). A new candidate lands when one mechanism replaces at least two, satisfies the [hosted performance rule](CONTRIBUTING.md#performance-changes) for every hosted alert, and lists and accepts any additional local loss up to 10% individually (never as a standing budget and never in place of hosted alert acceptance). Preserve ordering, stability, ownership, panic recovery, changing-key progress and the public memory bound.

| ID | Scope | Status / Done when |
|---|---|---|
| P3 | Unify `tile::Scratch`, `block::Buffers` and parallel job `Pool` into one scratch lifecycle | `engine::Scratch` already pairs the first two; preserve nested/reentrant lifetimes and allocation reuse rather than assuming one buffer per worker. |
| P5 | Restricted tail-parallel child continuation ([resource model](docs/design/evidence.md#p5-recursive-parallel-nodes)) | Deferred; requires a clear simplification or skewed-scaling payoff within O(P√n) and one active parallel phase per invocation. |

Do not reopen flat-state removal, fused order/variation, block-only tiles or rejected A/B/T2 constructions without a new saving-preserving construction ([evidence](docs/design/evidence.md#structure-analysis)).

### Rayon

- **Parallel root preparation:** `par_sort`, `par_sort_by_key` and `par_argsort` currently run `prepare` / `resolve_varying` serially on one thread, scanning every record on ordered, constant or narrow-bit inputs. Done when parallel preparation improves multi-core scaling on those inputs under existing `Send`/`Sync` and `Fn + Sync` bounds, keeps zero workspace heap on already ordered/constant inputs, bounds and documents extra reads past serial early exits, and does not regress random or serial controls.
- **Wider host/workload coverage:** extend scaling and memory observations to additional hosts, wide records and expensive key extraction as real workloads demand.

## Python binding

The PyO3 binding in `python/` implements `sort`, `sort_inplace`, `argsort`, `partition`, `partition_inplace` and `argpartition` over `list`, `array.array` and N-dimensional `numpy.ndarray` with optional Rayon `threads=` control, synchronized with the Rust crate version; see [design](docs/design/python-binding.md).

Remaining post-0.2 items:

- Validate a free-threaded (no-GIL) buffer access design before advertising free-threaded wheels (`#[pymodule(gil_used = true)]` currently enforces a GIL-enabled interpreter).
- Evaluate zero-callback numeric-field sorting for structured NumPy dtypes if real workloads need faster than `key=` throughput.

## C++

1. **C++ runner.** Measure the C++ implementation with the shared input specification and native-buffer timing. Add cross-language golden hashes in `conformance/vectors/` and verify them in both generators.
2. **C++17 header-only implementation** for trivially copyable records, with CMake install and a consumer test. Done when it passes the shared vectors and has a direct comparison with Rust.

## Benchmark tooling and investigations

- Finish the [CI scaling proposal](docs/design/ci-scaling-proposal.md) (per-operation cross-implementation plans).
- Calibrate a structural acceptance lane before relaxing the per-case timing rule ([rationale](docs/design/evidence.md#structural-acceptance-needs-calibration)).
- Populate and validate manual opponent tracking runs on Bare Metal (`main-benchmarks.yml`), confirming timeout headroom (< 300 s per remote job), native runtime images, `glidesort-sqrt` scratch evidence and site rendering on the [implementation comparison](docs/benchmarks/implementations.md).
- Size local paired session sample duration and prepared-memory cap from host RAM and 10M-record runtime; investigate benchmark build invalidation without weakening provenance.
- Let the Rust comparison manifest own the expected memory-case inventory; evaluate one normalized worker request instead of parallel TS/Rust CLI rules; expose structured current-head performance summaries in `status`.
- Validate the [optional local AOT CLI](docs/development/benchmarking.md#optional-local-aot-cli) beyond offline commands (real preparation, frozen binary identity, cleanup, interruption recovery). Keep direct Bun as default and in CI.
- Add cold-cache experiments, more seeds/threshold cases and native-CPU placement controls for source A/B studies as needed.

## Documentation figures

The block-partition figure replays the engine through WebAssembly ([site guide](docs/development/docs-site.md#writing-pages)); next retire the remaining engine model:

- Move `CountingPassDemo` from the frame slider in `StepControls` to structural navigation, then retire `StepControls`.
- Replace `sim/digits.ts` (`KeySample::collect`, `plan_digits`, `top_window`) with engine events, and extend the trace to tile finishing where a figure needs it.

## Integrations

Rerun the StarRocks and ClickHouse benchmarks against a released crate and link the results from the README. They contain upstream Apache-2.0 code, so they live outside this repository ([rationale](CONTRIBUTING.md#license)).

## Not planned

- A dedicated unstable kernel without a saving-preserving general primitive ([evidence](docs/design/evidence.md#nodes-movement-and-unstable-contracts)).
- vcpkg and Conan packages: on demand after C++ releases.
- A `core`-only build taking scratch from the caller instead of `alloc`: only for a real use case without a global allocator.
- Go: only if a real use case appears.
