# Roadmap

This page tracks unfinished work and its acceptance conditions. Current behavior belongs in the [user guide](docs/guide/getting-started.md), the [engine reference](docs/design/engine.md) and [shared semantics](conformance/semantics.md); delete an item when it lands or is rejected, after moving any conclusion later work depends on to its owner.

## Rust 0.1

The first crates.io release: stable sort only.

| Deliverable | Done when |
|---|---|
| First release comparison of the always-enabled SIMD backend on Stable | The complete local session regenerates from its raw directory; README and the site show its figures; the release attaches the archive |
| Release | Robert Clausecker has been told before publication; `rust/v0.1.0` follows [`RELEASING.md`](RELEASING.md) |

Open questions:

- **Sample duration and prepared-memory cap.** Fix the defaults from the test machine's memory and the time a 10M-record case takes.
- **Cost of cached bucket IDs on 8-byte rows.** Evaluate `{u32 key, u32 row}` in the first release report. If the cost is large, consider an `unsafe` entry point that skips the cache ([rationale](docs/design/engine.md#record-handling)).

## Engine

The [structure analysis](docs/design/evidence.md#structure-analysis) finds concentrated ownership, not proven minimality. [PR #120](https://github.com/Yikai-Liao/tessera-sort/pull/120) has merged the direct source ledger, so the completion study is no longer waiting for a design choice. The [post-ledger proposal](docs/design/evidence.md#post-ledger-simplification-proposal) evaluates further simplification against that engine. Start with the independent scan and stream-capacity experiments below, then shared split completion; none is a prerequisite for resolving [acceptance debt](#acceptance-debt) or extending [Rayon](#rayon) coverage.

### Acceptance debt

The consolidated engine (shared root preparation, node planning and order confirmation; one half-key extractor; shared direct-key counting; segmented scratch; bounded tile planner; single SIMD backend) has measured losses that no exception compensates for; they are listed under [landed with open losses](docs/design/evidence.md#landed-with-open-losses). Done when the shared paths are validated against the pre-consolidation baseline with A/A and placement controls, the open losses are resolved or individually accepted, and a fresh current-head Actions run passes preparation, all four measurement shards, Cloud archival and publication. Raw artifacts alone do not satisfy publication acceptance. R3's ordered-finishing implementation is merged; its [short-input controls](docs/design/evidence.md#shared-ordered-finishing-controls) remain unresolved here rather than as an implementation candidate.

### Parallel completion study

The merged direct source ledger is the chosen C design: it retires parallel permutation/compaction recovery while retaining staging and streaming. Its accepted losses and remaining evidence limits are in the [shared-ledger study](docs/design/evidence.md#shared-ledger-study). The independent A prototype and B's packed-output-stream model have [recorded blockers](docs/design/evidence.md#structure-analysis); those constructions need not be repeated, and their rejection does not exclude every packing or scattered-recursion design.

Remaining work:

- Validate above-window revision timing and investigate skew with placement controls; current-only output/allocation checks are not revision evidence. List remaining losses individually, without treating #120's exceptions as a general budget.
- Evaluate P1 below to decide whether one movement strategy can preserve staging's saving. If it cannot, record its blocking cost and retain both strategies as a trade-off.
- Revisit A or B only with a construction that addresses the recorded blocker. The [study ceiling](docs/design/evidence.md#structure-analysis) remains available for research, but changing the public space bound requires an accepted design and an explicit contract decision.

Every candidate preserves the public semantics, including `Send`-only records, `Fn + Sync` extraction, panic restoration and termination under inconsistent keys. Model active and suspended allocations, not one live buffer per worker. Large-input measurements need maintainer approval of the budget. A follow-up is complete when its saving is demonstrated within the controls or its blocking cost is recorded; structural and routing changes land separately.

### Simplification candidates

Candidates replace specialized mechanisms with more general operations; they are not promises to remove every listed path. The [proposal](docs/design/evidence.md#post-ledger-simplification-proposal) owns the rationale and source-level caveats. A candidate lands when one mechanism replaces at least two, and the retired mechanism's saving is kept or each loss is listed and accepted individually (up to 10% locally, never a performance budget). Preserve ordering, stability, ownership, panic recovery, changing-key progress and the public memory bound. Evaluate one candidate per PR against the preceding merged head, with three-block screens and the [benchmark protocol](docs/development/benchmarking.md#run-an-experiment); separate structural and routing commits and make no source-causal claim without placement controls. Rejection completes an evaluation, not permission to remove the saving another way. The proposed [structural acceptance calibration](#benchmark-tooling-and-investigations) does not yet change these gates.

Start with S1 and P1 independently. Their first screens can reject a direction cheaply, but a favorable result must expand to the coverage below before deletion. Then attempt shared split completion (N1/N3/N4). Do not gate unrelated local work on completion research.

| ID | Candidate | Done when |
|---|---|---|
| S1 | One explicit `fearless_simd` direct-key scan instead of separate scan kernels and their extractor adapter | Do not depend on automatic vectorization; test with LLVM loop/SLP vectorization disabled as well as the normal build. Actual dispatched code generation and paired whole-call results preserve sorted, `sorted99` and `far_swaps` performance with random/short controls; cover integer/float encodings, `Reverse`, wide fallbacks, baseline/native targets and MSRV. Retain one dispatch per pass and sealing. The shared pipeline now supplies opt-in [reversed integer/float controls](docs/development/benchmarking.md#linked-revision-comparisons); [PR #126](https://github.com/Yikai-Liao/tessera-sort/pull/126) implements the shared scan with a [scoped acceptance](docs/design/evidence.md#shared-explicit-direct-key-scan); [PR #128](https://github.com/Yikai-Liao/tessera-sort/pull/128) retires the insertion extractor adapters with [separate scoped acceptance](docs/design/evidence.md#direct-insertion-scan-operation). Broader width/backend coverage and full S1 acceptance remain open; investigate the operator-accepted 49–1,000-record differences later without treating them as a general loss budget. Prefix-sum and dependency removal are separate decisions. |
| P1 | Streaming final bands with the same child capacity as staging | At fixed root plan, shards and block size, compare current staging, ordinary-capacity streaming and extended-capacity streaming, starting at 10M records/eight threads. Extend to narrow/wide direct keys and rows, skew, below/above eligibility and oversized buckets. Retire staging only after timing, relocation/copy counts, peak/suspended allocation and owned/borrowed, panic, inconsistent-key and reentry coverage preserve its saving and invariants. Block sizing and capacity have explicit owners independent of staging. |
| N1/N3/N4 | One rank classifier, digit partition entry and split completion operation | Equal, Runs and Digit share child completion while preserving completed bands, candidate masks, no-move/rotation savings and cache-hot callbacks. Fixed-work counters and work models show no added scans or passes, with small-case timing controls. Close N1 only after consolidating progress ownership: actual non-progress must still force a bit-consuming plan under changing keys, without a second executor interpretation. A shared classifier alone closes neither progress nor scheduling. |
| N5 | One node/tile recursive operation | After split completion, remove duplicated analysis without extra order/variation scans, losing whole-tile insertion or leaking tile recovery. Check insertion/tile boundaries and 49–1,000-record cases with counters and placement controls. |
| T2 | MSD-only tile finishing | First preserve terminal one-digit completion; separately compare two-pass LSD with MSD on 8,192 keys in a 20-bit range, then repeated, sparse and wide-record cases. Account for table initialization, copy-back, scans and insertion. Delete LSD planning, tests and model branches only when their saving survives. |
| X1/X2 | Direct-key reconstruction and shared counting | Prove bit-exact inverse encoding and preservation of bits outside the window, including narrowed keys, floats and `Reverse`; preserve user-payload decline and `Send`-only parallel filling. Share counting without adding Rayon calls to serial operation. Close X2 only when one authoritative direct-key fact supplies uncached classification and count/fill eligibility through `KeyHalf`, while user-key records retain caching and decline filling. Keep window width unchanged first; widening needs a separate storage/cost budget. Scan-method cleanup follows S1 if accepted, but reconstruction can be tested independently. |
| P4 | Direct buffer borrowing, then retired-child delivery instead of end-vector recycling | First preserve adoption, guard restoration and reuse without take/return bookkeeping. After shared split completion, separately show that child delivery removes the end-vector pool without replacing it with unbounded queues, extra allocation or conflicting parent/child scratch borrows. Preserve immediate tile finishing and published-borrow panic coverage. |
| P5 | Recursive parallel scheduling of the same node decisions | Before implementation, prove the public space bound and progress with live parent directories, an active frontier and reentrant suspension. Then measure skewed and balanced roots, thread counts, wide records and expensive extraction. Keep partition analysis shared; do not infer a bound from worker count or fixed digit depth. |

Follow-up order: N5 and child delivery in P4 depend on shared split completion; P4's local buffer borrowing, T2 and key reconstruction are independent. A serial trial of the enlarged capacity follows P1's evidence, under the same capacity owner, with separate routing acceptance. P5 starts with a resource model, not recursive task spawning. Do not reopen flat-state removal, fused order/variation, block-only tiles or the rejected A/B constructions without a new saving-preserving construction ([evidence](docs/design/evidence.md#structure-analysis)).

Existing smaller candidates remain available; none is a prerequisite for S1 or P1:

| ID | Retire | Replace with | Risk |
|---|---|---|---|
| R4 | The `INSERTION_MAX` root shortcut beside the budgeted probe | One insertion attempt owning its budget | Known short `random/1000` losses; measure first |
| R5 | `Descending { strict }` | Always reverse stably | One extra scan on `reverse`; likely rejected, listed to measure once |
| N2 | `two_runs`, `NodePlan::Runs`, `split_ordered` | A block partition that detects non-decreasing bucket order and returns without moving | Loses the rotated `1*0*` form; `sorted_tail`, `bimodal`, `far_swaps`; speculative |
| X3 | Duplicated half-key dispatch in serial and parallel entries | One root helper, or `KeyHalf` over the narrowest covering unsigned type | The second is a routing change for narrow 64-bit variation |
| X4 | `finish_insertion` beside `insertion_sort(usize::MAX)` | One entry | Measured micro-choice on short tiles |
| T1 | Two homes for cached bucket IDs | Always the scratch tail | An extra cache line set per by-key pass on small tiles |
| P2 | `Config`, `BucketFinish` and `BoundedFinish` describing one root partition | One description consumed by the block layer | Reassess after P1 so deleted staging policy does not acquire another wrapper |
| P3 | `tile::Scratch`, `block::Buffers`, job `Pool` | A shared scratch lifecycle | Reassess after P1/P4; preserve nested/reentrant lifetimes and allocation reuse rather than assume one buffer per worker |

### Rayon

The optional feature and bounded completion are implemented ([semantics](conformance/semantics.md#rust-rayon-entry-points), [engine reference](docs/design/engine.md#parallel-root-completion), [baseline](docs/design/evidence.md#parallel-completion-baseline)). Done when whole-call scaling and memory coverage extend to other hosts, wide records and expensive extraction, with source-attribution controls for causal claims.

Repeated-key parallel roots (PR #78): validate the sharded equality route against main with four/eight-thread, random/ordered, wide/owned, panic and memory controls; resolve or individually accept serial regressions without changing public bounds. See [candidate evidence](docs/design/evidence.md#repeated-key-root-sharing).

## Python binding

An independent track binding the Rust crate; see the [design](docs/design/python-binding.md). It needs its own publication workflow, interpreter/ABI matrix and wheel install tests when it starts, not inactive CI.

## C++

1. **C++ runner.** Measure the C++ implementation with the shared input specification and native-buffer timing. Add cross-language golden hashes in `conformance/vectors/` and verify them in both generators.
2. **C++17 header-only implementation** for trivially copyable records, with CMake install and a consumer test. Done when it passes the shared vectors and has a direct comparison with Rust.

## Benchmark tooling and investigations

- Calibrate a structural acceptance lane before changing the per-case rule ([rationale](docs/design/evidence.md#structural-acceptance-needs-calibration)). Define structural scope as unchanged routing, classification, scan/pass counts, allocation policy and scheduling; SIMD, capacity, MSD-only finishing and recursive parallelism need timing evidence even if they delete code. Test fixed-work counters and movement/allocation models against same-source layouts and known regressions, retaining timing controls for cache, ISA and contention effects. Done when the maintained pipeline can report applicable coverage, uncertainty and each unresolved loss, and the maintainer approves the decision rule in the benchmark protocol. Until then counters remain descriptive, and unresolved timing needs an explicit per-case decision rather than an automatic waiver.
- Validate the [optional local AOT CLI](docs/development/benchmarking.md#optional-local-aot-cli) beyond offline commands: compare real preparation provenance, frozen binary identity and workspace cleanup against direct Bun, exercise interruption and recovery through the compiled entry point, and measure larger current-format real reports separately from fresh-process startup. Keep stable Bun direct execution as the default, including in CI; do not distribute AOT through Actions or Release. Broader AOT adoption needs this evidence and platform coverage.
- Let the Rust comparison manifest own the expected memory-case inventory, consumed by measurement and offline completeness validation, keeping timing/memory process isolation.
- Evaluate one normalized worker request instead of parallel TS/Rust user-facing CLI rules, keeping timed calls, inputs and recovery; validate any Criterion removal with code-placement controls.
- Expose structured current-head performance details through `status` from the check publisher's machine summary, not by parsing report wording.
- Validate the hosted paired pipeline on Actions: cold/warm full-suite duration, queue wait, paired median intervals, same-source false alerts and held-out detection for the required backend, before claiming hosted stability or tuning sample budgets; verify direct PR Cloud upload, fork/Dependabot summary-only behavior and partial reruns. An earlier calibration (PR #96, run `42904693411`) had no same-source alerts over 70 cases per configuration with measurement jobs of 346–406 s, which does not establish long-term rates.
- Retain raw paired evidence beyond Actions' 14-day artifacts if longer-term offline regeneration is needed.
- Investigate near-order, sparse-digit and skewed-key deficits with general mechanisms, not per-distribution tuning, against current-head continuous feedback.
- Add cold-cache experiments, more seeds and threshold cases, and native-CPU placement controls for source A/B studies (existing CGU, alignment and engine-order screens are process-level). Validate alignment's dispersion across hosted CPU models; revisit multi-layout sampling only with held-out false-alert, detection and whole-job cost evidence.
- Add hosted algorithm comparison groups after the local release report, keeping complete per-type length groups apart from revision-ratio Cloud history.

## Integrations

Rerun the StarRocks and ClickHouse benchmarks against a released crate and link the results from the README. They contain upstream Apache-2.0 code, so they live outside this repository ([rationale](CONTRIBUTING.md#license)).

## Not planned

- A dedicated unstable kernel before 0.1.
- vcpkg and Conan packages: after the first releases, on demand.
- A `core`-only build taking scratch from the caller instead of `alloc`: only for a real use case without a global allocator.
- Go: only if a real use case appears.
