# Roadmap

This page tracks unfinished work and its acceptance conditions. Current behavior belongs in the [user guide](docs/guide/getting-started.md), the [engine reference](docs/design/engine.md) and [shared semantics](conformance/semantics.md); delete an item when it lands or is rejected, after moving any conclusion later work depends on to its owner.

## Engine

The [structure analysis](docs/design/evidence.md#structure-analysis) finds concentrated ownership, not proven minimality. [PR #120](https://github.com/Yikai-Liao/tessera-order/pull/120) has merged the direct source ledger, so the completion study is no longer waiting for a design choice. The [post-ledger proposal](docs/design/evidence.md#post-ledger-simplification-proposal) evaluates further simplification against that engine. Start with the independent scan and stream-capacity experiments below, then shared split completion; none is a prerequisite for resolving [acceptance debt](#acceptance-debt) or extending [Rayon](#rayon) coverage.

### Algorithm extension experiment

The [proposal](docs/design/algorithm-extensions-proposal.md) derives the target architecture from all known algorithm consumers, specifies unimplemented Rust interfaces and gives a simple-to-complex implementation sequence. Start with behavior-preserving structural stages, then implement complete operations individually; do not export placeholder APIs. Each stage is accepted only with its contract and ownership checks, the affected documentation and figures, and measured performance acceptance or an explicit pending decision. External sorting and Wavelet matrix construction remain deferred.

Foundations A and B place scalar order and insertion primitives below recursive execution, and give tile leaf planning, storage and restoration one independent owner. Serial and Rayon contract checks pass; timing acceptance remains open. Bounded linked-revision screens of both stages found unresolved short-input differences ([assessment](docs/design/evidence.md#algorithm-extension-foundations)). Foundation C publishes final nonempty band ranges from ledger-owned facts, with sparse-bucket and concurrent callback checks passing alongside workspace, serial and Rayon tests and Clippy. Its bounded linked screen also leaves short-input timing acceptance open. Miri's serial and Rayon fixtures and the site build pass in CI ([#225](https://github.com/Yikai-Liao/tessera-order/pull/225)). Foundation D implements shared rank requirements, ordered refinement and an invocation-owned workspace, with full sorting as its first public consumer. Partial-rank identity, skipped-child analysis, equality bands, rotated ranges, panic restoration and workspace reuse pass private tests and independent review. Controlled foundation timing acceptance remains open. The serial one-shot [operations](conformance/semantics.md#serial-slice-operations) are implemented; native contract and recovery tests cover the new callers. Binary partition and cached-key sorting share the forward one-call distribution with complete owned-record recovery. Incremental prefix sessions remain unimplemented. The [category baseline](docs/design/evidence.md#stable-category-partition-baseline) measured the retired bucket-partition entry; category grouping now uses `sort_by_cached_key`. The maintained benchmark pipeline measures complete public operation calls with independent result checking, output-aware memory accounting and offline tables/JSON. A bounded Rust comparison and historical full-sort build are complete; the [baseline assessment](docs/design/evidence.md#serial-slice-operation-baseline) records local losses and placement limits. New figures remain deferred.

The [release surface](docs/design/algorithm-extensions-proposal.md#release-surface-alignment) is implemented: the operations carry their standard names, the 0.1 sorts are deprecated aliases of `sort`, `sort_by_key`, `par_sort` and `par_sort_by_key`, and category grouping uses `sort_by_cached_key` with its [measured cost](docs/design/evidence.md#cached-key-category-grouping) accepted. Remaining after 0.2: remove the revision benchmarks' use of the deprecated names once no compared baseline predates them. Refinement now owns its production tile budget while retaining the private explicit-budget execution body for ownership tests. Its [linked full-sort assessment](docs/design/evidence.md#refinement-tile-budget-ownership) leaves timing acceptance open, including the listed short-input differences and unmeasured partial, segmented and Rayon calls.

### Acceptance debt

The consolidated engine (shared root preparation, node planning and order confirmation; one half-key extractor; shared direct-key counting; segmented scratch; bounded tile planner; single SIMD backend) has measured losses that no exception compensates for; they are listed under [landed with open losses](docs/design/evidence.md#landed-with-open-losses). Done when the shared paths are validated against the pre-consolidation baseline with A/A and placement controls, the open losses are resolved or individually accepted, and a fresh current-head Actions run passes preparation, every measurement shard, Cloud archival and publication. Raw artifacts alone do not satisfy publication acceptance. R3's ordered-finishing implementation is merged; its [short-input controls](docs/design/evidence.md#shared-ordered-finishing-controls) remain unresolved here rather than as an implementation candidate.

### Parallel completion study

The merged direct source ledger is the chosen C design: it retired parallel permutation/compaction recovery, and [#154](https://github.com/Yikai-Liao/tessera-order/pull/154) then retired private staging (P1 below). Its accepted losses and remaining evidence limits are in the [shared-ledger study](docs/design/evidence.md#shared-ledger-study). The independent A prototype and B's packed-output-stream model have [recorded blockers](docs/design/evidence.md#structure-analysis); those constructions need not be repeated, and their rejection does not exclude every packing or scattered-recursion design.

Remaining work:

- Validate above-window revision timing and investigate skew with placement controls; current-only output/allocation checks are not revision evidence. List remaining losses individually, without treating #120's exceptions as a general budget.
- Accept P1 below: final-band completion has replaced private staging, but its routing and movement stages still need complete performance and allocation acceptance. If it cannot preserve the saving, record the blocking cost and restore or replace it.
- Revisit A or B only with a construction that addresses the recorded blocker. The [study ceiling](docs/design/evidence.md#structure-analysis) remains available for research, but changing the public space bound requires an accepted design and an explicit contract decision.

Every candidate preserves the public semantics, including `Send`-only records, `Fn + Sync` extraction, panic restoration and termination under inconsistent keys. Model active and suspended allocations, not one live buffer per worker. Large-input measurements need maintainer approval of the budget. A follow-up is complete when its saving is demonstrated within the controls or its blocking cost is recorded; structural and routing changes land separately.

### Simplification candidates

Candidates replace specialized mechanisms with more general operations; they are not promises to remove every listed path. The [proposal](docs/design/evidence.md#post-ledger-simplification-proposal) owns the rationale and source-level caveats. A candidate lands when one mechanism replaces at least two, and the retired mechanism's saving is kept or each loss is listed and accepted individually (up to 10% locally, never a performance budget). Preserve ordering, stability, ownership, panic recovery, changing-key progress and the public memory bound. Evaluate one candidate per PR against the preceding merged head, with three-block screens and the [benchmark protocol](docs/development/benchmarking.md#run-an-experiment); separate structural and routing commits and make no source-causal claim without placement controls. Rejection completes an evaluation, not permission to remove the saving another way. The proposed [structural acceptance calibration](#benchmark-tooling-and-investigations) does not yet change these gates.

S1 ([accepted](docs/design/evidence.md#shared-explicit-direct-key-scan)), shared split completion and progress ownership (N1/N3/N4) are done. P1's first screen can reject its direction cheaply, but a favorable result must expand to the coverage below before deletion; it needs an operator-approved large-input budget. Do not gate unrelated local work on completion research.

| ID | Candidate | Done when |
|---|---|---|
| P1 | Staging retired by #154 with an independent whole-child capacity; acceptance pending | At fixed root plan, shards and block size, compare the last staging head (`76a985f`) with ordinary-capacity and extended-capacity final-band completion, starting at 10M records/eight threads. Extend to narrow/wide direct keys and rows, skew, the former eligibility window and oversized buckets. Accepted when timing, relocation/copy counts and peak/suspended allocation preserve staging's saving, or each loss is listed and accepted individually. |
| N6 | Equality digit implemented; acceptance pending | Paired screens against `baa2710` on serial 65k/1M and four-thread 1M skewed roots (zipf2, skew99, two_values, hot50, bimodal), with random and d4096 controls, across u64, row_u64, u128 and row_u32. List each loss individually ([construction](docs/design/evidence.md#equality-digit-consumes-the-top-bit)). The depth fix is a correctness change and is not traded against timing. |
| P5 | Resource model available; implementation deferred | Node depth is now bounded by key width ([equality digit](docs/design/evidence.md#equality-digit-consumes-the-top-bit)), which supplies the serial continuation bound the model required. The [model](docs/design/evidence.md#p5-recursive-parallel-nodes) permits one parallel phase at a time, not an unrestricted active frontier. Require a simplification payoff before implementation and preserve the public space/progress contract under reentrant suspension. Then measure skewed and balanced roots, thread counts, wide records and expensive extraction. Keep partition analysis shared; do not infer a bound from worker count or fixed digit depth. |

Follow-up order: A serial trial of the enlarged capacity follows P1's evidence, under the same capacity owner, with separate routing acceptance. P5 has a restricted resource model; it is not an instruction to add recursive task spawning. Do not reopen flat-state removal, fused order/variation, block-only tiles or the rejected A/B constructions without a new saving-preserving construction ([evidence](docs/design/evidence.md#structure-analysis)).

Smaller candidates remain available; none waits for P1's acceptance:

| ID | Retire | Replace with | Risk |
|---|---|---|---|
| P2 | Residual partition configuration | One description consumed by the block layer | `BucketFinish` and `BoundedFinish` are gone; reassess `Config` only if it duplicates policy rather than owning test geometry and execution controls |
| P3 | `tile::Scratch`, `block::Buffers`, job `Pool` | A shared scratch lifecycle | `engine::Scratch` now pairs the first two per sort; preserve nested/reentrant lifetimes and allocation reuse rather than assume one buffer per worker |


### Rayon

The optional feature and bounded completion are implemented ([semantics](conformance/semantics.md#rust-rayon-entry-points), [engine reference](docs/design/engine.md#parallel-root-completion), [baseline](docs/design/evidence.md#parallel-completion-baseline)). Done when whole-call scaling and memory coverage extend to other hosts, wide records and expensive extraction, with source-attribution controls for causal claims.

Repeated-key parallel roots (PR #78): validate the sharded equality route against main with four/eight-thread, random/ordered, wide/owned, panic and memory controls; resolve or individually accept serial regressions without changing public bounds. See [candidate evidence](docs/design/evidence.md#repeated-key-root-sharing).

## Python binding

An independent track binding the Rust crate; see the [design](docs/design/python-binding.md). It needs its own publication workflow, interpreter/ABI matrix and wheel install tests when it starts, not inactive CI.

## C++

1. **C++ runner.** Measure the C++ implementation with the shared input specification and native-buffer timing. Add cross-language golden hashes in `conformance/vectors/` and verify them in both generators.
2. **C++17 header-only implementation** for trivially copyable records, with CMake install and a consumer test. Done when it passes the shared vectors and has a direct comparison with Rust.

## Benchmark tooling and investigations

- Finish the [CI scaling proposal](docs/design/ci-scaling-proposal.md). Cache hygiene and lane reuse are implemented. Next come operation benchmark groups selected by entry ownership, then per-operation cross-algorithm plans.

- Fix the sample duration and prepared-memory cap of local paired sessions from the test machine's memory and the time a 10M-record case takes.
- Investigate benchmark build invalidation without weakening provenance. Missing watched paths can force repeated rebuilds, but the existing watches do not cover every file that can affect Git dirty state. A solution must reliably detect dirty-state changes outside the benchmark source trees, optional revision file transitions and packed or detached Git references. Validate actual Cargo builds and snapshot workflows before changing the watches.
- Study monomorphization work in the optimized smoke harness before changing generic engine boundaries: LLVM IR inspection showed repeated `Partition::adopt_shards` instances across record and extractor types (adoption is since retired; the ledger is generic over the record type only). IR size alone does not establish compile-time savings or justify losing inlining; any extraction needs compile-time measurements and the engine performance and ownership controls.

- Activate [main long-term tracking](docs/development/continuous-benchmarks.md#main-long-term-tracking) only after a complete Bare Metal pilot confirms static-image execution controls, matching complete Criterion results, timeout headroom, Git publication and browser loading. Repeat-run noise is required before adding historical alerts; image-registry retention and long-term Git growth need operational monitoring.

- Calibrate a structural acceptance lane before changing the per-case rule ([rationale](docs/design/evidence.md#structural-acceptance-needs-calibration)). Define structural scope as unchanged routing, classification, scan/pass counts, allocation policy and scheduling; SIMD, capacity, MSD-only finishing and recursive parallelism need timing evidence even if they delete code. Test fixed-work counters and movement/allocation models against same-source layouts and known regressions, retaining timing controls for cache, ISA and contention effects. Done when the maintained pipeline can report applicable coverage, uncertainty and each unresolved loss, and the maintainer approves the decision rule in the benchmark protocol. Until then counters remain descriptive, and unresolved timing needs an explicit per-case decision rather than an automatic waiver.
- Validate the [optional local AOT CLI](docs/development/benchmarking.md#optional-local-aot-cli) beyond offline commands: compare real preparation provenance, frozen binary identity and workspace cleanup against direct Bun, exercise interruption and recovery through the compiled entry point, and measure larger current-format real reports separately from fresh-process startup. Keep stable Bun direct execution as the default, including in CI; do not distribute AOT through Actions or Release. Broader AOT adoption needs this evidence and platform coverage.
- Let the Rust comparison manifest own the expected memory-case inventory, consumed by measurement and offline completeness validation, keeping timing/memory process isolation.
- Evaluate one normalized worker request instead of parallel TS/Rust user-facing CLI rules, keeping timed calls, inputs and recovery; validate any Criterion removal with code-placement controls.
- Expose structured current-head performance details through `status` from the check publisher's machine summary, not by parsing report wording.
- Validate the hosted paired pipeline on Actions: cold/warm full-suite duration, queue wait, paired median intervals, same-source false alerts and held-out detection for the required backend, before claiming hosted stability or tuning sample budgets; verify direct PR Cloud upload, fork/Dependabot summary-only behavior and partial reruns. An earlier calibration (PR #96, run `42904693411`) had no same-source alerts over 70 cases per configuration with measurement jobs of 346–406 s, which does not establish long-term rates.
- Retain raw paired evidence beyond Actions' 14-day artifacts if longer-term offline regeneration is needed.
- Investigate near-order, sparse-digit and skewed-key deficits with general mechanisms, not per-distribution tuning, against current-head continuous feedback.
- Add cold-cache experiments, more seeds and threshold cases, and native-CPU placement controls for source A/B studies (existing CGU, alignment and engine-order screens are process-level). Validate alignment's dispersion across hosted CPU models; revisit multi-layout sampling only with held-out false-alert, detection and whole-job cost evidence.
- Accept manual algorithm tracking: run Bare Metal pilots for every production adapter, especially the slowest wide/random and adverse cases, and confirm each algorithm's single remote job completes within 300 seconds with headroom. Verify native runtime images, glidesort-sqrt scratch evidence, per-algorithm Git publication and browser loading. Workflow plumbing and local functional checks alone do not satisfy remote timeout acceptance. Populate the [algorithm comparison](docs/benchmarks/algorithms.md) with a run of every opponent after memory tracking lands, and check that its cohort covers the opponents' u64 inputs.

## Documentation figures

The block-partition figure replays the engine through WebAssembly ([site guide](docs/development/docs-site.md#writing-pages)); the next step retires the remaining engine model.

- Move `CountingPassDemo` from the frame slider in `StepControls` to structural navigation, then retire `StepControls`.
- Replace `sim/digits.ts` (`KeySample::collect`, `plan_digits`, `top_window`) with engine events, and extend the trace to tile finishing where a figure needs it.

## Integrations

Rerun the StarRocks and ClickHouse benchmarks against a released crate and link the results from the README. They contain upstream Apache-2.0 code, so they live outside this repository ([rationale](CONTRIBUTING.md#license)).

## Not planned

- A dedicated unstable kernel before 0.1.
- vcpkg and Conan packages: after the first releases, on demand.
- A `core`-only build taking scratch from the caller instead of `alloc`: only for a real use case without a global allocator.
- Go: only if a real use case appears.
