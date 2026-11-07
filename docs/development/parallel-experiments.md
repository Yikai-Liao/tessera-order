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
requires Rust 1.89. The parallel ADR is numbered 0010 to preserve upstream ADRs 0007–0009.

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
