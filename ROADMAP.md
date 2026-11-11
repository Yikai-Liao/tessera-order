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

The open structural question is whether the engine's decomposition adds complexity that another decomposition would remove, not whether its line count is necessary. A mechanism counts when it adds a rule, a state or an invariant; tests, comments and safety arguments are not reduction targets. Necessity is relative to two fixed inputs: the [space contract](conformance/semantics.md#space), whose parallel O(P√n) bound is the ceiling (maintainer decision, 2026-11-11), and the per-case acceptance rule of the [simplification candidates](#simplification-candidates). A mechanism is necessary when no more general mechanism keeps its saving within that rule. The rule therefore decides which adaptive paths survive; replacing it, for example with a rule over input classes or an aggregate, is a maintainer decision that would reopen those paths, and no work below changes it.

Current reading: the [simplification series](https://github.com/Yikai-Liao/tessera-sort/pull/97) retired duplicated mechanisms, the remaining serial candidates each retire one local duplicate or shortcut, and the first candidate to remove an adaptive state, R2, was [rejected](docs/design/evidence.md#rejected-or-on-hold) for constant-input losses above the rule. The serial structure is therefore plausibly near necessary under the rule, but no audit has shown it. The largest open structural question is parallel completion, where two protocols move record ownership differently after one shared scatter. A from-scratch minimal engine is not planned: it would price only the adaptive paths, which the rule already prices candidate by candidate, and would not locate a boundary. Whole-call timing cannot rank structures on its own either, because the eight-core [parallel A/A ranges](docs/design/evidence.md#reading-small-case-results) are wider than the expected differences; structural comparisons fall back on modeled work and [fixed-work counters](docs/development/benchmarking.md#fixed-work-counters).

New work proceeds in this order: the [mechanism audit](#mechanism-audit), the [parallel completion study](#parallel-completion-study), then the [simplification candidates](#simplification-candidates) in their listed order; R3 continues in [PR #107](https://github.com/Yikai-Liao/tessera-sort/pull/107). The audit and the study change no engine code until a design is chosen, and no candidate is a prerequisite of either. [Acceptance debt](#acceptance-debt) stays open alongside them, and [Rayon](#rayon) coverage on further hosts waits for the study's completion design.

### Acceptance debt

The consolidated engine (shared root preparation, node planning and order confirmation; one half-key extractor; shared direct-key counting; segmented scratch; bounded tile planner; single SIMD backend) has measured losses that no exception compensates for; they are listed under [landed with open losses](docs/design/evidence.md#landed-with-open-losses). Done when the shared paths are validated against the pre-consolidation baseline with A/A and placement controls, the open losses are resolved or individually accepted, and a fresh current-head Actions run passes preparation, all four measurement shards, Cloud archival and publication. Raw artifacts alone do not satisfy publication acceptance.

### Mechanism audit

The audit answers how much of the engine is necessary under the acceptance rule. It reads code and documents only, with no build or measurement. Its unit is a mechanism: a state, route, protocol or retained representation that a change would add or retire, such as `Order::Flat`, `plan_progress` or the owner-lock protocol, not a function or a file.

For each non-test mechanism, record its group, its owner and what it buys. The groups are the contract floor (the stable block partition, tile passes, record ownership and recovery that the guarantees require), adaptive paths (routes that exist for a measured saving) and parallel completion. The owner follows the [ownership table](docs/README.md#who-owns-what): a guarantee in semantics, a rule on an explanation page, a boundary or invariant in the engine reference, a rejected alternative or open loss in engine evidence, or an accepted effect recorded only in a PR description or code comment. Then trace the representative changes named in [`AGENTS.md`](AGENTS.md#engine-design-and-simplification) (sampling, tile capacity, partition selection, block layout and task size) and count the files, invariants and rule interpreters each one touches.

A rule that a trace finds interpreted in more than one place enters the candidate table. A mechanism with no owner needs documentation, and enters the table only with a named general replacement. If the serial groups yield no entry, the serial structure is near necessary under this rule: every serial mechanism has an owner and every traced change meets one interpreter per rule. Structural search then continues only in the parallel completion study.

Done when the classification and traces are in the audit PR description, and that PR documents the missing owners and updates the [candidate table](#simplification-candidates), with new rows or with the near-necessary verdict.

### Parallel completion study

The two [parallel completion](docs/design/engine.md#parallel-root-completion) protocols share the root scatter and directory adoption, then move record ownership differently: materialization makes a bucket contiguous before sorting it, while bounded staging sorts it elsewhere and then evacuates unread sources from its final band. Unifying their callbacks, configuration or scratch owners removes neither protocol. The study asks when a bucket must become a contiguous, exclusive slice and how ownership moves with that decision, and keeps or retires each protocol by evidence, not by line count.

Fixed constraints: the parallel [space bound](conformance/semantics.md#rust-rayon-entry-points) is the ceiling, so a design that needs an n-record buffer is out of scope. Staging meets that bound only by treating the root fan-out R as a constant: reentrant key callbacks can suspend up to R staged buckets, and staging runs only when R buckets at its limit cover the input, so their total can approach the input size. A candidate must not widen this gap or assume one live buffer per worker. Every guarantee in semantics holds unchanged, notably `Send`-only records, the `Fn + Sync` key function, panic restoration and termination under inconsistent keys.

Known facts: in the [baseline](docs/design/evidence.md#parallel-completion-baseline), staging's advantage passed the controls only for random `u32`, materialization ends in a sequential compaction chain, and no measured length lies above the [staging window](docs/design/engine.md#bounded-staging). Above that window, or when any root bucket exceeds the staging limit, materialization is the only route; splitting an oversized bucket by position does not sort it without a further key partition or a stable merge. PR feedback reaches neither case, because the [continuous plan](bench/plans/continuous.toml)'s parallel cases stop below the window.

Candidates, none established:

- **A. One contiguity mechanism.** Keep materialization and remove its sequential chain. After the parallel block permutation, the remaining move is a monotone left shift by the partial-block slack accumulated before each record, so segments could shift in parallel once each boundary snapshots the records its successor overwrites, as split permutation cycles already snapshot their boundary blocks. A boundary's snapshot equals the shift distance there, up to the root's total slack, so the snapshot total grows with the segment count and must be bounded within the space ceiling. Buckets must still reach jobs as soon as compaction releases them. If parallel materialization then keeps staging's measured saving, staging's owner locks, relocation and free-block discovery retire.
- **B. Scattered buckets through recursion.** Partition a bucket's block chain without materializing it, since the scatter already reads a logical block sequence, and gather only tile-sized leaves. The block layer does not support this today: an adopted directory only compacts or finishes and never scatters again, and a pass takes the free-prefix length as its bucket count, while an adopted prefix holds one block per bucket per shard. B therefore needs a per-bucket free reserve and buckets with several partial blocks. Each uncompacted partition also leaves up to one partial block per child, and that slack compounds with depth, so model fragmentation and directory size against the space ceiling before prototyping. If the slack must be reclaimed by compaction, B becomes a variant of A.
- **C. One protocol, two strategies.** If both movement strategies keep distinct measured savings, they share one ownership and recovery protocol instead of two.

Steps:

1. Measure on the [baseline host](docs/design/evidence.md#parallel-completion-baseline), with peak heap. Inside the staging window, run both protocols at the baseline's equal root plans. Above the window, and on a skewed root whose largest bucket exceeds the limit, only materialization runs; time its block permutation, sequential chain and leaf jobs separately. Large-input runs need the maintainer's approval of the measurement budget.
2. Extend the audit PR's parallel completion rows with the work each mechanism avoids and whether it survives each candidate.
3. Model A's parallel shift with its snapshot total, and B's fragmentation and directory size; prototype only candidates whose model stays within the space ceiling.
4. Prototype on an unmerged branch, covering large, small and skewed buckets, non-`Copy` and `&mut` records, key panics, and inconsistent and reentrant key functions.

Done when one candidate retires a protocol while keeping its saving within the controls, or when each candidate's blocking cost is recorded in [engine evidence](docs/design/evidence.md) and both protocols stay as a measured trade-off. Compare independent protocols and invariants, full passes and copies, metadata peak, suspended buffers and whole-call time, and list each loss individually as for the candidates below. Whole-call differences inside the eight-core parallel A/A ranges are not detectable; compare those cases by modeled copies and the sequential critical path, and serial effects with fixed-work counters. A chosen design lands in stages after the study, with structural commits separate from routing changes.

### Simplification candidates

Local cleanups that replace a specialized mechanism with a more general one, ordered by expected value for risk. Each retires one mechanism and none settles the structural questions above; the [mechanism audit](#mechanism-audit) adds rows or records that the serial structure has none left. A candidate lands when one mechanism replaces at least two, the retired fast path's saving is kept or each loss is listed and accepted individually (up to 10% locally, never a performance budget), and structural and routing changes are separate commits under the [benchmark protocol](docs/development/benchmarking.md#run-an-experiment). Evaluate one candidate per PR against the preceding merged head, with three-block screens, and do not claim source causality without placement controls. Rejection completes an evaluation; it does not permit removing the saving another way without evidence.

| ID | Retire | Replace with | Risk |
|---|---|---|---|
| R3 | Three copies of "ascending returns, descending reverses" | One `finish_ordered` that keeps the flat route ([R2 rejected](docs/design/evidence.md#rejected-or-on-hold)) | Inlining of the root path; in review as PR #107 |
| R4 | The `INSERTION_MAX` root shortcut beside the budgeted probe | One insertion attempt owning its budget | Known short `random/1000` losses; measure first |
| R5 | `Descending { strict }` | Always reverse stably | One extra scan on `reverse`; likely rejected, listed to measure once |
| N1 | `plan_progress` and the equality fallback in the executor | A plan that carries its progress guarantee | Keep the inconsistent-key termination test |
| N2 | `two_runs`, `NodePlan::Runs`, `split_ordered` | A block partition that detects non-decreasing bucket order and returns without moving | Loses the rotated `1*0*` form; `sorted_tail`, `bimodal`, `far_swaps`; speculative |
| N4 | `partition_by` beside `partition_digit` | One entry taking a classifier and bucket count | A `&mut` classifier measured 40% slower; keep the closure owned |
| X1 | `try_count_window_parallel` beside `try_count_window` and its `KeyHalf` forwards | One counting operation receiving the shard count | Flag-shaped; serial must keep no Rayon call |
| X2 | Two expressions of "the record is its own key" | One extractor fact | `KeyHalf` over `Direct` must keep both |
| X3 | Duplicated half-key dispatch in serial and parallel entries | One root helper, or `KeyHalf` over the narrowest covering unsigned type | The second is a routing change for narrow 64-bit variation |
| X4 | `finish_insertion` beside `insertion_sort(usize::MAX)` | One entry | Measured micro-choice on short tiles |
| T1 | Two homes for cached bucket IDs | Always the scratch tail | An extra cache line set per by-key pass on small tiles |
| P2 | `Config`, `BucketFinish` and `BoundedFinish` describing one root partition | One description consumed by the block layer | Follows the parallel completion study, which decides what the description holds |
| P3 | `tile::Scratch`, `block::Buffers`, job `Pool` | One scratch owner per worker | Follows the study; check allocation counts in the memory tests, and do not assume one live buffer per worker under reentrant callbacks |

Order: R3 and R4; then N1, X1, X2, N4; then X4, T1, X3; then speculative R5 and N2; P2 and P3 after the parallel completion study.

### Rayon

The optional feature and bounded completion are implemented ([semantics](conformance/semantics.md#rust-rayon-entry-points), [engine reference](docs/design/engine.md#parallel-root-completion), [baseline](docs/design/evidence.md#parallel-completion-baseline)). Done when whole-call scaling and memory coverage extend to other hosts, wide records and expensive extraction, with source-attribution controls for causal claims.

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
