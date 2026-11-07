<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Parallel sorting experiments

This is an experimental checkpoint, not a recommendation to replace the default engine. The optional Rayon API in the crate uses a serial root partition and bounded parallel child completion. Native root scatter, parallel block permutation, materialization/child pipelining, and block-recycling merge remain separate research candidates. Passing a focused test or winning one distribution does not make a candidate ready for the public API.

## Baseline and main synchronization

The initial implementation and measurements used upstream `d3b3c35`. Commit `60873c8` preserves the validated child-parallel API. Commit `f59a424` merges upstream `52dab6a` without changing any Rust sorting source or Cargo dependency relative to that checkpoint. The five upstream commits add the documentation site and improve CI. The merge keeps both the upstream documentation and the parallel design entry.

After the merge, all-feature workspace tests and doctests, warnings-denied Clippy and rustdoc, no-default-feature tests, Rust 1.85 tests, seven CI-routing tests, and the documentation model tests, type check, and site build passed. An independent old/new same-binary check completed five cases and 675 fully verified timing samples, each accumulating at least 25 ms. Merged child-parallel versus pre-merge child-parallel paired ratios were 1.009, 0.987, 1.036, 1.001, and 1.005, all within their combined A/A and placement envelopes. This establishes no measured integration regression in these cases, not the absence of a smaller regression below the controls. Identical-source serial sorted-u64 copies differed substantially again; this is a code-layout investigation signal, not an upstream algorithmic speedup.

### Later upstream SIMD integration

The experimental branch also merges upstream `72c2984`, which follows `52dab6a` with
four commits. Unlike the first synchronization, this update changes Rust sorting code:
it adds optional SIMD inversion scans and prefix sums and a shared insertion entry point.
The parallel preparation path now uses that same entry point. Both `rayon` and
`portable-simd` stay optional; the former retains Rust 1.85 compatibility, while SIMD
requires Rust 1.89. The parallel ADR is now numbered 0011 to preserve upstream ADRs 0007–0010.

The measurements below remain tied to their earlier frozen implementations. They are
not speedup claims against the newly merged SIMD baseline. New default and SIMD-enabled
strong-serial comparisons are required before a later performance acceptance decision.
Research patches retain their explicit historical application bases rather than being
silently rewritten during this merge.

The later [combined phase and worker-count study](best-combined-profile.md) now supplies
same-binary comparisons against both default and portable-SIMD serial code from
`72c2984`. It retains the old-core combined implementation as an explicitly frozen
candidate. Its 16 targeted cases establish large random-data advantages but retain
low-cardinality plateaus, broad controls, and an unresolved trace-disabled outlier.
The historical matrix below is not replaced or relabeled by that study.

### Subsequent ascending-confirmation integration

Upstream `5b13ac1` adds SIMD-aware ascending-order confirmation after the first
unequal encoded key. The experimental branch merges this change while keeping
the optional Rayon entry points and existing generic extraction contracts.
The parallel ADR moves to 0011 so upstream's ascending-confirmation ADR keeps
0010. Historical benchmark snapshots, including the combined and halo screens,
remain unchanged and are not relabeled as measurements of this newer main.
The same-source combined port requires separate validation and measurement
before any performance conclusion; integration alone is not an acceleration claim.

## Measurement method

The first broad matrix covers 55 cases across `u32`, `u64`, full-width and half-only `u128`, 16-byte and 128-byte records, and owned records containing a `String`. Cases include random, several skew levels, low cardinality, sorted, reverse, equal, and sparse random swaps. It is a deliberately uneven coverage plan, not a workload weighting or the full type/distribution/size Cartesian product.

- One binary compares 19 variants, including strong serial Tessera, a tighter-digit serial control, individual parallel stages, their combinations, Rayon stable sort, and the Copy-only rank-merge prototype where its types are supported.
- Nine randomized blocks use fresh input for every sort. Cloning happens on the actual pool worker before the timer; full output and stability are checked afterwards. Each sample accumulates at least 25 ms of sort time. Small variant dispatch is inside the timer.
- Both the original serial implementation and the primary parallel candidate have A/A repetitions and independently placed identical-source controls. Full reciprocal min/max control envelopes are retained, including outliers.
- Reported thread scaling is public API scaling. One thread takes a serial fallback, and additional threads can change decomposition. It is not a fixed-kernel scheduling experiment.
- Separate allocator runs measure additional requested heap bytes, not RSS, stack, or pre-created thread pools. Input/reference copies and owned string payload preparation are outside the measured operation.

