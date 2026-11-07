<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Stable-rank/path root: measured phase constraints

These are two bounded descriptive diagnostics of the same current stable-rank/path root. They are different drivers, with different code placement and instrumentation. Their absolute durations must not be combined or treated as an A/B speed acceptance. No algorithm change or performance gain was accepted.

## Shared input and correctness protocol

Both studies used u64 and typed Row16 records, 10,000,000 records, SplitMix64 seed 1, eight workers, eight root shards, radix 256, shift 56, block size 128 for u64 and 64 for Row16. Every measured root used natural paths without fallback.

Each driver executed exactly 40 full API calls: two record shapes, one warmup plus three measured blocks per shape, five calls per block. Thus each study retained 30 measured calls and 10 warmups. A fresh input clone was prepared on a sorting worker before every API timer. Internal sort allocation and cleanup remained timed. Every output was compared with a complete stable reference and independently fingerprinted, including both key and payload for Row16. Input identity, exact route geometry and all raw observations were retained. No duration floor, adaptive repeat, rescue run or slow-sample deletion was used.

The second study also ran four separate 4099-record validation API calls, ordinary and instrumented roots for each shape, with duplicate keys and stable payload ids. Both instrumented validation calls exercised a three-record tail. These small calls were not used for timing claims.

## Study 1: existing root spans

The first driver only emitted the root's existing histogram, pre-scatter plan, scatter, restore and children clocks. It added no algorithm instrumentation. Each block bracketed shuffled Public, Full and Masked child modes with two calls to the unchanged native control.

Across all nine measured root calls per shape:

- u64: the four timed root spans occupied 56.20–63.84% of full API time; children occupied 36.04–43.66%.
- Row16: the four timed root spans occupied 59.24–68.97%; children occupied 30.75–40.57%.
- The separately timed pre-scatter plan was small: mode medians about 0.088–0.090 ms for u64 and 0.155–0.193 ms for Row16.
- Scatter and restore were the large root spans. The full-call residual was at most 0.061 ms for u64 and 0.108 ms for Row16.

The four-root-span subtotal is not full root elapsed. Its omitted gaps include actual setup, checking and destruction as well as clock boundaries. The residual is not pure timer overhead. Root and children are sequential in this research root; this study does not inherit producer/consumer overlap conclusions from another implementation.

Existing restore combined dependency discovery, planning, scheduling and movement. Its size alone did not establish which part dominated. Native after/before ratios ranged from 0.748 to 1.056 for u64 and 0.874 to 1.313 for Row16, so this was not a speed acceptance.

## Study 2: restore boundary split

An isolated diagnostic root reused the exact leaf and dependencies. Only five marked source additions were permitted: three Duration fields and a captured flag, an entry clock, boundaries around the unchanged execute_jobs call, and the final-tail boundary. Removing those additions reproduced the original source bytes. The root entry, discovery loops, LPT assignment, allocation, copy closure, scheduler, join, tail copy and unsafe blocks were unchanged. Six boundary clock reads were added per natural-path restore, with no inner-loop counters.

The planning span included incoming validation, path/cycle discovery, the long-chain decision, component ordering, LPT assignment, copy-accounting arithmetic, job vectors, temporary allocation and copy-closure construction. Move/join covered the unchanged execute_jobs call through its return. Metadata destruction appeared in a same-call restore residual.

Only Public child mode was used. Each block bracketed shuffled C1, C2 and D with native controls. C1 and C2 entered the same ordinary root body; D entered the instrumented clone. D was compared with the arithmetic mean of C1 and C2 within each block.

### Actual instrumented split

Ranges below are across three measured D calls per shape. Fractions were computed within each call before summarization.

| Shape | Discovery/planning ms | Planning / restore | Move/join ms | Move/join / restore | Restore residual ms |
|---|---:|---:|---:|---:|---:|
| u64 | 0.807–0.832 | 17.81–18.32% | 3.696–3.781 | 81.57–82.07% | 0.00495–0.00560 |
| Row16 | 2.241–2.639 | 19.15–24.26% | 8.163–9.454 | 75.68–80.76% | 0.00598–0.01041 |

At the measured length, both block sizes divide the input exactly. The tail span therefore measured an empty branch, 40–301 ns, not nonzero tail movement. The small correctness fixtures separately covered nonzero tails.

Discovery/planning was a minority of the instrumented restore. These observations do not support prioritizing parallel dependency planning on the premise that it dominates this interval. They do not prove that removing it would be cost-free or that another movement design would win.

### Instrumentation and control differences

The three per-block ratios are retained in execution order:

| Shape | C2/C1 total | D/ordinary-mean total | C2/C1 restore | D/ordinary-mean restore |
|---|---|---|---|---|
| u64 | 1.072, 0.942, 1.126 | 0.962, 1.126, 1.013 | 1.017, 1.006, 1.012 | 0.957, 0.975, 0.960 |
| Row16 | 1.058, 1.297, 0.914 | 1.031, 0.983, 0.941 | 1.032, 1.484, 0.926 | 1.019, 0.812, 0.919 |

The u64 instrumented restore was below its ordinary mean in every block, despite tighter repeated ordinary-restore controls. This systematic observed difference is compatible with code layout or instrumentation-related perturbation; its cause is not isolated. Row16 ordinary restore controls were substantially noisier. Native after/before ranges were 0.998–1.252 and 0.809–1.183, respectively.

Consequently, these splits describe instrumented calls, not precise uninstrumented phase costs. Move/join includes scheduling and synchronization, not just copying. No memory-bandwidth, cache-miss, useful-CPU-utilization or hardware-causal conclusion is supported. No independently computed medians were added or subtracted to invent a phase duration.

## Reproducibility identities

All source and binary identities were checked before and after their runs. Timing artifacts were hash-bound before the exclusive measurement window ended. Raw calls, warmups, fingerprints, geometry, strict parser results and resource records remain retained with the original research evidence.

- Current root source manifest: 00c9e7a791d3148843dda806d7a37139d73d8a93d8cf3b0fa8b3291a68d0bcd8
- Study 1 source plan: 91d80510566a72ec0254aecb4c56d16681437ebd86b5d9c4acaa2c83fe52a2ec
- Study 1 binary: 1d08bdd555c75291d98976f62f7f3e1611a63b007db2bcaaf10bfd44505bb4ec
- Study 2 source plan: 2efd044c0c80b6ea0fe3b37198962f1dd44fbda6cc8cca95e68396c55cb2c5dc
- Study 2 binary: 04778aa6c5c1a8366c12bfc8fc69ce82152a6fc5bfe581c17ce375cefcfc31a6
- Study 2 diagnostic root rlib: 4baa763fc36f44ce196f27f70d85fcf820d1917e390ea20c1b1475d076f92fc8

This summary intentionally excludes local paths and host details. It records phase constraints for subsequent design work, not a portable benchmark result or a performance acceptance.
