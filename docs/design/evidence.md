# Engine evidence

This page keeps only what code and Git history cannot: how to read a small-case result, which experiments failed or stalled and must not be repeated blindly, which measured losses landed changes still carry, and how the engine's structure was judged. The full measurements of each accepted change belong in its PR description; commands, hashes and raw-result paths belong with the raw data outside Git. Entries describe their recorded revision and environment, not current-head validation. Use the [benchmark protocol](../development/benchmarking.md#run-an-experiment) for new measurements, and delete an entry when a change makes it obsolete.

## Reading small-case results

Process-level and three-block screens flag changes to investigate; only linked in-process comparisons with independent copies separate a source change from code placement, and even those do not bound all layouts. Identical instructions at another address moved short inputs by 20–90%:

| Investigation | Observation | Conclusion |
|---|---|---|
| Port vs prototype, ordered `u32`/`u128` | Ratios 0.53–1.28 in the measured build, 0.79–1.86 after a comment-only rebuild, 0.98–1.06 with `-x86-branches-within-32B-boundaries` | Skylake-SP keeps branches crossing 32-byte boundaries out of the µop cache (JCC erratum). Redirecting six identical probe functions to one address removed the `u32` gap |
| Low-fanout tiles (`e0ecd94` → `54926ca`), `random/48` | Identical copies differed by 21–22% against A/A of ±1.4–3.8%; moving the candidate's `sort_tile` body 160 bytes to the baseline address took 282–287 ns/sort back to 243–247 (baseline 237–243) | Tile code placement; the Intel runner's mechanism is unidentified |
| Serial CI artifacts (`fafaaa2` → `a26cac4`), `row_u64/equal/48` | Alert +6.9%; candidate/baseline 1.0231 against A/A 0.9672–1.0328; both executables ran the same 348 instructions | Placement sensitivity; a small whole-binary cost cannot be excluded |
| Eight-core parallel | Same-entry A/A 0.80–1.20 with inherited masks, 0.90–1.10 pinned, 0.78–1.22 on the pinned repeat | No stable detection window for a 5–10% source gain on this host |

Branch alignment is not a free speedup: it changed the prototype's own time by −30% to +69% across small cases. Stable Rust has no attribute for function or branch alignment, so a source change that helps one binary can hurt another. The crate has nothing to fix; benchmarks control placement ([source-attribution controls](../development/benchmarking.md#additional-source-attribution-controls)), and CLI builds default to 64-byte loop and block alignment.

## Port parity baseline

The engine is a Stable Rust port of the research prototype in `sort-research-rs` at `6c0877f0491dc3b8fc052aa529bce44bf8a53fa8`. Measured inside one Nightly binary of the prototype's harness (fixed seed, one pinned core, rotated order, every output checked), the median per-case ratio `port / prototype` was 0.984–1.004 over 27–136 cases per matrix (ranges 0.53–1.23, placement-dominated), for both `stable_sort_by_key` and `stable_sort`, in the default build and with `target-cpu=native` and fat LTO. Cases outside the control range were within 3% of the prototype forced onto cached bucket IDs (`record128` zipf 1.3 n = 128: 1.168 against 1.030), the cost of the caching policy the port adopts on purpose ([record handling](engine.md#record-handling)); its cost on 8-byte rows is an open [roadmap](../../ROADMAP.md#rust-01) question. The harness is Apache-2.0 code and stays outside the repository.

## Rejected or on hold