All 9,171 timing samples passed full reference validation and the duration target. The matrix explicitly records 26 unsupported Copy-rank case/variant pairs. All 1,019 supported allocation measurements verified the output and ended with zero retained-byte delta. Raw samples, profiles, and harnesses are retained outside the repository, as required by the benchmark policy.

## Broad results, including regressions

The table compares the native-scatter/block-permutation/pipeline candidate with its low-digit route enabled against original serial Tessera. Times are medians of the per-block A/A mean. Ratios are medians of paired block ratios, so they need not equal the quotient of the two displayed aggregate medians. Values below one favor the candidate.

| Case | Serial ms | Candidate ms | Paired ratio | Serial control envelope | Candidate control envelope |
|---|---:|---:|---:|---|---|
| `u64`, random, 10,000,000 | 96.835 | 29.293 | 0.303 | 0.923–1.084 | 0.843–1.186 |
| 16-byte record, random, 10,000,000 | 156.647 | 57.337 | 0.362 | 0.817–1.224 | 0.470–2.128 |
| Full `u128`, random, 1,048,576 | 13.500 | 4.467 | 0.331 | 0.755–1.325 | 0.809–1.236 |
| 128-byte record, random, 262,144 | 16.880 | 8.511 | 0.500 | 0.712–1.405 | 0.751–1.331 |
| Owned record, 256 keys, 32,768 | 0.186 | 0.874 | 4.353 | 0.805–1.242 | 0.243–4.118 |
| 16-byte record, 16 keys, 65,536 | 0.190 | 0.495 | 2.563 | 0.713–1.402 | 0.576–1.736 |
| High-half `u128`, 16 keys, 65,536 | 0.215 | 0.487 | 2.148 | 0.634–1.577 | 0.607–1.648 |
| `u64`, sorted, 1,048,576 | 0.342 | 0.651 | 1.949 | 0.892–1.121 | 0.851–1.175 |

The four displayed improvements have nine directional wins; the four regressions have none. Across the entire uneven plan, the conservative primary comparison has 13 faster cases, four slower cases, and 38 unresolved cases after requiring agreement beyond both complete control envelopes. These counts are not an overall speed score. In particular, the broad matrix does not justify enabling the combination unconditionally.

Earlier four-case measurements observed 45.15 ms for 10M random 16-byte records. The later 56–59 ms measurements are retained rather than replacing them with the best run. Copy rank merge is competitive on wide records but does not beat the strongest radix combination on the large scalar/16-byte random cases. Owned and low-cardinality results need separate decisions.

## Space costs

| Case | Serial extra heap | Eight-thread combined extra heap |
|---|---:|---:|
| `u64`, random, 10M | 1,318,748 B | 6,168,395 B |
| 16-byte record, random, 10M | 1,849,780 B | 7,971,658 B |
| `u32`, 256 keys, 1,048,576 | 580,608 B | 4,478,528 B |
| Owned record, 256 keys, 262,144 | 752,640 B | 5,614,180 B |

For the small `u32` case, per-shard block/bucket floors make scratch larger than the 4,194,304-byte input and the measured Rayon stable buffer. Fixed-thread asymptotic sublinearity does not make that practical cost disappear. Thread count, block size, directory capacity, temporary metadata overlap, and live child workspaces must all be included in the space account. A full linear record-output buffer is a different algorithmic tradeoff and is not introduced silently.

A separate root-shard budget experiment reduces the `u32` 256-key/1M peak from 4,478,528 B to 2,301,388 B or 1,220,720 B while preserving eight-thread child/permutation capacity. It has not established a universal speed win. The protected large random cases keep their existing peak; small-case performance is being measured independently.

## Diagnosed failures and next structural work

