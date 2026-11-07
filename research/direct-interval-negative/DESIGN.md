<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Direct live-prefix relocation: a gated next question

Status update, 2026-11-07: the authorized pure-metadata probe rejected this fixed candidate on its first real native Row16 graph. See RESULT.md for cap failures, source identities and shared-lock records. No record-moving candidate, unsafe code, sort benchmark, or shared-repository modification was made. The remainder preserves the prior design and gates rather than proposing continued implementation.

## Result first

The only proposed change is to move the native root's live source prefixes directly to their final stable rank intervals. It would replace **whole-block permutation followed by squeeze** with **dependency-controlled direct interval moves plus bounded source evacuation**. It retains the native scatter, its observed classifications, the final contiguous child interface, and the serial leaf core. It does not introduce another histogram/key pass, repack every record into another block directory, recurse through virtual directories, or revive group-halo squeeze.

The opportunity is specific: native root materialization currently requests roughly two full-input copies after scatter, while a successful direct mover requests exactly one final copy per live record plus the records it evacuates. The obstacle is also specific: an unaligned final interval can overwrite two unread source blocks. Existing path/cycle results solve a one-source-per-target-block permutation, not this two-dependency problem. There is no evidence yet that the actual native graph fits a bounded live evacuation budget with enough ready work. A failed bounded metadata probe should end this candidate before unsafe Rust is written.

## What the evidence establishes

- `../tessera-current-scaling-profile/report-v2.md` records an older exact-source diagnostic, not the latest public tip. Its W8 root completion is close to total completion, with short residual child tails. Its permutation/squeeze intervals include parallel execution and overlap; the near-98% timestamp is not a 98% sequential fraction. Latest source-matched measurements remain necessary.
- In that report the native combined root copies roughly n records in permutation and n in squeeze. Public instead has serial block partition/compaction and materialized bucket jobs. They must remain separately identified.
- `../tessera-bottleneck-audit/pbbs-block-dependency-design.md` explains why PBBS/Parlay ranks make output destinations disjoint but do not protect unread aliased input. Their full output allocation supplies that protection.
- Exact-rank aligned scatter already removes squeeze by building exact final blocks. Its histogram/rank prepass, block staging, restore, and delayed children are additional costs. `../tessera-region-path-formal/evidence/aggregate.jsonl` does not establish an advantage over strong combined: Row16 paired median 0.8818 with only 7/9 faster blocks and a wide placement envelope; u64 paired median 1.1278 with only 1/9 faster blocks. Those historical source measurements do not prove a present-tip regression or improvement.
- The prefix exchange broker adds up to three copies per exchange and repeated waves. Natural path restoration saves only a modest part of the restoration copies. Neither supplies the desired large materialization-stage deletion.
- `../tessera-parallel-repack-model/README.md` and `../tessera-rank-compact-evidence/` already cover native-prefix repack followed by restoration. The measured packed repack has planning, an extra relocation stage, substantial metadata, and missing child overlap. Merely renaming it or deleting descriptors is not this proposal.
- The virtual output-rank stream is explicitly stopped in `../tessera-output-rank-model/README.md`. This design neither reopens it nor changes child algorithms.
- Prior waiting, deferred launch, root geometry, ordered-map, and copy-coalescing controls are not proposed again. Their outcomes must be carried into the current baseline choice.

## Exact input to the probe

Capture the adopted **native** combined root immediately before `compact::permute_blocks`, after local guards have transferred ownership and local directories have been dropped. Do not capture after permutation. Do not substitute the aligned-root interval CSVs: their exact final blocks have already solved alignment, reducing the graph to paths/cycles and making this test falsely favorable.

For each nonempty logical live prefix, emit:

- node ID and semantic bucket;
- original allocation kind and ID, physical block ID, and offset in that allocation;
- initialized prefix length, at most B;
- final output start rank, from the cumulative live-prefix lengths;
- provenance fields only when actually retained by the native scatter; never infer an original-tail fragment merely from a scratch address.

