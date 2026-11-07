<!-- SPDX-License-Identifier: BSD-2-Clause -->
# R16 coarse-child completion diagnostic

## Finding

This experiment separates two interleaved costs in a stable parallel radix sort: a child partition's scatter/materialization work and its callbacks that scan and sort contiguous tiles. It is a mechanism diagnostic, not an optimized runtime patch or an accepted speedup.

Across the selected children in three measured runs per record type:

| Record type | Callback scan + sort / selected child CPU | Remaining child CPU after subtracting callbacks |
|---|---:|---:|
| u64 | 57.1–63.9% | 1.25–1.96 ms |
| 16-byte key/index record | 39.4–41.2% | 3.54–4.74 ms |

These are instrumented, same-child, same-run observations. They are **not calibrated percentages of the uninstrumented implementation**. Both callback work and parent partition work remain substantial.

The actual last-finishing child was logical bucket 15 in every measured leaf-timed run. Its service occupied nearly the entire final tail in five runs. In the remaining run, it occupied 70.8% of the tail, with a separate 2.65 ms publication-to-start delay. Ready tiles sometimes overlapped workers without active child-service intervals, but that observation does not establish usable scheduler capacity, bounded scratch allocation, or a sharing gain.

## Important limitations

The instrumentation comparison was not clean. One u64 leaf-timed call was 1.63× the contemporaneous native control. Another boundary-only call was 3.00× control, with its actual last finisher at bucket 2, outside the fixed focus. Both runs are retained. The latter child's service was about 59.53 ms, almost entirely charged as thread CPU. Its cause is unresolved; it was not discarded or labeled scheduling noise.

Only three measured runs per type were collected. Mode comparisons and same-code controls varied substantially. Sequential wall/CPU clock sampling and instrumentation costs remain in phase measurements. No hardware counters, cache-miss measurements, bandwidth measurements, scheduler traces, or allocator peaks were collected.

Even a zero-cost calculation for just the selected callbacks leaves parent work and often another child on the critical path. Such calculations hold other work fixed and are not predictions or global bounds for a different scheduler. No tile-sharing algorithm was implemented, and no optimization conclusion follows.

## Validation and scope

- Eight workers, CPUs 0–7, 10 million fixed-seed random records, root radix 16.
- Direct u64 and a 16-byte record containing a u64 key and original index.
- 214 fully reference-verified timing-stage calls, plus 60 separate correctness-only smoke calls.
- Every constituent call used a fresh input clone and checked the complete expected output.
- Actual affinity, selected SIMD dispatch, compiler configuration, root geometry, focused work counts, source identities, and binary hashes were checked.
- Earlier negative outcomes for radix reduction and a draining-consumer scheduler remain unchanged.

This packet intentionally contains only an English summary, methodology, and integrity metadata. Raw measurements, event traces, research harnesses, and diagnostic implementation remain outside the publication packet.