1. **Chunk-local sorting followed by whole-array serial sorting is not a free merge.** The requested implementation was measured on scalar and record types. It did not demonstrate a reliable win over strong serial Tessera. Individually sorted runs also alias regular occupancy samples; changing the sample reduced scratch in one experiment but slowed the large end-to-end case.
2. **Idle Rayon consumers can nest on one worker.** The pipelined prototype's `rayon::yield_now` could execute another long-lived consumer, leaving fewer workers doing useful bucket work. A separate `std::thread::yield_now` candidate eliminates that observed nesting and passes no-spare-worker nested public-sort/join/panic tests. Its five-block speed screen is directional evidence, not a broad acceptance result.
3. **The sorted-u64 regression has a reproducible code-layout cause.** Five implementations have byte-identical 283-byte order-probe machine code. Controlled copies of that code at different addresses reproduce about 609 microseconds versus 310–312 microseconds. All nine paired blocks exceed the conservative reciprocal A/A envelope of 0.711–1.407. No scheduler or changed instruction sequence is needed to reproduce this regression. A no-inline attribute alone merely changes placement; an alignment-robust confirmation kernel is being tested.
4. **Making every compact phase parallel is not automatically faster.** A generic rank-repack prototype reduces its original excessive metadata but remains behind the strongest combination in the measured pilot. Its planning, repack, restoration, and non-overlapped children are measured separately. Fewer copy instructions can still lose to irregular access and scheduling overhead.
5. **Block-recycling merge needs a fast merge loop.** Padding hot state, cached block cursors, a loser tree, and parallel restoration substantially improved the actual prototype. Doubling the local run count reduced local-sort time but increased merge/planning work and heap enough to lose overall. The owned-record extension has separate panic/ownership validation; Copy performance is not a substitute for owned measurements.
6. **Virtual bucket consumption did not beat the strongest combination and is stopped.** The completed [two-stage control checkpoint](virtual-stage-controls/README.md) records the actual end-to-end losses, targeted-stage improvements, extra heap, and limited three-block evidence. Reducing root scanning and restoration did improve their own prototypes, but neither reached the existing combined implementation. Further work returns to that strongest implementation and targets its measured permutation/squeeze bottlenecks rather than continuing virtual-directory recursion.

## Safety and maturity limits

The public API must retain stable generic records, `T: Send` without requiring `Sync`, exclusive borrows, non-cloning keys, owned-record conservation under key panics, and no-default-feature support. Research crates and patches are not proof that all those contracts are satisfied.

Default Stacked Borrows Miri coverage is recorded per path. A Rayon-only reproduction fails inside the current Crossbeam dependency under that model; changing to Tree Borrows and observing assertions pass is not reported as a clean replacement pass. The corresponding moving kernels have separate scoped-thread/default-model checks. Native stress tests, focused Miri tests, and code review each cover different risks.

Evidence identifiers for this checkpoint:

- Timing samples SHA-256: `c6d457d9b82ac9cdcf341589c0f496183c5a2e3821115c7f0acd124adf0918ed`.
- Per-case summary SHA-256: `43a3e62f6506c7c4446ad085f5dababc12a900715635d60e39d75cd91b445db7`.
- All source/binary/sample manifests remain with the external experiment bundle. No outliers were removed. The main-merge verification is a separate five-case run, not a reuse of old speed ratios.

## Rejected source-prefetch mechanism

A separate [fixed four-block source-prefetch control](compact-prefetch-control.md)
left the best combined layout and copy count unchanged but did not establish an
end-to-end improvement in its bounded screen. The optional research patch is
retained for reproducibility; no new default or parameter sweep follows.

## Histogram-aligned root checkpoint

The standalone `research/aligned-root` crate preserves a different native-root
structure: a pure-key histogram assigns final bucket/shard rank intervals, shared
boundary blocks receive disjoint fragments, and full body blocks reuse consumed
input storage. Packed block restoration produces real contiguous children and
requires no interior-hole squeeze. This is a sealed Copy-only research API, not
the generic public Rayon implementation, and has no panic-permutation guarantee.

The checkpoint includes the serial restoration control, cached-write-cursor
variant, parallel block restoration, and direct-u64 child routing. Debug/release,
Rust1.85, Clippy and focused default-model concurrent Miri checks pass. Source
notices and the exact frozen Tessera child dependency are preserved.