Also emit n, record size, P_root, pool workers, R, B, input/full-block counts, all allocation capacities, physical-ID translation, semantic ends, native source revision/hash, leaf-core revision/hash, enabled features, generator/fingerprint, and hook position. Every pointer remains local; exports use allocation IDs and integer offsets, never assumed adjacency between scratch allocations.

Validate that source ranges are unique and disjoint, lengths sum to n, final intervals partition [0,n), each semantic bucket has its original shard order, and no uninitialized padding is counted. The original partial input tail must not silently become an authoritative full-block source. Use the existing directory as authority; no additional key calls or record capture are required.

## A fixed algorithm for the first probe

Let V be the nonempty prefixes, with V <= floor(n/B)+PR. Node i has original source interval S_i and exact destination interval D_i of equal length <=B.

Create edge i -> j when writing D_i intersects the still-live original input source S_j, for j != i. Scratch sources never intersect input destinations. Own-source overlap is handled by memmove and needs no self edge. Since an interval of length <=B intersects at most two B-aligned input blocks, there are at most 2V edges. A block has at most one authoritative live prefix in the native directory. These assertions must be checked, not assumed from an aligned-root fixture.

1. Drain ready nodes whose dependency count is zero. Each copies its exact live interval to its final destination. Its original source is released only **after** the copy completes. The reverse-edge list then releases waiting writers.
2. At a deadlock, choose one still-original, not-running source node with the largest incoming dependency count, breaking ties by final output rank. Evacuate its initialized prefix to one B-capacity spill slot and redirect its sole authoritative source there. Remove the dependency edges to its original address. This does not complete the node: its own destination can still be blocked.
3. Keep draining ready nodes. A spilled node frees its slot only after its final copy completes. Never spill a node twice.
4. If a new spill would exceed the frozen capacity, reject the entire plan **before record movement** and retain the unchanged combined materializer. No online fallback may invoke an old directory guard after direct movement has started.

For the first dry run use explicit ready frontiers and joins. This is deliberately conservative and easy to audit. Record frontier count and width; do not conceal thousands of barriers by calling the schedule parallel. A subsequent asynchronous DAG executor is not automatically authorized by a successful token simulation.

The planner needs only O(V+E) node/edge state and at most 2V move/evacuation events. A static incoming-count ordering can implement the fixed evacuation rule without repeatedly recomputing SCCs: an unconsumed source's incoming dependency edges remain until that source is released. Export SCC statistics once as a diagnostic, not as an unmeasured expensive production requirement.

There is no proved sublinear live-spill bound for this heuristic. Its graph can stall, and a bounded failure says this policy is unsuitable, not that every possible interval mover is impossible. Do not repair failure by splitting down to individual records, constructing a linear visited array, retaining unbounded interval fragments, raising the space constant after seeing results, or sweeping victim-selection policies.

## Why movement and stability could be sound

Final ranks are obtained from the already-stable bucket/original-shard/original-position stream. A node preserves the order inside its prefix. Its final interval never changes, so correct movement preserves stability without reclassification.

Before an ordinary ready move, every other live original source intersecting its destination has been consumed or redirected to spill. Thus it cannot destroy an unread record. Its own overlap is safe with memmove. Distinct destinations are disjoint, and no ready node depends on another ready node's live original source. Parallel copies in one ready frontier therefore have disjoint destinations and do not overwrite one another's reads. Source release and successor readiness occur only after copy completion with the required synchronization.

Evacuation changes sole logical ownership; the old bytes become stale, not another owned T. The snapshot step is joined before consumers of the released address can write. The first implementation must use an original input raw ancestor and distinct scratch allocation ancestors, without a fresh whole-input mutable reborrow.

A bucket becomes publishable after all its nodes finish. Every input cell in its destination has then been written only after any original source there was retired. No future node writes that destination, and no future root read uses that original address. Later reads of a spilled token use spill storage. This supplies a candidate retirement proof for contiguous child slices, unlike releasing a bucket merely because some physical blocks were copied.

