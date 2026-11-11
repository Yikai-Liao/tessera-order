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

The [structure analysis](docs/design/evidence.md#structure-analysis) states the open structural question, its yardstick and the current reading: local serial duplication has converged but minimality is not shown, and parallel completion is the largest open question. The [mechanism audit](docs/design/evidence.md#mechanism-audit) is complete. New work proceeds with the [parallel completion study](#parallel-completion-study), then the [simplification candidates](#simplification-candidates) in their listed order; R3 continues in [PR #107](https://github.com/Yikai-Liao/tessera-sort/pull/107). The study changes no engine code until a design is chosen, and no local candidate is its prerequisite. [Acceptance debt](#acceptance-debt) stays open alongside them, and [Rayon](#rayon) coverage on further hosts waits for the study's completion design.

### Acceptance debt

The consolidated engine (shared root preparation, node planning and order confirmation; one half-key extractor; shared direct-key counting; segmented scratch; bounded tile planner; single SIMD backend) has measured losses that no exception compensates for; they are listed under [landed with open losses](docs/design/evidence.md#landed-with-open-losses). Done when the shared paths are validated against the pre-consolidation baseline with A/A and placement controls, the open losses are resolved or individually accepted, and a fresh current-head Actions run passes preparation, all four measurement shards, Cloud archival and publication. Raw artifacts alone do not satisfy publication acceptance.

### Parallel completion study

The study decides when a bucket must become a contiguous, exclusive slice and how ownership moves with that decision, and keeps or retires each [parallel completion](docs/design/engine.md#parallel-root-completion) protocol by evidence. Its premises and the candidate models are in the [structure analysis](docs/design/evidence.md#structure-analysis): one ownership protocol with two movement strategies (C), one contiguity mechanism (A), or scattered buckets through recursion (B).

Fixed constraints: auxiliary space stays within the [structure analysis](docs/design/evidence.md#structure-analysis)'s O(P^{3/2}√n) ceiling, so a design that needs an n-record buffer is out of scope. A candidate must not widen staging's gap under reentrant suspension or assume one live buffer per worker. Every guarantee in semantics holds unchanged, notably `Send`-only records, the `Fn + Sync` key function, panic restoration and termination under inconsistent keys.

Steps form a research order, not gates; a model that meets a clear blocker is recorded and stops.

1. Measure on an available host, recording its environment and peak heap alongside the models. The [historical baseline](docs/design/evidence.md#parallel-completion-baseline) supplies comparison context, not a required machine. Inside the staging window, run both protocols at equal root plans. Above the window, and on a skewed root whose largest bucket exceeds the limit, only materialization runs; time its block permutation, sequential compaction and leaf jobs separately. Large-input runs need the maintainer's approval of the measurement budget.
2. Extend the audit's parallel completion rows with the work each mechanism avoids and whether it survives each candidate.
3. Model C, from the audit: whether one ownership, recovery and completion-state protocol can serve both movement strategies and delete one protocol's responsibilities.
4. Model A: auxiliary space, extra record moves and critical path, for independent and dependent segments, with P growing with the pool and R bounded by the root digit cap; a measured configuration does not establish a bound.
5. Model B: partial blocks, free reserves, directories and recovery metadata over the active frontier of live subproblems.
6. Prototype on an unmerged branch only a candidate whose model holds the space ceiling and deletes mechanisms, covering large, small and skewed buckets, non-`Copy` and `&mut` records, key panics, and inconsistent and reentrant key functions.

Done when one candidate retires a protocol while keeping its saving within the controls, or when each candidate's blocking cost is recorded and both protocols stay as a measured trade-off; either way the result replaces the candidate models in the structure analysis. Compare independent protocols and invariants, full passes and copies, metadata peak, suspended buffers and whole-call time, and list each loss individually as for the candidates below. Whole-call differences inside the eight-core parallel A/A ranges are not detectable; compare those cases by modeled copies and the sequential critical path. A chosen design lands in stages after the study, with structural commits separate from routing changes.

### Simplification candidates

Local cleanups that replace a specialized mechanism with a more general one, ordered by expected value for risk. Each retires one mechanism and none settles the structural questions above; the [mechanism audit](docs/design/evidence.md#mechanism-audit) records the cross-module alternatives and adds N3 for shared equality classification. A candidate lands when one mechanism replaces at least two, the retired fast path's saving is kept or each loss is listed and accepted individually (up to 10% locally, never a performance budget), and structural and routing changes are separate commits under the [benchmark protocol](docs/development/benchmarking.md#run-an-experiment). Evaluate one candidate per PR against the preceding merged head, with three-block screens, and do not claim source causality without placement controls. Rejection completes an evaluation; it does not permit removing the saving another way without evidence.

| ID | Retire | Replace with | Risk |
|---|---|---|---|
| R3 | Three copies of "ascending returns, descending reverses" | One `finish_ordered` that keeps the flat route ([R2 rejected](docs/design/evidence.md#rejected-or-on-hold)) | Implemented in PR #107; [short-input controls](docs/design/evidence.md#shared-ordered-finishing-controls) remain unresolved |
| R4 | The `INSERTION_MAX` root shortcut beside the budgeted probe | One insertion attempt owning its budget | Known short `random/1000` losses; measure first |
| R5 | `Descending { strict }` | Always reverse stably | One extra scan on `reverse`; likely rejected, listed to measure once |
| N1 | `plan_progress` and the equality fallback in the executor | A plan that carries its progress guarantee | Keep the inconsistent-key termination test |
| N3 | Below/equal/above classification in serial equality partition and the parallel rank closure | One equality classifier | Preserve cached ranks, payload ownership and inconsistent-key progress; found by the mechanism audit after PR #78 |
| N2 | `two_runs`, `NodePlan::Runs`, `split_ordered` | A block partition that detects non-decreasing bucket order and returns without moving | Loses the rotated `1*0*` form; `sorted_tail`, `bimodal`, `far_swaps`; speculative |
| N4 | `partition_by` beside `partition_digit` | One entry taking a classifier and bucket count | A `&mut` classifier measured 40% slower; keep the closure owned |
| X1 | `try_count_window_parallel` beside `try_count_window` and its `KeyHalf` forwards | One counting operation receiving the shard count | Flag-shaped; serial must keep no Rayon call |
| X2 | Two expressions of "the record is its own key" | One extractor fact | `KeyHalf` over `Direct` must keep both |
| X3 | Duplicated half-key dispatch in serial and parallel entries | One root helper, or `KeyHalf` over the narrowest covering unsigned type | The second is a routing change for narrow 64-bit variation |
| X4 | `finish_insertion` beside `insertion_sort(usize::MAX)` | One entry | Measured micro-choice on short tiles |
| T1 | Two homes for cached bucket IDs | Always the scratch tail | An extra cache line set per by-key pass on small tiles |
| P2 | `Config`, `BucketFinish` and `BoundedFinish` describing one root partition | One description consumed by the block layer | Follows the parallel completion study, which decides what the description holds |
| P3 | `tile::Scratch`, `block::Buffers`, job `Pool` | One scratch owner per worker | Follows the study; check allocation counts in the memory tests, and do not assume one live buffer per worker under reentrant callbacks |

Order: R3 and R4; then N1, X1, X2, N4; then X4, T1, X3; then speculative R5 and N2; then N3; P2 and P3 after the parallel completion study.

### Rayon

The optional feature and bounded completion are implemented ([semantics](conformance/semantics.md#rust-rayon-entry-points), [engine reference](docs/design/engine.md#parallel-root-completion), [baseline](docs/design/evidence.md#parallel-completion-baseline)). Done when whole-call scaling and memory coverage extend to other hosts, wide records and expensive extraction, with source-attribution controls for causal claims.

Repeated-key parallel roots (PR #78): validate the sharded equality route against main with four/eight-thread, random/ordered, wide/owned, panic and memory controls; resolve or individually accept serial regressions without changing public bounds. See [candidate evidence](docs/design/evidence.md#repeated-key-root-sharing).

## Python binding

An independent track binding the Rust crate; see the [design](docs/design/python-binding.md). It needs its own publication workflow, interpreter/ABI matrix and wheel install tests when it starts, not inactive CI.

## C++

1. **C++ runner.** Measure the C++ implementation with the shared input specification and native-buffer timing. Add cross-language golden hashes in `conformance/vectors/` and verify them in both generators.
2. **C++17 header-only implementation** for trivially copyable records, with CMake install and a consumer test. Done when it passes the shared vectors and has a direct comparison with Rust.

## Benchmark tooling and investigations

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