No performance win is established. Earlier u64 v1/v2 controls used by-key APIs
on both sides; the later Direct-u64 comparison is recorded separately rather
than relabeling those results. Exact additional heap is8,069,820 bytes for u64
and11,258,012 for Row16 at eight workers, about6.1 times the serial comparison.
Histogram cost, root write-cursor cost, and lost root/child overlap remain in
the end-to-end accounting. See the crate README and validation record for the
individual controls, noisy/adverse measurements, and unsupported contracts.



## Bounded halo squeeze: no adoption

A generic `T: Send` follow-on to the frozen strongest combined implementation
keeps native scatter and block permutation, stages bounded threatened-source
snapshots, and runs each group's squeeze plus ordinary contiguous child sorts
in parallel. It uses no full-input record buffer. The snapshot cap is
`min(floor(n/8), 32 * P * ceil(sqrt(n)))` record cells; root buffers and child
workspaces are additional and measured separately.

The corrected same-binary screen uses random seed-1 u64 and 16-byte records,
10 million records, four/eight workers, and three randomized blocks. It verifies
84 measured and 28 warmup outputs. Paired halo/combined medians are 1.453/1.030
for u64 at four/eight workers and 1.147/1.198 for 16-byte records. None supports
adoption. The u64 four-worker ratio is slower in all three blocks and exceeds
the baseline A/A-plus-placement envelope of 1.229. Candidate-repeat envelopes
reach 3.151 and 3.782 because long samples remain unexplained; they do not excuse
the regression. Independent timing medians must not replace within-block ratios.

Measured additional peak heap at eight workers increases from 6,168,395 to
9,191,622 bytes for u64 and from 7,971,658 to 10,351,952 bytes for 16-byte records.
All allocator runs end with zero retained delta. Four-worker peaks increase by
600 bytes each despite nonzero halo capacity because allocation lifetimes differ.
These are allocator peaks, not sums of separate capacity maxima.

The initial pilot is preserved separately. Its record H1/H2 labels used
semantically equivalent but distinct closure monomorphizations, so their ratio
was not exact-entry A/A. Only the driver dispatch changed for the corrected
screen; all algorithm bytes and allocator measurements remain unchanged.
Neither pilot establishes an improvement, and no further run seeks a winner.

Validation is deliberately qualified: 72 default debug tests, 10 applicable
opt-in public tests, ten optimized halo tests, eight strict-provenance default
Stacked Borrows serial/scoped-thread Miri tests, lint/docs and no-default checks
pass. Rust 1.85 was a compile check, not a full test run. Two old opt-in worker
injection probes fail on a verified cap-driven serial-child fallback; they are
retained and the whole opt-in suite is not called passing. Actual Rayon-under-
Miri is not covered. Production defaults remain unchanged.

The source patch, worker-explicit ownership/space argument, application hashes,
and complete two-pilot report are archived under `research/patches/halo-squeeze.*`.
They apply after the historical combined patch, not directly to current main.
Full raw evidence and the harness stay outside Git. The screen's main/SIMD
controls remain frozen at `72c2984`, not the subsequently merged `5b13ac1`.


## Combined pipeline on current main

`research/patches/combined-main-5b13.patch` preserves the stronger native-root,
parallel block-permutation and standard-thread-yield pipeline on upstream
`5b13ac1` serial/SIMD kernels. It applies to experimental commit `b1d36ce`, not
to the original historical base. Cargo identities and features are unchanged.
The patch is archived and not applied to the public API. Its base `b1d36ce`
used child-only Rayon; the later public overlap implementation described below
is a distinct implementation, not promotion of this research patch.

The isolated port passes debug all-target tests with SIMD off (47 unit and 22
integration tests) and on (52 and 22), six doctests per feature set, Rust 1.85
default/Rayon/no_std tests, lint, rustdoc and focused Miri. Final inherited lint
cleanup reruns both library suites and affected Miri kernels. These are not
claims of full optimized integration tests or Miri execution of Rayon scheduling.
Independent archive application and all twenty runtime/test hashes match.

An optimized nine-label, four-case pilot compares old combined, same-core new
SIMD-off/on, and exact-main serial controls. All 108 measured and 36 warmup
outputs verify, as do 144 separate smoke/corruption/fresh-input checks. The
new-SIMD/old-combined paired medians are 0.910/1.027 for 16-byte records at
four/eight workers and 0.890/0.813 for u64. No improvement is established:
the first three control envelopes span 17.6–22.4%, and u64 eight-worker C2
contains a retained 1,129.698 ms sample versus the same-block C1 at 35.838 ms.
Its cause is unverified; neither that sample nor its 31.522-fold A/A ratio is
removed. Three blocks do not meet the nine-block acceptance rule.