For the first physical mover use sealed pure-key Copy records, including stable IDs and full payload verification. Copy does not by itself guarantee a permutation after a panic. This prototype must say so if recovery is absent.

A generic extension must retain the existing native scatter recovery for user-key panics, add no key calls after adoption, and transfer ownership once to a new nonallocating recovery guard. Inconsistent user keys must remain memory-safe and retain every owned record; sorting/stability is required only under the existing consistent-key contract. Preallocate graph, plan, spill slots, job state, and error storage before ownership changes. On scheduler/child panic, join workers and finish all remaining planned moves without user calls, then resume the panic. Never let the former Sorter guard compact a directory made stale by this mover. Owned values, Send-but-not-Sync values, exclusive references, allocation failures, and secondary panic payload drops are separate mandatory gates.

## Full space ledger, with processor dependence

Let t=sizeof(T), w=sizeof(usize), H=P_root*R, F=floor(n/B), Q<=P be active movers/children, and K the maximum simultaneously live B-capacity spill slots.

- Existing native root record storage: 2H*B cells, counting retained shard allocations and reserved tails, even if logically dead.
- New evacuation storage: K*B cells. No extra B-record worker temporary is needed for direct memmove; registers are O(Q).
- Existing adopted root directories/partials/ends: O(F+H+R) words. Exact source capacities are required, including any simultaneous caller-retained allocation.
- New graph/plan: V node records, at most 2V edges and their reverse-index storage, V ready IDs, V victim-order IDs, at most 2V event IDs, K free-slot entries, O(R+Q) completion state. Record the actual Rust layouts/capacities. This is O(F+H+R+Q+K) words, not O(n) record IDs. Do not call all of it free because it has the same asymptotic order as the native directory.
- Children: separately include all simultaneously allocated and pool-retained serial leaf workspaces. For A disjoint active children of sizes n_i and a serial c*sqrt(n_i) bound, the active part sums to at most c*sqrt(A*n), but retained capacities can make actual peaks larger. Measure rather than assume they are released.
- Diagnostic exports, external inputs/reference arrays, and tracer buffers are separately identified and disabled for algorithm heap/timing runs.

Use a frozen initial extra-record cap

    K = floor(P*ceil(sqrt(n))/B).

This is a frozen research acceptance cap with coefficient one independent of P, not a proposed public constant or a measured optimum. Checked arithmetic and allocator capacities apply. All root scratch, directories, graph, queues, state, spill and caller/child-retained auxiliary capacity must also fit the frozen total cap `32*P*ceil(sqrt(n))*t` bytes. At n=10M/P8 this is 6,477,824 bytes for u64 and 12,955,648 for Row16. If the native baseline alone or a concrete complete graph plan does not fit, reject; do not raise the cap or hide metadata. Relative to strong serial, actual same-core heap peaks must still be checked against the user's few-times preference.

The full expression is

    t*(2*P_root*R+K)*B + O(w*(n/B+P_root*R+K+P+R)) + child/caller-retained bytes.

With fixed radix and a balanced B proportional to sqrt(w*n/(t*P_root*R)), the root terms are O(sqrt(t*w*n*P_root*R)), plus the explicit spill cap. For growing R or minimum B floors use the full expression; do not present this as independent of R or hide P by fixing it at eight. The whole input can be smaller than conservative floors, so dispatch must validate a concrete byte cap before allocations and decline when it cannot fit.

## Minimal executable gates and refusal conditions

### Gate 0: source identity

Receive the latest exact-source profile and freeze public, strong serial, and strong combined leaf-core identities. The live shared repository changed during this review, so no current-main result is inferred from historical reports. Keep the publisher's checkout read-only.

### Gate 1: one real native graph, no record mover

Start with the existing 10M seed-1 Row16 random input at P_root=W=8, R and B frozen to the actual chosen combined baseline. Capture once outside timing, validate the above invariants, and run the single bounded dry-run policy. This is not an aligned-directory or random-permutation substitute. If it survives, repeat on matching u64 and then W4 geometry without a parameter search; add spread-d16, skew, empty buckets, and original tails as correctness/adversarial cases.

