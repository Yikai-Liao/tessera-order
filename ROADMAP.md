# Roadmap

This page tracks unfinished work and its acceptance conditions. Current behavior belongs in the [user guide](docs/guide/getting-started.md), the [engine reference](docs/design/engine.md) and [shared semantics](conformance/semantics.md); delete an item when it lands or is rejected, after moving any conclusion later work depends on to its owner.

## Engine

The [structure analysis](docs/design/evidence.md#structure-analysis) finds concentrated ownership, not proven minimality. [PR #120](https://github.com/Yikai-Liao/tessera-order/pull/120) has merged the direct source ledger, so the completion study is no longer waiting for a design choice. The [post-ledger proposal](docs/design/evidence.md#post-ledger-simplification-proposal) evaluates further simplification against that engine. Start with the independent scan and stream-capacity experiments below, then shared split completion; none is a prerequisite for resolving [acceptance debt](#acceptance-debt) or extending [Rayon](#rayon) coverage.

### Operation extension experiment

The [0.2 scope decision](docs/design/operation-extensions-proposal.md#release-surface-alignment) keeps full and cached-key sorting, optional Rayon full sorting, argsort and single-rank selection. The removed development APIs have [migration recipes](docs/guide/getting-started.md#migrate-from-earlier-development-snapshots). It also [admits](docs/design/operation-extensions-proposal.md#0-2-additions) parallel argsort and tuple keys:

- **Parallel argsort.** Done when `par_argsort` and `par_argsort_by_key` sort the engine argsort's packed index words through the existing parallel root, pass the argsort tie, panic and inconsistent-key tests, and show whole-call scaling against serial argsort and `par_sort` at 1M and 10M records without serial argsort regressions. If they need a second scatter or staging path, record the blocker and defer them.
- **Tuple keys.** Done when `RadixKey` covers tuples up to 128 encoded bits with tuple-order tests (signed, float, `Reverse` per field), the [key encoding](conformance/semantics.md#key-encoding) states the concatenation, too-wide tuples fail to compile, and u64/u128 sorts show no regression. Remove the revision benchmarks' use of the deprecated 0.1 sort names once no compared baseline needs them.

Controlled timing acceptance is still open for the [extension foundations](docs/design/evidence.md#operation-extension-foundations) and [tile-budget ownership change](docs/design/evidence.md#refinement-tile-budget-ownership), including unresolved short-input differences. The [serial operation](docs/design/evidence.md#serial-slice-operation-baseline) and [category](docs/design/evidence.md#stable-category-partition-baseline) baselines include retired operations and do not establish acceptance for the narrower surface. Preserve existing figures; new operation figures remain deferred.

### Engine and algorithm boundary

Evaluate the [complete-operation boundary](docs/design/engine.md#knowledge-boundaries) against the preceding stages: cover selection at band boundaries and ties, cached-key distribution, argsort, short and large full sorts, and Rayon large-child completion. Revisit [tile-local selection](docs/design/evidence.md#tile-local-selection-experiment) only with a workload and a design that can improve on full tile sorting. No speedup or regression acceptance follows from the structural change alone.

Reduce payload movement for dense interior ranks. The root currently moves records into every digit bucket for a single boundary; a pre-root 11-bit histogram plus three-way `block::partition` was [rejected](docs/design/evidence.md#rejected-or-on-hold). Evaluate a construction that preserves existing root fast paths, stability, panic/inconsistent-key safety and the workspace bound without adding a second movement kernel or an extra full read pass on uniform inputs. Done when dense interior selection improves on narrow and wide records (or its blocking cost is recorded) without regressing endpoints or full-sort controls. See the [sparse-order measurements](docs/design/evidence.md#sparse-order-as-source-spans). For the endpoints, narrow the timing uncertainty on distant swaps and random narrow keys. See the [stable endpoint measurements](docs/design/evidence.md#stable-endpoints).

Eliminate redundant preparation and indirect lookups in `argsort` (`refine/indices.rs`): probe order and scan variation directly on source slices (using `Direct` scans for `argsort`) instead of pre-filling `0..len` before `scatter_indices` overwrites it, while cached-key sorting reuses the exact varying-bit mask already computed during key collection without a second root variation scan (tracking descending and near-order completion under the cached-facts item below). Evaluate packing the full remaining key width when it stays within two-pass tile LSD so narrow keys need no tie lookups, and separately evaluate whether decoupling deterministic uncached tile extraction from bijective count/refill ([X2 rationale](docs/design/evidence.md#x1-x2-reconstruct-direct-keys-from-counts)) saves enough on packed index words to justify the added extractor state. Done when argsort and cached-key sorting improve under paired controls, pass all tie, panic, inconsistent-key and exactly-once tests, keep `sort`/`sort_by_key` neutral, and record the decision if decoupling `Extract::DIRECT` does not pay off.

Cached-key distribution on block-sized inputs still builds per-bucket ends and invokes a no-op finisher through the block operation that full refinement needs for boundaries and child finishing. This change removes only tile boundary output; it does not establish a block speedup. Revisit a complete movement-only block operation if allocation or timing evidence justifies it, keeping the same restoration guarantees.

Close the [short insertion leaf losses](docs/design/evidence.md#short-insertion-leaves): tiny random keys still trail the standard stable sort, and nearly sorted inputs just above the width-based limit lost to the previous engine under the current small-slice probe swap budget. Evaluate a single-guard hole-shift insertion loop and a small-slice probe-budget floor. Done when those cases match or beat the previous engine and the standard stable sort, without returning random 48-record inputs to insertion.

Resolve short-input placement sensitivity for [shared order confirmation](docs/design/evidence.md#shared-order-confirmation-for-serial-operations) before claiming a gain across input sizes.

Next capabilities follow the [engine capability design](docs/design/operation-extensions-proposal.md#further-engine-capabilities), each with a concrete consumer and independent acceptance:

- Reuse cached order/variation facts while preserving descending and near-order completion; keep the exactly-once caller-key contract.
- Let complete-key counts support unique/count outputs without refilling duplicate keys first; retain outside-bit proof and float encoding semantics.
- Evaluate read-only kth and small-k index outputs with bounded candidates, before adding multiple-prefix scheduling or parallel rank APIs.

### Acceptance debt

The consolidated engine (shared root preparation, node planning and order confirmation; one half-key extractor; shared direct-key counting; bounded tile planner; single SIMD backend) has measured losses that no exception compensates for; they are listed under [landed with open losses](docs/design/evidence.md#landed-with-open-losses). Done when the shared paths are validated against the pre-consolidation baseline with A/A and placement controls, the open losses are resolved or individually accepted, and a fresh current-head Actions run passes preparation, every measurement shard, Cloud archival and publication. Raw artifacts alone do not satisfy publication acceptance. R3's ordered-finishing implementation is merged; its [short-input controls](docs/design/evidence.md#shared-ordered-finishing-controls) remain unresolved here rather than as an implementation candidate.

### Parallel completion study

The merged direct source ledger is the chosen C design: it retired parallel permutation/compaction recovery, and [#154](https://github.com/Yikai-Liao/tessera-order/pull/154) then retired private staging (P1 below). Its accepted losses and remaining evidence limits are in the [shared-ledger study](docs/design/evidence.md#shared-ledger-study). The independent A prototype and B's packed-output-stream model have [recorded blockers](docs/design/evidence.md#structure-analysis); those constructions need not be repeated, and their rejection does not exclude every packing or scattered-recursion design.

Remaining work:

- Validate above-window revision timing and investigate skew with placement controls; current-only output/allocation checks are not revision evidence. List remaining losses individually, without treating #120's exceptions as a general budget.
- Accept P1 below: final-band completion has replaced private staging, but its routing and movement stages still need complete performance and allocation acceptance. If it cannot preserve the saving, record the blocking cost and restore or replace it.
- Revisit A or B only with a construction that addresses the recorded blocker. The [study ceiling](docs/design/evidence.md#structure-analysis) remains available for research, but changing the public space bound requires an accepted design and an explicit contract decision.

Every candidate preserves the public semantics, including `Send`-only records, `Fn + Sync` extraction, panic restoration and termination under inconsistent keys. Model active and suspended allocations, not one live buffer per worker. Large-input measurements need maintainer approval of the budget. A follow-up is complete when its saving is demonstrated within the controls or its blocking cost is recorded; structural and routing changes land separately.

### Simplification candidates

Candidates replace specialized mechanisms with more general operations; they are not promises to remove every listed path. The [proposal](docs/design/evidence.md#post-ledger-simplification-proposal) owns the rationale and source-level caveats. A candidate lands when one mechanism replaces at least two, and the retired mechanism's saving is kept or each loss is listed and accepted individually (up to 10% locally, never a performance budget). Preserve ordering, stability, ownership, panic recovery, changing-key progress and the public memory bound. Evaluate one candidate per PR against the preceding merged head, with three-block screens and the [benchmark protocol](docs/development/benchmarking.md#run-an-experiment); keep structural and routing changes separately reviewable and make no source-causal claim without placement controls. Rejection completes an evaluation, not permission to remove the saving another way. The proposed [structural acceptance calibration](#benchmark-tooling-and-investigations) does not yet change these gates.

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

Parallel root preparation: `par_sort` and `par_sort_by_key` currently run `prepare` serially on one thread, scanning every record whenever any key bit is constant (timestamps, positive signed integers, bounded floats, `d256`, `sparse_digits`, `zipf2`) or inputs are ordered. Done when parallel root preparation improves multi-core scaling on ordered, constant and narrow-bit inputs under the existing `T: Send` and `Fn + Sync` bounds, keeps zero crate heap allocation on already ordered or constant inputs, bounds and documents any reads beyond the serial early exits, and does not regress random or serial controls.

Repeated-key parallel roots (PR #78): validate the sharded equality route against main with four/eight-thread, random/ordered, wide/owned, panic and memory controls; resolve or individually accept serial regressions without changing public bounds. See [candidate evidence](docs/design/evidence.md#repeated-key-root-sharing).

## Python binding

An independent track binding the Rust crate; see the [design](docs/design/python-binding.md). It needs its own publication workflow, interpreter/ABI matrix and wheel install tests when it starts, not inactive CI.

## C++

1. **C++ runner.** Measure the C++ implementation with the shared input specification and native-buffer timing. Add cross-language golden hashes in `conformance/vectors/` and verify them in both generators.
2. **C++17 header-only implementation** for trivially copyable records, with CMake install and a consumer test. Done when it passes the shared vectors and has a direct comparison with Rust.

## Benchmark tooling and investigations

- Finish the [CI scaling proposal](docs/design/ci-scaling-proposal.md). Cache hygiene, lane reuse and continuous operation cases are implemented. Next come per-operation cross-implementation plans.
- Add the parallel argsort composition baseline under the [comparison model](docs/design/comparison-model-proposal.md#plan) once Tessera has a parallel argsort entry point.

- Fix the sample duration and prepared-memory cap of local paired sessions from the test machine's memory and the time a 10M-record case takes.
- Investigate benchmark build invalidation without weakening provenance. Missing watched paths can force repeated rebuilds, but the existing watches do not cover every file that can affect Git dirty state. A solution must reliably detect dirty-state changes outside the benchmark source trees, optional revision file transitions and packed or detached Git references. Validate actual Cargo builds and snapshot workflows before changing the watches.
- Study monomorphization work in the optimized smoke harness before changing generic engine boundaries: LLVM IR inspection showed repeated `Partition::adopt_shards` instances across record and extractor types (adoption is since retired; the ledger is generic over the record type only). IR size alone does not establish compile-time savings or justify losing inlining; any extraction needs compile-time measurements and the engine performance and ownership controls.

- Activate [main long-term tracking](docs/development/continuous-benchmarks.md#main-long-term-tracking) only after a complete Bare Metal pilot confirms static-image execution controls, matching complete Criterion results, timeout headroom, Git publication and browser loading. Repeat-run noise is required before adding historical alerts; image-registry retention and long-term Git growth need operational monitoring.

- Calibrate a structural acceptance lane before changing the per-case rule ([rationale](docs/design/evidence.md#structural-acceptance-needs-calibration)). Define structural scope as unchanged routing, classification, scan/pass counts, allocation policy and scheduling; SIMD, capacity, MSD-only finishing and recursive parallelism need timing evidence even if they delete code. Test fixed-work counters and movement/allocation models against same-source layouts and known regressions, retaining timing controls for cache, ISA and contention effects. Done when the maintained pipeline can report applicable coverage, uncertainty and each unresolved loss, and the maintainer approves the decision rule in the benchmark protocol. Until then counters remain descriptive, and unresolved timing needs an explicit per-case decision rather than an automatic waiver.
- Validate the [optional local AOT CLI](docs/development/benchmarking.md#optional-local-aot-cli) beyond offline commands: compare real preparation provenance, frozen binary identity and workspace cleanup against direct Bun, exercise interruption and recovery through the compiled entry point, and measure larger current-format real reports separately from fresh-process startup. Keep stable Bun direct execution as the default, including in CI; do not distribute AOT through Actions or Release. Broader AOT adoption needs this evidence and platform coverage.
- Let the Rust comparison manifest own the expected memory-case inventory, consumed by measurement and offline completeness validation, keeping timing/memory process isolation.
- Evaluate one normalized worker request instead of parallel TS/Rust user-facing CLI rules, keeping timed calls, inputs and recovery; validate any Criterion removal with code-placement controls.
- Expose structured current-head performance details through `status` from the check publisher's machine summary, not by parsing report wording.
- Validate the hosted paired pipeline on Actions: cold/warm full-plan duration, queue wait, paired median intervals, same-source false alerts and held-out detection for the required backend, before claiming hosted stability or tuning sample budgets; verify direct PR Cloud upload, fork/Dependabot summary-only behavior and partial reruns. An earlier calibration (PR #96, run `42904693411`) had no same-source alerts over 70 cases per configuration with measurement jobs of 346–406 s, which does not establish long-term rates.
- Retain raw paired evidence beyond Actions' 14-day artifacts if longer-term offline regeneration is needed.
- Investigate near-order, sparse-digit and skewed-key deficits with general mechanisms, not per-distribution tuning, against current-head continuous feedback.
- Add cold-cache experiments, more seeds and threshold cases, and native-CPU placement controls for source A/B studies (existing CGU, alignment and engine-order screens are process-level). Validate alignment's dispersion across hosted CPU models; revisit multi-layout sampling only with held-out false-alert, detection and whole-job cost evidence.
- Accept manual implementation tracking: run Bare Metal pilots for every production adapter, especially the slowest wide/random and adverse cases, and confirm each implementation's single remote job completes within 300 seconds with headroom. Verify native runtime images, glidesort-sqrt scratch evidence, per-implementation Git publication and browser loading. Workflow plumbing and local functional checks alone do not satisfy remote timeout acceptance. Populate the [implementation comparison](docs/benchmarks/implementations.md) with a run of every opponent after memory tracking lands, and check that its cohort covers the opponents' u64 inputs.

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
