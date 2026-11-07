<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Async retirement overlap diagnostic

This bounded follow-up diagnoses the existing exact-target exchange with Barrier and RetireInline child scheduling. It introduces no algorithm, tuning, or padding variant and does not establish performance acceptance.

The same binary contained ordinary Barrier/Inline controls and their instrumented counterparts, each with two labels calling the identical named body. The two fixed inputs were 10 million seed-1 u64/Row16 records, eight workers/shards, radix 256, shift 56, and block sizes 128/64. One warmup and three measured blocks produced exactly 64 full calls, each fully compared with a stable typed reference. All 32 diagnostic traces, including warmups, passed event validation. Individual calls had no 50 ms duration floor.

## Early overlap exists, but it is not free parallelism

Instrumented Inline started its first child at a median 0.214 ms for u64 and 0.436 ms for Row16, versus 6.061 ms and 12.223 ms for Barrier. Every measured Inline call had 255/256 child intervals overlapping owners with unprocessed targets. Approximately 99.5%/99.4% of child wall time overlapped that processing horizon.

However, 248/256 children began while their own synchronous owner still had targets to process. Median final-owner processing markers moved from 5.968 ms to 16.336 ms for u64 and from 12.158 ms to 24.208 ms for Row16. Later readiness therefore spreads over a longer interval while children run. The first bucket's readiness is not the missing opportunity; synchronous child execution delays further processing by its owner.

The marker means completion of the owner's target-processing loop before its final retirement callbacks. Another exchange may already have placed an unprocessed target home. An overlapping owner can also be inside another child, spinning, or descheduled. These events do not prove simultaneous copying, additional useful parallel work, or a hardware bottleneck.

## CPU accounting within the diagnostic execution

After subtracting nested children only from the matching actual worker/thread interval, median root residual CPU sums were 45.737 ms/84.324 ms for Inline u64/Row16, versus 39.865 ms/85.682 ms for Barrier. The six-call Inline ranges were 28.499–52.538 ms and 79.593–99.059 ms. Child CPU sums were 71.131 ms/83.568 ms for Inline versus 81.616 ms/94.433 ms for Barrier.

Inline root residual represented a median 39.8%/50.8% of the measured root-plus-child CPU, excluding histogram/scatter and other end-to-end work. Residual includes bookkeeping, exchange/lock work, and instrumentation. It does not isolate locks, memory bandwidth, copying, or false sharing. Barrier child jobs may run on different threads than their earlier root jobs; cumulative CPU is never subtracted across those intervals.

## Perturbation prevents a performance conclusion

Paired median instrumented/ordinary ratios were 0.929/1.115 for u64 Barrier/Inline and 0.991/1.081 for Row16. The ordinary Inline/Barrier ratios were 0.841/0.913, but controls varied substantially: u64 Barrier A/A reached 1.423× and instrumented Barrier 2.214×; Row16 A/A reached 1.228×. Instrumented Inline itself was materially perturbed. Three directional blocks and one-call durations do not justify a speedup claim, acceptance, or attribution. No rescue run or expanded benchmark followed.

Worker<T> stayed 392 bytes, alignment 8, with source offsets next 296, identity 352, one 360, three 368, remaining 0, and workspace 48. The first-worker address modulo 64 was 0/16 in the captured u64/Row16 observations. Those facts do not show hot-field cache-line sharing or its cost. No padded variant was tested.

## Safety and reproducibility

The exact exchange transaction and Worker declaration were unchanged. O(P+R) event storage was preallocated; no clocks, callbacks, new retries, or allocations entered payload transactions. Root buffers, workspaces, Stats, and event vectors were disposed before the timer closed. Only fixed scalar snapshots survived for logging.

The diagnostic fork passed 25 inherited native tests, three negative comparator fixtures, and 16 tiny full-reference calls with empty/tail/readiness/phase assertions. An initial driver-only JSON format-string compile failure was retained, then fixed without recompiling or changing the successful kernel/test artifacts. This diagnostic adds no Miri claim. Raw traces, detailed per-run summaries, source/binary hashes, and failed-attempt lineage remain in the preserved evidence package.

Source manifest SHA256: 3fb709e6d23a3a9dc1ad50c84aa8a9ee1f7880d00570254f8c459e29cd4ab7c7. Driver SHA256: 59d26d29d1a88469bd0acde503e62f47c8c908fddb6a5fe1ce22ed30fb537c07. Derived summary SHA256: 4569fe45738eb2d8c502bd1e7210f96cea67187db89b66c2f7d4c76c482187c8.