Required fields: V, E, self-overlap records, input/scratch source records, component/SCC sizes, initial ready records, peak live spill slots/cells, total spill records, number of evacuations, frontier sizes, dependency-wave count, max and sum per-worker assigned records, fixed-point records, all bucket-ready record-weighted quantiles, and full destination coverage. Emit every cap rejection and the unscheduled state size. Simulate baseline split-cycle jobs and serial squeeze on the same metadata with explicit copy-step conventions; include snapshots, padding, final tail, and overlapping child work only when a comparable trace exists.

Reject immediately if the frozen spill cap is exceeded, if any fallback would occur after mutation, if construction needs record-level or unbounded fragment state, or if the schedule has too little runnable work. Also reject the large-pass-saving premise unless exact logical post-scatter copy volume is at most 75% of the baseline permutation+squeeze count. That threshold is a conservative screening decision, not proof that 25% fewer copy requests means 25% faster. Do not change it to rescue the sample.

If readiness is late, label that result explicitly. Late readiness does not alone forbid faster direct materialization, but eliminates an early-child-overlap rationale. A barrier-only prototype must not be advertised as equivalent to combined's overlapping child execution.

### Gate 2: only if Gate 1 survives

Write a small token mover and then sealed Copy moving kernel, with exhaustive small geometries, self-overlap, two dependencies, scratch-source starts, empty/tiny prefixes, tails, cap failure before mutation, long dependency chains, multiple cycles, and immediate destructive child writes after publication. Require native real-Rayon tests and default Stacked Borrows/strict-provenance tests of the actual pointer kernel. Token tests are not an unsafe proof.

### Gate 3: narrowly measured phase experiment

Only after the safety gates, compare unchanged native scatter + unchanged combined materializer against unchanged scatter + direct mover at identical n/input/layout/P/R/B and leaf core. Include planning and allocations, not only copy kernels. Keep the unchanged combined overlap implementation as a control. First measure direct relocation/root phases and total, actual peak heap separately, same-core public and strong serial baselines, duplicated A/A labels, independent placement builds, randomized matched blocks, common CPU affinity/lock, verified warmups, and complete stable reference equality outside timing. Retain all tails/outliers and tracer-on/off controls if tracing is used.

Run an acceptance-length screen only after the small phase experiment shows a plausible margin beyond controls. A phase win with lost child overlap is not a whole-sort win. A narrow u64-only result is not generic progress. Any apparent improvement must survive Row16 and the complete semantic/panic/owned gates before public integration is considered.

## What is unknown, rather than a claimed root cause

No current native live-prefix graph has been inspected here. The live-spill requirement, ready-frontier width, planning cost, dependency-queue cost, memory locality, and overlap value are unknown. Logical copies are not DRAM traffic or hardware bandwidth. The old W8 profile motivates trying to delete a materialization pass; it does not prove that this interval executor will be faster. The smallest useful next output is therefore a measured bounded graph result, including a clean rejection if it fails.

## Source-only probe delivery

`probe.py`, `test_probe.py`, and `CAPTURE.md` were written after the design review, without build or execution. The probe enforces the coefficient-one spill cap. It reports dependency-policy acceptance separately from total-space eligibility. The initial conservative 64-bit structure-of-arrays allocation plan is `38V+16E+4(F+2PR)+8K+12R+16P+484` bytes, including construction mapping and twenty Vec headers, in addition to the full reported native/caller retained capacity and `K*B*t` spill storage. This can reject Row16 even before a favorable dependency result; that is an admissible outcome. It is a fixed proposed layout, not a measured Rust allocation. The native capture must supply `root_and_retained_aux_capacity_bytes`; absent data produces unknown space eligibility rather than a pass. Later concurrent children need their own same-cap peak check. Iterative SCC statistics are diagnostic only and do not establish a linear-spill lower bound. The first source-only simulator does not yet include an exact baseline schedule replay; that omission is explicit in its output.