- **Flat samples as ordinary order candidates (R2), rejected.** Removing `Order::Flat` against `ef6c174` removed a state and a root-only branch, but constant inputs lost the vectorizable union/intersection scan: `u64/equal/65536` 0.839 / 0.834 and `row_u64/equal/65536` 0.925 / 0.921 (both baseline placements, A/A 0.994–1.006; `hot50`, `runs`, `two_values` neutral). Hosted [PR feedback](https://github.com/Yikai-Liao/tessera-sort/pull/105#issuecomment-6797484301), without placement controls, measured serial `u64/equal/65536` +70.0% and four-thread `u64/equal/1000000` +82.0%. Both hosted losses exceed the 10% limit, and [PR #105](https://github.com/Yikai-Liao/tessera-sort/pull/105) closed unmerged. A replacement must keep the constant-input scan; whether the loss is confined to all-equal inputs is unproven.
- **First-distinct-key order confirmation.** Removed more source but regressed 1,000-record constant inputs by 14–29%.
- **Ascending-first `confirm_order` reordering.** +6.6% instructions and 8–13% slower on `f64/sorted/65536`; reverted.
- **Streaming scalar scan for 99%-sorted budgeted insertion.** `u64/sorted99/65536` 1.2879 against A/A 0.989–1.011.
- **Removing the histogram maximum and testing insertion eligibility from bucket ends.** Matched the instruction saving but regressed `random/49` by 11–24% and `u64/random/1M` by about 14%.
- **Out-parameter for strict descent.** 2.4–5.3% slower on `u64/equal/65536` and `{u32,u32}/random/49`; strictness travels in the returned classification.
- **Early insertion finish for tiny inputs.** Regressed other small inputs.
- **Sample-weighted child work in tile planning.** Slower on `bimodal/1000` (1.316, then 1.057 with recursive projection) and `random/1000` (+2.7–3.5%) with +3–10% instructions; replaced by the bounded planner.
- **Parallel micro-tuning.** Index-only leaves and indexed final writes (14.8–18.4 ms against an 11.6 ms baseline), a frozen-directory wave (15.3 ms), cache-line-padded owners (inconclusive, A/A 0.909–1.091), source-aware free-block placement (relocation volume −15%, timing unverified) and a sorted free list. Final-scatter fusion **A** removed one 80 MB transfer per 80 MB input, and **B** also capped LSD digits at eight bits; ratios against the baseline were 0.982–1.102 (A) and 0.957–1.254 (B, slower on `u32`) against parallel A/A of 0.80–1.20. None entered the engine, and radix, final-scatter and relocation tuning is paused.
- **SIMD constraints** (AVX2, interleaved separate binaries, no paired protocol):
  - Dispatch belongs around the insertion pass. Per-scan dispatch cost more than it saved on random input, and a kernel outside its target-feature inlining context was 3.5× slower on sorted `u64` and 7× on sorted `f64`.
  - Materializing user keys on an 8,192-record near-sorted tile was 1.4–1.6× slower than scalar callbacks, so user-key tiles keep callbacks.
  - A sixteen-lane prefix sum lost to eight lanes (cross-lane swizzle).
  - Stable three-way compress scatter of 64K `u32` took 460 µs against 147 µs for scalar cursor writes, and AVX-512CD conflict detection would double the 32 KiB cursor table.

## Shared ordered finishing controls

The R3 `finish_ordered` refactor keeps the flat-sample route and adds no scan, allocation or cached classification. Its short-input linked screens did not establish stable latency controls: at 48 records, `row_u64/equal` had baseline/current ratios 0.797 / 0.823 against A/A 0.764–1.236, `row_u64/sorted` 0.898 / 0.814 against 0.770–1.230, `row_u64/random` 1.043 / 0.927 against 0.996–1.004, and `u64/equal` 0.961 / 0.966 against 0.968–1.032. Increasing the sample duration did not stabilize the controls: row equal A/A widened to 0.659–1.341, reverse to 0.871–1.129 with current-copy median 1.164, and sorted baseline ratios were 0.950 / 0.978 against 0.975–1.025. Fixed-work row counts matched; `u64` added 0.30–0.56% instructions. These controls remain unresolved and do not establish source causality. Current-head hosted feedback and accepted effects belong in [PR #107](https://github.com/Yikai-Liao/tessera-sort/pull/107).

## Landed with open losses

These changes are in the engine; each measured loss below stays open and no distribution or size exception compensates for it.

| Change | Gain that justified it | Open loss |
|---|---|---|
| Bounded tile planner (against `3db3860`) | `random/49` 1.6–1.9× faster than main | `row_u32/random/65536` 0.977 / 0.982, `u64/bimodal/65536` 0.977 / 0.990, `u32/sparse_digits/65536` 0.959 / 0.961 (A/A ≤ ±1.5%); `u64/bimodal/49` current B 0.806 |
| Shared planning and required SIMD (`b8968b6` → `383ddb7`) | `d16` 0.88–0.89 | `random` 1.035 (`u64`) / 1.047 (`row_u64`), cause unidentified |
| Shared order confirmation (against `28e6a96`) | `sorted/1000` 1.36 (`u64`) / 1.55 (`f64`); descending −24.8% / −19.8% instructions at 1,000 records | `u64/sorted/65536` 0.958 / 0.961, `f64/sorted/65536` 0.983 (+29.5% / +5.3% instructions in the AVX2 inversion scan, whose code changed with inlining) |
| Candidate-masked root and child scans (against `ce71415`) | One scan implementation | `row_u64/sorted_tail/49` 0.804 / 0.800, A/A 0.855–1.145 |
| Counting-pass inlining | `u32/random/1M` −11.45% instructions, −14.42% reads | `row_u64/random/1000` 0.954 / 0.950 |
| Scalar insertion finishing (against `26b340c`) | Random `u64` 0.94–0.97 (8,193 to 1M) | `u64/sorted_tail/65536` 1.0219 / 1.0108 |
| Ordered children and bucket-order split (against `8bd64d9`) | `sorted_tail/65536` 0.333; scratch 8.7 MB → 65,536 B | `d16/65536` 1.1214 (tracks placement; Portable SIMD −1.1%) |

Neutral or already accepted without a recorded loss: strictly descending reversal (1.24–1.56× on reverse inputs), direct finishing of insertion-sized roots (random `u64` 0.889, rows 0.943), the active block directory (0.998–1.001) and terminal parallel completion (0.9965). Parallel adoption latency was not measured.

## Repeated-key root sharing

PR #78 remains unaccepted. Its [pre-simplification paired screen](https://github.com/Yikai-Liao/tessera-sort/actions/runs/43033277279) (`c2c84d5` against `c168df3`) found serial `u64/random/49` and `u64/random/1000` 15.2% and 12.7% slower despite unchanged serial source; layout is a hypothesis, not an established cause. The smaller closure-based candidate requires fresh exact-head feedback and source-attribution controls. Historical dual-build and 10M gains remain in the [PR evidence](https://github.com/Yikai-Liao/tessera-sort/pull/78#issuecomment-6755255766), not current acceptance. The earlier cache-lock batching candidate was rejected.

## Parallel completion baseline

Host: AMD Ryzen 7 8845H, CPUs `0,2,4,6,8,10,12,14` (one SMT sibling per core), `powersave`, boost and SMT on, Rust 1.99.1, release, `compare,rayon`. These are whole-call comparisons against the serial A/A pair, not source-attributed gains; the implementation is in the [engine reference](engine.md#parallel-root-completion). Speedup is the reciprocal of the paired median ratio; ranges span nine blocks; peak heap excludes input preparation, the pool and thread stacks.

| 10M input | Eight-worker speedup | Block range | Serial A/A | Parallel peak heap |
|---|---:|---:|---:|---:|
| Random `u32` | 6.01× | 4.86–6.62× | 0.981–1.019 | 12,076,304 B |
| Random `{u32, u32}` | 4.58× | 4.32–4.77× | 0.987–1.013 | 18,909,498 B |
| `d256` `u32` | 1.97× | 1.59–2.15× | 0.995–1.005 | 65,536 B |
| `d256` `{u32, u32}` | 1.83× | 1.79–1.89× | 0.976–1.024 | 15,757,602 B |
| Random `u64` | 3.80× | 3.59–4.12× | 0.996–1.004 | 18,275,618 B |
| Random `{u64, u32}` | 2.95× | 2.79–3.17× | 0.994–1.006 | 29,635,068 B |

Fivefold scaling is demonstrated for random `u32` only. Zipf2 measured 0.96–1.00×, sorted 0.42–0.89× and reverse 0.87–0.94× (these adaptive paths allocate nothing and skip bounded completion). Random `u32` at 4M / 8M / 16M measured 4.09× / 6.23× / 5.56× and 8-byte records 4.76× / 4.56× / 4.09×. Blocks at two and eight times the balanced size gave 4.22× and 4.35× on 8-byte records, so the factor-four layout stayed. Sorted `row_u32` measured 1.41× in one executable and 0.42–0.43× in the next although both return from the order probe before partitioning; the cause is unresolved.

Where the time goes (`7ab58c9`, random 10M): relocation moved 0.76–0.79 of the input bytes (about 40% re-relocation) and owner-lock contention was negligible; the root scatter reached about 39 GB/s of logical payload traffic against STREAM references of 35 GB/s (cached stores) and 56 GB/s (non-temporal copies), and no reliable DRAM-roofline distance was established. Of sampled cycles, 39% were in the tile sorter, 23% in root scatter, 26% in copies and 10% in bucket finishing; 28% of tile stores missed L1, and the twelve-bit second pass spreads 256 KiB of active lines over a 32 KiB L1, so cache-level locality is the open hypothesis.

Bounded staging against materialization (block permutation plus compaction) at equal root plans, eight workers, 10M random, median ms:

| Input | Bounded | Materialized | Materialized / bounded (paired) |
|---|---:|---:|---:|
| `u32` | 7.7 | 11.1 | 1.33 |
| `{u32, u32}` | 14.0 | 16.8 | 1.15–1.19 |
| `u64` | 18.3 | 17.0 | 0.97–0.98 |
| `{u64, u32}` | 35.5 | 31.7 | 0.86–0.91 |

Only the `u32` direction passed the control rule in both copies, so no single record width selects the better strategy. The materialized path ends with a sequential compaction chain (7.4 ms at eight workers) that concurrent leaf jobs overlap but cannot remove; bounded gather, leaf sorting and protected writes overlap across workers (wall unions 7.6, 8.5 and 7.3 ms, not additive). Every length above lies inside the [bounded-staging window](engine.md#bounded-staging); no measurement covers a uniform root above it, where materialization is the only route. New completion research should start from these complete-path controls before adding ownership complexity.

## Structure analysis

Compare constructions by their rules, states and invariants, not source length. A replacement must meet [semantics](../../conformance/semantics.md) and the [per-case acceptance rule](../../ROADMAP.md#simplification-candidates); showing each mechanism useful alone does not prove minimality. The maintainer-approved study ceiling is O(P^{3/2}√n), with fixed root fan-out R; an n-record buffer is excluded. The public [O(P√n) bound](../../conformance/semantics.md#rust-rayon-entry-points) changes only if an accepted design needs it.

### Mechanism audit

The code-and-contract audit found concentrated ownership, not proven minimality. No additional serial construction is ready for a prototype. N3 tracks equality classification duplicated by #78, whose [performance acceptance](#repeated-key-root-sharing) remains open. Source paths below are relative to `rust/tessera-sort/src/`; documentation/model paths are under `docs/`.

| Change | Authority, consumers and invariant | Coordinated documentation/model edits |
|---|---|---|
| Sampling | `engine/plan.rs::KeySample` feeds equality/occupancy; samples certify neither ownership nor progress. `engine.rs::probe_order` samples adjacent order, a different fact. | `explain/routing.md`; `.vitepress/theme/sim/digits.ts::sampleKeys` and its sampling tests. |
| Tile capacity | `engine/tile.rs::capacity` converts byte/cursor limits for both entries and `plan_tile`. Layout changes follow `Cursor`, derived tables, `lib.rs::thresholds` and scratch reservation; `sort_staged_child` owns the cursor-bounded allowance. | `explain/tiles.md`, `sim/digits.ts`. |
| Partition selection | `plan_node` feeds `execute_node` and parallel `sort_root`. Both equality classifiers interpret below/equal/above (N3); the unchanged-node guard and `plan_progress` are N1. Preserve bucket meaning and shrinking records or candidate bits. | `explain/routing.md`; recovery/inconsistent-key tests. |
| Block layout | `block.rs` owns storage, descriptor costs, live prefixes and recovery; `block/parallel.rs::adopt_shards`, `compact.rs` and `finish.rs` consume them. Follow lookup, adoption, partials, relocation and restoration. Reserves are per shard/bucket; adoption charges both directories. | `explain/block-partition.md`, `sim/blockPartition.ts`, `sim/parallelFinish.ts`. |
| Task size | `engine/parallel.rs` owns cutoff, shard quota, batching and pooling; block code clamps shards and divides cycles. Preserve retired exclusive borrows and joins. Reentrant callbacks invalidate one-live-buffer-per-worker assumptions. | `design/engine.md`; bucket-borrow, parallel and memory tests. |

Here `sim/` abbreviates `.vitepress/theme/sim/`. Sampling, capacity and partition changes also follow their owning Rust tests; movement and scheduling do not reinterpret those policies.

**Ownership group.** `Partition` owns the adopted directory. Materialization snapshots cycle boundaries, tracks completed pieces, then compacts with an inverse map/free block; buckets are lent after their copy sources retire. Staging instead uses owner locks, atomic locations, free caches and private `Bucket` records. Recovery finishes bands before disarming the partition guard: staged/completed flags and `input_materialized` discharge different obligations. Permutation orders blocks, compaction removes slack, gather creates sortable ownership, and protected writes evacuate unread sources. Sharing callbacks deletes none of these obligations.

| Alternative | Mechanisms and ownership change | Moves, critical path and decision |
|---|---|---|
| C: one ledger, two strategies | Move post-adoption recovery to `finish.rs`, retiring cycle/compaction recovery. Adds relocatable-source state to materialization. | Gather/sort/write needs an oversized-bucket buffer. A streaming mover needs partial-source/resumable-write ownership; wrapping the old compactor leaves two protocols. Study this blocker first. |
| A: one contiguity mechanism | The independent-segment prototype replaced staging locks, locations, free discovery and recovery with source-suffix snapshots after block permutation. | Within the study ceiling, but not the public bound; the prototype failed the per-case screen. [Evaluation](#independent-packing-study). Dependent segments remain a different coordination trade-off. |
| B: scattered recursion | A bounded-lane arena could retire staging ownership while retaining one terminal compactor; inherited partials, recursive reserves and callback recovery move into that arena. | The studied packed-output-stream construction loses direct final placement and adds a post-join sequential pass; [modeled rejection](#scattered-completion-study). Other scattered constructions remain unexcluded. |
| Fused order/variation scan | One `engine.rs` result could replace flat/order handoff and variation scanning; adds directional/reduction state. | Must keep masked progress, early rejection, constant vector reduction, ascending SIMD and strict-descent savings. Separate modes restore current mechanisms; no saving-preserving construction was found. Not a measured rejection of all fused scans or permission to repeat R2. |
| Block passes for tiles | Move recovery to `block.rs`, retiring tile cursors, alternating buffers, cached-ID placement and `RestoreTile`. Adds directories/packing and a replacement for LSD finishing. | Block passes scatter then materialize; tile counting alternates buffers across digits. Per-digit replacement adds packing; retaining alternating buffers retains the targeted mechanism. No prototype without avoiding those passes. |

Parallel completion remains open: staging cannot handle oversized buckets or roots outside its [window](engine.md#bounded-staging); positional splitting still needs key partitioning or stable merging. Its saving passed controls only for random `u32`; above-window materialization remains unmeasured, and continuous cases stop below staging eligibility. Differences inside the historical parallel A/A ranges are undetectable there. The [study](../../ROADMAP.md#parallel-completion-study) must compare copies, critical path, metadata and suspended buffers alongside timing. A rewrite without a concrete alternative merely reprices adaptive paths; prototype only a model deleting mechanisms within the space ceiling. C remains an open design model; A’s evaluated construction is rejected below.

### Scattered completion study

Independent per-node arenas cost a frontier-dependent sum of square roots, with potentially linear growth before the fixed key domain saturates; fixed-width asymptotics alone do not reject them. A stronger B construction assigns at most P traversal lanes a shared arena, transfers child directory ownership depth-first, gathers tile leaves, appends completed leaves into a packed logical output stream sharing its final partial block, then materializes once after all lanes join. Lane reservations can survive callbacks if progress never waits for another permit. Accounting must include pending sources, suspended operations and completed output; equality recursion has no keywidth depth bound, so a digit-depth-only model would change equality routing.

This construction could retire staging’s owner locks, atomic locations and protected writes. We reject this specific gather–append–compact construction as a saving-preserving simplification: when eligible root buckets fit the same tile capacity, root scatter and leaf sorting match staging and no descendant packing is saved, but gather plus direct final writes becomes gather plus stream append plus final copying—three full-record transfers instead of two, excluding identical tile work and any relocation. It also defers a sequential full-output materialization until the lane join. Direct-final writes would need source/destination-overlap handling absent here; adopting tile output blocks or skipping already-final ranges would be different, untested constructions. The [counted model](../.vitepress/theme/sim/scatteredCompletion.test.ts) checks these transfers, join ordering and record conservation through tile errors, not arena reserves, Rust provenance or timing. Removed coordination could compensate in whole-call time; no measured slowdown or general impossibility is claimed. The historical staging comparison establishes why direct placement matters, not B’s penalty.

### Independent packing study

The independent A prototype first permutes blocks, then groups whole buckets into at most P output segments. Forward packing never overtakes an unread source; before packing, each segment snapshots the source suffix peers could overwrite beyond its final band. Accumulated partial-block slack is at most PR blocks per boundary, so snapshots across P segments cost O(P²Rb), or O(P^{3/2}√(Rn)) with balanced block length, plus block floors and directories. These buffers are bounded by segments even during callback suspension. Completed segments own exclusive final bands; recovery packs unfinished segments without callbacks before retiring the directory guard. This removes staging’s owner locks, atomic locations and free discovery but retains permutation recovery and adds snapshots and segment completion. One oversized bucket still dominates one packing segment.

[PR #117](https://github.com/Yikai-Liao/tessera-sort/pull/117) retained the prototype in history and restores the accepted engine. Ownership tests, independent review, Miri, ASan and platform/MSRV checks passed. Its paired revision screen nevertheless found parallel four-thread losses of 26.9% on wide-record sparse digits and 15.9% on `u64/far_swaps` (both 1M), plus 9.8% on `u128/sparse8`, 8.5% on wide-record random (both 1M) and a serial 7.9% on `row_u32/random/49`. These are process-level alerts, not established source effects; none is accepted. The two losses above the per-case limit and the larger memory ceiling reject this prototype as submitted, without claiming all independent packing is slower. Current-only large/above-window/skewed checks verify outputs and allocation, not revision gains.

Dependent packing could reduce boundary snapshots by waiting for consumed-source frontiers. That adds publication/wait rules; a finishing callback must not suspend before enabling a dependent source consumer, so release cannot simply coincide with local packing completion. This model restores ownership coordination and a release chain rather than preserving independent movement. It was not prototyped or timed; other constructions remain unexcluded.