The source lineage and full report are stored beside the patch. The pilot
measures random 10M inputs only, process affinity 0–7, API allocation included.
No new heap or full-distribution claim is made. Follow-on layout experiments
must compare literal and indirect versions on the same core, and give the
strong serial baseline the same leaf improvement.

## Bucket-completion overlap in the public API

The public Rayon entry point used to finish the root compaction before any child
task started. The compaction already calls its visit hook once per materialized
bucket, and such a bucket is neither written again nor read again as a copy
source, so the hook is a safe hand-off point: the crate now queues a run of
adjacent materialized buckets on the Rayon scope as soon as the run reaches
`MIN_TASK_BYTES`, and the workers sort those runs while the rest of the array
is still being compacted. Runs take their workspaces from a reusable pool. The
ordinary non-reentrant benchmark keys keep at most one leased workspace per
worker; reentrant user keys can suspend several jobs on one worker, so this is
not an unconditional workspace-count bound. See the current space account in
[the parallel design](../design/parallel.md).

Measured as before (one binary, interleaved blocks, median, full output
verification, AMD Ryzen 7 8845H: 8 cores, 16 threads, N=10M):

| case | 2 workers | 4 | 8 | 16 |
|---|---|---|---|---|
| u64 random, before | 1.44x | 1.47x | 1.89x | 2.08x |
| u64 random, overlapped | 1.69x | 2.67x | 2.66x | 2.59x |
| 16-byte rows, before | 1.28x | 1.33x | 1.70x | 1.73x |
| 16-byte rows, overlapped | 1.62x | 2.21x | 2.19x | 2.18x |
| u32 random, before | 1.45x | 1.51x | 1.98x | 2.13x |
| u32 random, overlapped | 1.67x | 2.41x | 2.90x | 2.81x |
| u128 random 1M, overlapped | 1.33x | 1.76x | 1.84x | 2.00x |

Absolute 16-worker time falls from 30.85 ms to 24.81 ms for u64, from 56.23 ms
to 44.94 ms for 16-byte rows, and from 20.15 ms to 15.38 ms for u32. The
overlapped rows were re-measured on the base that also carries the vector
ascending-order confirmation; the earlier rows come from the base before the
overlap. Against
`rayon` on the same pool the parallel call now wins seven of eight cases; the
remaining loss is the single-digit route below.

The plateau at about 2.6x from four workers up is now the serial root itself:
one scatter pass (8.96 ms at N=10M) plus one compaction pass (13.88 ms) is the
whole critical path, and the children hide inside it. Measured ceilings for
those two passes on the same machine are 1.75x for a scattered write pass
(14.40 ms to 8.18 ms) and 2.0x for random block copies (9.81 ms to 4.97 ms), so
parallelizing the compaction is the next structural step: at its ceiling the
root falls to roughly 16 ms, which is the 4x region.

### Single-digit inputs still do not parallelize

Inputs whose varying key bits fit one digit (`top - low <= SPLIT_BITS`) take the
serial equality and outlier routes before any partition happens, so
`par_stable_sort` reports a speed ratio of about 1.00 and loses to `rayon` by
about 15% on 10M records with 256 distinct values. Removing the serial root above
would not change that: the whole sort is one materialization pass with no
independent work behind it.

## Public overlap borrowing correction

Review of `6c657de` found that a batch retained the first materialized bucket's
pointer and used it to reconstruct later adjacent buckets. Numeric adjacency
is insufficient: the first pointer's borrowing provenance covers only its own
slice. A two-bucket test using the actual partition callback and `Job::run`
reproduced a default Stacked Borrows strict-provenance Miri failure at the second
slice reconstruction, without invoking Rayon scheduling.

The correction stores each bucket's own pointer and length. Scheduling,
thresholds, key extraction and root movement are unchanged. Three focused
scalar/owned/exclusive-borrow tests pass the same Miri model. The historical
hardware results above are preserved as reported, not relabeled as measurements
of the correction. Subsequent fair comparisons must identify the corrected
public source separately from the frozen research combined implementation.
