# Rust engine port

This page retains measured evidence behind engine choices, not current development instructions. The initial-port results and follow-ups below describe their recorded revisions and environments; they are not current-head validation. Use the [benchmark protocol](../development/benchmarking.md#command-capabilities) for new measurements.

The engine in `rust/tessera-sort` is a Stable Rust port of the research prototype in `sort-research-rs` at commit `6c0877f0491dc3b8fc052aa529bce44bf8a53fa8`. The crate is unreleased (`0.0.0`, `publish = false`).

## Eight-core parallel completion

These are local whole-call algorithm comparisons, not isolated source-change or placement-controlled claims. The portable bounded-finishing implementation and its fallback are described in the [engine reference](engine.md#parallel-root-completion).

The host was an AMD Ryzen 7 8845H with eight physical cores. Runs used CPUs `0,2,4,6,8,10,12,14`, one SMT sibling per core; SMT remained enabled, the governor was `powersave`, and boost remained on. The compiler was Arch Rust 1.99.1, LLVM 23.1.3, `x86_64-unknown-linux-gnu`, release profile, default target flags and benchmark features `compare,rayon`. No machine power settings were changed.

The maintained command was:

```sh
pnpm tools compare algorithms --plan parallel \
  --types u32,row_u32,u64,row_u64 \
  --distributions random,d256,zipf2,sorted,reverse \
  --lengths 1000000,10000000 --cpu 0,2,4,6,8,10,12,14 \
  --out /tmp/tessera-goal-lab/delivery-matrix
```

It completed 40 inputs, nine shuffled blocks per input, serial Tessera A/A, the eight-worker Tessera adapter, standard stable sort and Rayon stable-8. Every timed output was verified. Peak allocation was measured separately; the memory worker now includes selected Tessera parallel adapters at large non-`u64` types as well. Reports and raw journals stay outside Git at the path above. The measured library source hash was `d1f7097916fbf360a80771f6e6bb691cb08b75f8b4cdc142e43eef89fdd6c3bc`; the timing executable hash was `57b1d4c6dcabee8cd8c3cddc7fbfbae681e96f08d1e7afbf8bb182fe42a08d4d`.

Speedups below are reciprocals of paired median time ratios against the mean of the serial A/A pair, not ratios of independent medians. Ranges span the nine blocks. Peak heap excludes input preparation, verification, the warmed pool and thread stacks.

| 10M input | Eight-worker speedup | Block range | Serial A/A envelope | Parallel peak heap |
|---|---:|---:|---:|---:|
| Random `u32` | 6.01× | 4.86–6.62× | 0.981–1.019 | 12,076,304 bytes |
| Random 8-byte `{u32, u32}` | 4.58× | 4.32–4.77× | 0.987–1.013 | 18,909,498 bytes |
| `d256` `u32` | 1.97× | 1.59–2.15× | 0.995–1.005 | 65,536 bytes |
| `d256` 8-byte `{u32, u32}` | 1.83× | 1.79–1.89× | 0.976–1.024 | 15,757,602 bytes |
| Random `u64` | 3.80× | 3.59–4.12× | 0.996–1.004 | 18,275,618 bytes |
| Random 16-byte `{u64, u32}` | 2.95× | 2.79–3.17× | 0.994–1.006 | 29,635,068 bytes |

The fivefold example is demonstrated for random `u32`, not for both primary record representations. At 10M, Zipf2 measured 0.96–1.00× across these four types. Sorted inputs measured 0.42–0.89× and reverse inputs 0.87–0.94×; those adaptive paths did not allocate heap and did not use bounded completion. They are real limitations, not a universal scaling claim.

An earlier executable without parallel small-window counting (`final-matrix`) measured sorted `row_u32` at 1.41×, whereas the later `narrow-matrix` and delivery builds measured 0.43× and 0.42×. This exceeds the controls. Inspection confirms the sorted path still returns from the shared order probe before partitioning or counting, and its measured peak remains zero; both the serial and parallel absolute scan times changed between executables. The source of that cross-build shift is unresolved: no placement-controlled experiment was performed, so it is not dismissed as noise or attributed to the new counting algorithm.

Additional retained comparisons cover 4M, 8M and 16M random primary inputs (`size-scaling`): `u32` measured 4.09×, 6.23× and 5.56×; 8-byte records measured 4.76×, 4.56× and 4.09×. Their A/A envelopes were 0.945–1.055, 0.991–1.009 and 0.996–1.004 for keys, and 0.974–1.026, 0.992–1.008 and 0.991–1.009 for records. The 16M record case retained 26,005,472 bytes of peak sort heap. The independent `delivery-general-valid` run completed 15 additional 1M inputs across floats, 128-byte records and owned records with random, repeated, skewed, sorted and reverse keys. The comparison harness does not support `u128`; correctness of both narrowed 128-bit halves is tested through the public API instead.

Blocks at two and eight times the balanced size were also screened (`scale-two`, `scale-eight`). Random 8-byte records measured 4.22× and 4.35×, with serial A/A envelopes 0.981–1.019 and 0.986–1.014. The eight-times layout used 28,207,372 bytes of peak heap on that input. These separate-executable screens did not justify replacing the four-times layout; they do not isolate a causal block-size effect.

Remaining work includes improving record scaling beyond 5×, understanding the ordered-path build sensitivity, wide-record and skew scaling, and independent source/placement controls. No result here establishes a speedup on other machines or a gain attributable to one isolated source edit.

### Memory-organization diagnostics at `7ab58c9`

A follow-up investigated random seed-one `u32` and 8-byte `{u32, u32}` records at 10M on the same eight physical cores. The sorting implementation remained at `7ab58c9`; none of the candidates below was accepted. Full reports and primary raw evidence are retained outside Git at `/home/lyk/code/tessera-research/parallel-7ab58c9/`, with an index in `README.md` and file hashes in `SHA256SUMS`. This is a regular directory outside both temporary storage and the user cache. Its three analysis reports cover the streaming reference (`roofline/ANALYSIS.md`), movement counters (`cost-attribution-ANALYSIS.md`) and sorting-scoped hardware sampling (`uprof-scoped-ANALYSIS.md`).

#### Logical movement and contention

Temporary per-bucket instrumentation ran through the maintained comparison pipeline. Counters were aggregated after joining; journals identified repeated relocations of each immutable source prefix. The twenty finishing invocations per type include validation, warm-up, timed calls and allocation measurement, not just the nine timed samples. Instrumented durations are diagnostic: probing `try_lock` before a contended acquisition and recording copy statistics can change scheduling and cache behavior.

| Median metric | `u32`, 40 MB input | 8-byte records, 80 MB input |
|---|---:|---:|
| Gather calls | 23,503 | 32,746 |
| Relocated record bytes | 30.377 MB | 63.059 MB |
| Relocation bytes / input bytes | 0.759 | 0.788 |
| Bytes moved after a source's first relocation | 12.119 MB | 26.020 MB |
| Maximum relocations of one source | 8.5 | 9 |
| Final write calls | 22,776 | 31,900 |
| Physical-owner lock acquisitions | 83,652 | 119,146.5 |
| Contended physical-owner acquisitions | 5 | 12 |
| Summed blocked acquisition time | 651 ns | 3,044 ns |
| Contended free-cache acquisitions | 0 | 0 |
| Peer `try_lock` failures / `yield_now` calls | 0 / 0 | 0 / 0 |

Fractional event counts are medians over an even number of invocations. Most record writes were 2–4 KiB; most `u32` writes were 1–2 KiB. Gathering and final writing each transferred the whole input once. Relocation bytes count moved records, not reads plus writes: moving 63 MB adds approximately 126 MB of logical payload traffic. None of these quantities is a measurement of DRAM traffic. Lock blocking was negligible in these samples; false sharing, coherence traffic and uncontended lock costs remain separate questions.

The diagnostic bucket-finish elapsed medians were 4.484 ms for keys and 7.728 ms for records. Summed gather/sort/write durations were 5.468/24.730/3.909 thread-ms for keys and 16.681/29.400/13.305 thread-ms for records. These accumulated wall durations include stalls and overlap across workers; they are neither CPU times nor additive critical-path durations. The counters and their instrumentation are archived as `cost-attribution*`.

#### Streaming references are not a DRAM roofline

Unmodified STREAM source at `jeffhammond/STREAM@6703f7504a38a8da96b353cadafa64d3c2d7a2d3` was compiled outside the repository with GCC 16.2.1, `-O3 -march=native -fopenmp`, twenty iterations and 80 MB or 160 MB per array. The eight-worker copy reference measured approximately 35 GB/s with ordinary cached stores and 56 GB/s with large non-temporal copies. Rates count payload reads plus writes and summarize three processes' average iteration times; these calibration runs have repeat ranges, not the sorting runner's paired A/A controls. Compiler flags, source hashes, validation and the store-policy distinction are retained in `roofline/`.

For 8-byte records, five nominal whole-input record copies represent 800 MB of logical read/write traffic. The uninstrumented sorting median of 11.557 ms would imply 69.2 GB/s, exceeding the large-copy reference because tile traffic reuses caches. Conversely, write allocation and metadata can add traffic omitted by nominal copies. The earlier 4.081 ms root scatter corresponds to 39.2 GB/s of logical payload throughput, about 70% of the large-copy reference, not 70% DRAM utilization. No reliable whole-sort distance from a DRAM roofline was established.

#### Sorting-scoped hardware samples

AMD uProf 6.0.1520 collected separate IBS OP and CPI profiles. The initial release profile lacked debug information and reliable stack unwinding, so its whole-process totals were not used for sorting-only attribution. A diagnostic build added debug information, frame pointers and a temporary controller scope around Tessera's `ThreadPool::install`. Profiling started paused: input generation, pool construction, other algorithms, restores and verification remained outside the enabled windows. The memory worker's separate process was excluded from the tables below. The profiler permission policy changed externally from `perf_event_paranoid=2` to `-1`; the experiment did not change power, SMT or kernel-symbol settings.

The diagnostic build retained the baseline sorting source. Its manifest source hash, including the benchmark-only controller patch, was `7f740d7bbc2cffa33cea0fa5a5bb186e8ac66df41bbfb83f06f9ce283381a758`; its executable hash was `52bc21e5af7f1e97c31cbcc1799a1a744c8303a612c3e63775bb4ed69c8d56e7`. The retained runner performed eighteen parallel calls in the main timing process, including calibration. These builds and profiled times are not acceptance measurements. The patch, flags, manifests and executables are in `uprof-scoped-build/` and `uprof-scoped-harness.patch`.

The main process's CPI profile contained 21,206 cycle samples:

| Function category | Self cycle samples | Share |
|---|---:|---:|
| Tile sorter | 8,316 | 39.22% |
| Root scatter | 4,874 | 22.98% |
| Copy instructions | 5,465 | 25.77% |
| Bucket finishing outside callees | 2,183 | 10.29% |
| Other | 368 | 1.74% |

These are sampled active-core cycle proportions, not wall-clock critical-path percentages. Inclusive Rayon call-graph counts overlap and must not be summed. Dynamic callers identify parallel shard work even when folded generic code receives another partition symbol's name. Instruction/op sample shares also differ from cycle shares: the tile accounts for about 70% of sampled ops but 39% of sampled cycles. Reports are in `uprof-scoped-cpi/`.

The tile's IBS profile contained 5,902 sampled loads: 5,493 L1 hits, 392 L2 hits, 14 local-cache hits and 3 local-DRAM hits. About 93.1% hit L1 and 6.6% hit L2. Its 409 sampled L1 load misses averaged 16.84 cycles, with about 80.6% of accumulated sampled miss latency attributed to L2 service. Of 4,765 sampled stores, 1,335 missed L1 (28.0%). These are sampled demand operations, not byte fractions: prefetches, write allocation and writebacks are not accounted by the hit percentages. They establish neither DDR nor L2 bandwidth saturation. Reports and raw captures are in `uprof-scoped-row32/`.

The balanced two-pass plan over the remaining contiguous 24 key bits uses twelve-bit digits, or 4,096 buckets per pass. One active 64-byte line per bucket represents 256 KiB, larger than the 32 KiB L1 data cache, while much of the private tile working set can fit in the 1 MiB L2. Cache-level locality is therefore a relevant next hypothesis. Precise false-sharing attribution and memory-controller read/write traffic remain unmeasured.

#### Candidate outcomes and remaining work

Index-only leaves and indexed final writes did not demonstrate a whole-call improvement: the first record screen measured 14.823 ms versus the earlier 11.557 ms baseline, and packed-key variants measured 18.379 and 15.844 ms. Separating complete-band protection from copying measured 12.420 ms; a frozen-directory wave prototype measured 15.294 ms. These are separate-build development screens, not isolated source-causal comparisons. Native tests passed, but no candidate received full production validation or was retained as an optimization.

Cache-line-padded physical owners did not demonstrate a benefit; that record screen's A/A envelope was 0.909–1.091, so it does not settle the false-sharing hypothesis. Source-aware placement, preferring free blocks strictly inside an unread source's own final band, reduced instrumented record relocation volume to 53.405 MB and repeated volume to 16.548 MB. Its whole-call timing benefit is unverified. A sorted-local-free-list variant avoided linear preferred-block selection but likewise established no reliable speedup.

Later unchanged-baseline repeats also shifted beyond earlier controls. Increasing the sample target to 100 ms and prepared cap to 1,024 MiB for both baseline and candidate did not settle the variation: the baseline record A/A envelope still spanned 0.831–1.169. That experiment also changed the prepared working set and is not directly comparable with the earlier 5 ms sessions. No serial gain or regression is attributed to these source changes; the serial core was not modified, and future candidates must preserve its performance rather than trade it for parallel speedup.

These diagnostics motivated separate final-scatter fusion and smaller-fan-out experiments, recorded [below](#final-scatter-fusion-and-narrower-digits). Neither the diagnostics nor those follow-ups establish a new production algorithm or routing choice.

### Final-scatter fusion and narrower digits

A bounded follow-up implemented two portable prototypes against the `7ab58c9` sorting baseline. **A** retained the existing one/two-pass LSD plans but scattered the last pass directly into the final input band. **B** separately capped those eligible LSD plans at eight bits, producing three passes for contiguous remaining 24-bit variation. Insertion, longer baseline plans that may choose MSD, oversized buckets and public APIs were unchanged. Prototypes, source snapshots, test logs, retained binaries and raw observations live outside Git at `/home/lyk/code/tessera-research/final-scatter-fusion/`; its `README.md` indexes the experiments and `SHA256SUMS` checks their files. Neither prototype was retained in the production engine.

Both prototypes completed the last histogram and cached user classifications before protecting the entire output band. No metadata lock spanned a user callback. A callback-free scatter then committed final ownership, disarmed tile restoration and suppressed the outer staging copy. A key panic before that transfer restored staging and used the existing recovery write. This reused the bounded allocations and preserved the O(P√n) space argument.

The maintained comparison pipeline temporarily registered two independently compiled baseline crates, fixed A, and two calls to B in one executable. The serial Tessera entry remained first in the catalog, preserving the runner's serial A/A contract. The parallel pairs were additionally summarized with the maintained `summarizePaired` function, using the two baseline samples' mean per block; they must not be confused with the standard report's serial denominator. Independent baseline copies had distinct code addresses, but independent B placement copies were not tested. These are bounded development screens, not final source/placement attribution.

The host and compiler conditions matched the preceding investigation: eight physical Ryzen 7 8845H cores, CPUs `0,2,4,6,8,10,12,14`, powersave governor, boost and SMT enabled, Arch Rust 1.99.1, release default target flags and `compare,rayon`. No power or kernel policy changed. Each acceptance session used 10M seed-one random records, nine shuffled blocks, the default 5 ms target and 256 MiB prepared cap; every timed output was verified. The valid-control primary screen is `A-screen-valid/`, followed by `B-screen/` and the four-type `AB-matrix/`. The initial `A-screen/` accidentally placed a parallel adapter first in the catalog, invalidating its serial A/A pair; it is explicitly excluded.

| 10M random input | A / parallel baseline | B / parallel baseline, two calls | Baseline parallel A/A envelope | B parallel A/A envelope |
|---|---:|---:|---:|---:|
| 4-byte `u32` | 1.102 | 1.254 / 1.228 | 0.887–1.113 | 0.801–1.199 |
| 8-byte `{u32, u32}` | 1.061 | 1.160 / 1.119 | 0.828–1.172 | 0.867–1.133 |
| 8-byte `u64` | 0.994 | 0.957 / 1.039 | 0.897–1.103 | 0.899–1.101 |
| 16-byte `{u64, u32}` | 0.982 | 0.980 / 0.990 | 0.875–1.125 | 0.923–1.077 |

Lower ratios are faster; ratios are paired medians, not quotients of independent absolute medians. In the matrix's 8-byte record case, baseline absolute medians were 14.537/14.816 ms, A was 16.192 ms, and B was 17.143/17.767 ms. The earlier valid A screen measured A at 16.809/17.468 ms versus baseline 15.900/16.342 ms, with A's own A/A envelope 0.894–1.106. No session demonstrated an acceptable primary-input gain. B also showed a slower direction on `u32`; apparently faster wide-key rows remain within controls and mostly exercise unchanged longer-plan fallbacks. This does not justify changing the serial engine or claiming that either hypothesis is universally ineffective.

A separate instrumented session (`movement/`) counted actual record transfers at the gather, relocation, tile-scatter, guard-restore and final-copy sites. It covered 19 baseline, 19 A and 20 B finishing invocations, including validation, calibration, timed calls and B's memory-worker call. Diagnostic counters alter scheduling; their times are not acceptance evidence.

| Finishing payload moved per 80 MB input | Baseline | A | B |
|---|---:|---:|---:|
| Gather | 80 MB | 80 MB | 80 MB |
| Ordinary tile scatters | 160 MB | 80 MB | 160 MB |
| Fused final scatter | 0 | 80 MB | 80 MB |
| Guard restoration | 0 | 0 | 0 |
| Separate final copy | 80 MB | 0 | 0 |
| Median source relocation | 62.733 MB | 64.521 MB | 63.713 MB |

A therefore removed one real 80 MB transfer. B's three scatters moved the same 240 MB as the baseline's two scatters plus final copy. Relocation changed only slightly. These are logical payload bytes, excluding root scatter and metadata, not measured cache-link or DRAM traffic.

Sorting-scoped IBS used one retained diagnostic executable with debug information, frame pointers and the installed uProf controller. Sampling was enabled only around the selected complete parallel sort. The main processes contributed eighteen baseline calls, eighteen A calls and thirty-six B calls; B's two registered calls share an adapter. Memory children were excluded. Working reports are `ibs-baseline/`, `ibs-a-sqlite/` and `ibs-b-sqlite/`, with aggregation in `ibs-summary.json`. Initial A/B DuckDB-backed translations yielded empty process/function reports and were excluded; SQLite-backed recollections supplied their tables. Kernel-symbol mismatch warnings remain, so this analysis concerns user-space functions.

| Tile and fused-output function self samples | Baseline | A | B |
|---|---:|---:|---:|
| Store samples | 5,984 | 5,957 | 17,490 |
| L1 store-miss samples | 1,648 | 1,676 | 1,502 |
| Store-miss fraction | 27.54% | 28.13% | 8.59% |
| Load L2-hit fraction | 6.65% | 4.41% | 2.72% |
| Mean sampled L1 load-miss latency | 19.38 cycles | 28.27 cycles | 40.98 cycles |

The groups include the tile kernel plus fused output/protection when emitted separately; they are not instruction-identical regions. A's final-output function alone had 745 misses among 1,430 store samples (52.10%). B's final-scatter function had 395 among 5,944 (6.65%). B showed a lower sampled store-miss fraction in these differing function populations; final-write locality and the timing causes remain unisolated. A removed a real copy without demonstrating a whole-call benefit. B also adds a full histogram/key-extraction round, and its combined tile/output op samples per profiled call increased. Neither operation samples nor their latency sums are wall-clock shares or cache bandwidth measurements; no single cause has been isolated for the timing outcome.

A passed focused native and Miri ownership/panic tests and an independent static Agentic Review. B additionally passed the Rayon all-target tests, Rayon plus Portable SIMD tests, strict crate Clippy, and forced one/two/three-pass Miri cases. Tests covered stable equal keys, owned drop counts, inconsistent keys, exclusive non-`Sync` borrows, first/last histogram panics, adjacent bands, relocation, and a native 65,535-record three-byte tile with cache-tail IDs. A preliminary B Miri fixture was too small to select the intended three-pass route; it failed its route assertion, not an ownership check, and was replaced by a correctly sized fixture. No accepted engine change remains, so the current algorithm models and contracts are unchanged.

Neither experiment establishes the dominant whole-call bottleneck. Further radix, final-scatter and relocation tuning is paused in favor of measurement identifiability and completion-path comparisons below. A ten-bit cap alone is not a distinct 24-bit plan: the existing balanced planner still produces 8+8+8. Mixed radix, software write combining, new relocation policies and lock restructuring were not measured in this follow-up.

### Controlled completion-path diagnostics

The next investigation compared bounded finishing with the existing block-permutation/compaction fallback, without changing the production engine. Evidence, temporary source copies, setup scripts and retained pipeline sessions are in `/home/lyk/code/tessera-research/materialization-paths-3b13726/`. This is a local diagnostic screen, not a production routing decision, a bandwidth roofline or a layout-causal performance claim.

#### Controls and remaining variability

Read-only topology checks confirmed that CPUs `0,2,4,6,8,10,12,14` cover eight distinct physical cores on the Ryzen 7 8845H. Governor, boost and SMT remained unchanged; desktop applications remained active. Each temporary worker optionally pinned itself to one of those CPUs and asserted its actual allowed mask before warm-up. Pinning commands and assertions are preserved with the experiment source; this temporary environment setting is not a standard session-manifest field. Group observations retain the maintained runner's boundary load, temperature and first-allowed-CPU frequency, not a per-core frequency timeline.

Two separately compiled source copies per path, each with two registered calls, distinguished same-entry repetitions from between-copy observations. Distinct emitted entry addresses were checked, but instruction equivalence was not established; an entry may also inline. These copies are not samples of a randomized layout distribution. Same-entry A/A reflects execution variability; between-copy differences still mix execution, possible code-generation differences and placement sensitivity. Neither repeating one binary nor comparing two addresses isolates layout bias.

All uninstrumented sessions used the maintained `pnpm tools compare algorithms` pipeline, nine shuffled blocks, verified seed-one random 10M inputs, the default 5 ms target and a 1 MiB prepared cap. The cap forces exactly one input buffer at every selected width, avoiding variant-dependent prepared working sets. Serial Tessera remained the first catalog entry and retained its normal A/A pair. Parallel summaries in `path-paired.json` separately reuse the maintained `summarizePaired` function; normal report denominators remain serial. All four acceptance-screen timing executables had the same SHA-256, `c4a6256c0d52fdef6ad37d2a9abb676ba5f589c2927f62a4124b1067c7701448`.

| 8-worker 8-byte records | Bounded A same-entry A/A | Bounded B same-entry A/A | Bounded between-copy A/A |
|---|---:|---:|---:|
| Inherited CPU mask | 0.799–1.201 | 0.836–1.164 | 0.842–1.158 |
| Per-worker pinning | 0.902–1.098 | 0.915–1.085 | 0.893–1.107 |
| Per-worker pinning repeat | 0.851–1.149 | 0.897–1.103 | 0.782–1.218 |

The sessions are `inherited-8-valid/`, `pinned-8/` and `pinned-8-repeat/`. An earlier CLI invocation failed before measurement because its redirected output occupied the CLI's reserved log path; it supplies no timing evidence. Pinning did not establish a stable small-effect detection window. These observed extrema are neither confidence intervals nor a minimum detectable effect estimate. Reliable identification of a 5–10% source gain remains unresolved; no CI threshold, compiler flags or machine policy was changed to hide it.

#### Same root, different completion plans

All diagnostic copies retained the root digit planner and bounded limit, forced eight scatter shards at pool widths 1/2/4/8, and kept the bounded route's factor-four root block size. Only the materialized copies declined bounded finishing after the common scatter and directory adoption. Setting the bounded limit to zero instead would also change the root block size and confound the comparison. One-worker diagnostic copies bypassed the public serial shortcut; stock `tessera-par-1` was measured separately and still used the serial engine.

The materialized path retained its existing scoped jobs and 64-KiB tile budget; bounded finishing retained its larger bucket-local tile budget. Thus this compares complete completion plans, including their different leaf planning, rather than isolating a copy or holding leaf work constant. No new dataflow algorithm was implemented.

The four-type `scaling/` session measured every width with both copies and repetitions, plus stock parallel adapters and serial controls. Entries below are independent absolute medians for copy A, in milliseconds, useful for describing scaling rather than paired verdicts:

| 10M random input | Bounded, 1 / 2 / 4 / 8 workers | Materialized, 1 / 2 / 4 / 8 workers |
|---|---|---|
| `u32` | 38.16 / 20.67 / 12.08 / 7.67 | 49.06 / 26.96 / 15.93 / 11.08 |
| 8-byte `{u32, u32}` | 52.21 / 29.35 / 17.65 / 13.96 | 65.71 / 35.83 / 22.80 / 16.84 |
| `u64` | 80.51 / 44.38 / 25.39 / 18.33 | 71.91 / 40.05 / 25.41 / 17.01 |
| 16-byte `{u64, u32}` | 110.66 / 65.68 / 42.94 / 35.54 | 101.00 / 58.53 / 41.41 / 31.71 |

At eight workers, materialized copies A/B had paired median ratios of 1.334/1.337 for `u32`, 1.189/1.151 for 8-byte records, 0.977/0.970 for `u64`, and 0.864/0.905 for 16-byte records, relative to the mean of the bounded A/B first calls in each block. Only the `u32` direction passed that control rule for both copies; the 8-byte record verdict differed by copy, and both wide-key rows remained within those controls. Same-entry repeat envelopes provide additional execution-variability checks, not replacement denominators. These results do not justify switching production routes. They do show why a single record width cannot select an overall completion strategy.

#### Overlap and the compaction chain

A separate instrumented `trace-run/` recorded per-worker nested wall intervals into private buffers and flushed them after sorting joined. `trace-events/` retains timestamps, Linux thread IDs and worker indices; `trace-summary.json` separates process IDs and call counts, keeping one-call validation populations separate from the eighteen-call timing populations. The maintained memory policy skipped these temporary adapters on this non-`u64` large input; no diagnostic-path allocation result is claimed. Main-process aggregates include eighteen sorts per path and width: nine calibration and nine timed calls. They are diagnostic populations, not acceptance timings. Root layout markers confirmed eight shards, 256 buckets and 316 records per root block for both paths at every width on the 8-byte record input.

At eight workers, bounded completion's median wall span was 9.31 ms after a 4.83 ms root scatter/adoption span. Gather, leaf sorting and protected writes overlap across workers: their wall unions were 7.58, 8.48 and 7.26 ms, respectively. A union measures coverage by at least one interval, not CPU work or an exclusive contribution; these values must not be added.

For the materialized path, median root scatter/adoption was 5.26 ms, block permutation 5.07 ms and compaction-with-handoff 7.43 ms. Leaf intervals overlapped compaction for 7.32 ms; the post-compaction tail was only 0.19 ms. At one worker the corresponding compaction span was 6.82 ms, without concurrent leaf execution. Medians of separate intervals need not sum to the median whole call. The trace exposes a sequential compaction chain that remains near the end of this path despite concurrent leaf jobs; adding more leaf jobs alone cannot remove that chain. It does not isolate whether overlap, memory contention, scheduling or instruction work explains its duration.

The next measurement priority remains repeated same-entry controls with per-core frequency and scheduler-state observations. RAPL energy reads were permission-denied, the uProf power driver was unavailable, and scheduler tracefs access was denied; none was changed. uProf Threading Analysis was not collected in this round. Wall spans include descheduling and are not on-CPU/off-CPU classifications. No hardware-memory-bandwidth limit or irreducible data dependency has been established. New materialization research should use these complete-path controls before adding ownership complexity; smaller radix, write-combining and relocation micro-tuning remain paused.

### Repeated-key root sharing

A cache-lock batching candidate after the parallel baseline was not adopted. Exact-head continuous feedback showed no parallel alert, and a local reproduction using its original CI executables found the targeted 10M `row_u32` random case 0.44% slower in Default and 3.16% slower in Portable SIMD against the mean of two baseline calls. Those three-operation reproductions are not nine-block acceptance; they preserve a failed hypothesis rather than a performance win. Artifacts, hashes and all observations are retained under `/home/lyk/code/tessera-research/parallel-completion-06f6068/`.

The next candidate addresses a real serial route: a sampled repeated key previously handed the entire root to the serial node sorter. A 262,144-record fixture with stable payloads and `Send`-only records failed its multi-worker assertion before the change. The candidate encodes the existing below/equal/above policy as a two-bit digit, reuses sharded scatter and materialized completion, skips the equal band, and sorts the two outer bands independently. It adds no unsafe code, new threshold or public bound. The rank adapter intentionally does not forward record counting/filling. Native tests cover worker use, owned `Send`-only records under a worker key panic and changing repeated keys; sequential ownership fixtures cover rank boundaries and stable payloads.

Two initial maintained algorithm sessions used 10M seed-one `row_u32/skew99`, four workers on physical cores `0,4,8,12`, nine shuffled blocks per build, default 5 ms sampling and a 256 MiB prepared cap. Parallel whole-call medians changed from 16.72 to 15.25 ms in Default and 17.82 to 15.75 ms in SIMD. These are separate-executable development screens, not paired source-revision gains. Default serial A/A bounds were 0.9913–1.0087 before and 0.9260–1.0740 after; SIMD bounds were 0.9091–1.0909 and 0.8291–1.1709. The broad controls, changing serial absolute times and lack of independent placement controls prevent a precise source-causal claim. Full blocks, allocation observations, manifests and retained binaries are in `repeated-root-before/` and `repeated-root-after/` under that same evidence directory. Wider controls, matching-configuration current-main feedback and source attribution remain acceptance work, not established improvements.

### Serial artifact stability

The actual Portable SIMD CI artifacts for baseline `fafaaa2` and parallel candidate `a26cac4` were investigated on a Ryzen 7 8845H for `row_u64/equal/48`. The remote alert was +6.9%. An earlier local CPU-2 source screen overlapped another worktree's CPU-2 benchmark and had A/A bounds of roughly ±36%; it cannot adjudicate this alert.

The artifact experiment instead pinned every process to CPU 14, on another physical core, retained the original binaries and ran nine shuffled blocks. Each block repeated the identical baseline executable for A/A and included two instruction-preserving probe relocations per revision. The candidate/original-baseline paired median was **1.0231**, with all nine original-candidate ratios above one, but the final A/A envelope was **0.9672–1.0328**. The candidate's two relocated-probe medians were **1.0256** and **1.0088**. These observations support placement sensitivity and retain a possible small whole-binary cost; the broad final controls do not establish its exact magnitude or eliminate it.

The probe's normal-path bytes were identical, not merely equal in size. Single-stepping both a verification call and a warm-up call found the same **348 executed instructions**, including **317 probe instructions and 48 key loads**. The steady batching caller executed the same nine instructions between sorts. The equal input returned from the order probe without allocation, insertion or partitioning. Whole-process cycle profiles put about 78% of each probe's samples in the same equal-key scan loop; they revealed no additional sorting stage. Those profiles include untimed preparation and statistics, and sampled instruction positions are not per-instruction stall attribution.

Relocation changed only a serial caller's displacement and the copied probe's address, with its cold PC-relative references repaired. The spare slots were unreachable under the explicit saved single-case plan; full-suite execution of these diagnostic binaries is invalid. Reference verification passed before measurement. These interventions are not source changes or deployable layout workarounds, and the local AMD result does not identify the Intel runner's microarchitectural cause. Raw binaries, hashes, scripts, instruction traces and profiles remain outside Git in `~/code/tessera-research/serial-artifact-attribution/`. Exact-head continuous feedback remains a separate acceptance requirement.

## What changed

The initial port keeps the research pipeline: order probe, budgeted insertion, variation scan, root narrowing for 128-bit keys, node routing, digit planning, tile leaves and stable reversal of equal-key runs. A measured follow-up recognizes strictly descending keys during order confirmation and reverses them directly; descending inputs with equal keys keep the stable reversal.

The port changes how the engine is reached, not what it decides:

| Prototype | Port |
|---|---|
| Nightly specialization selects the record path | `Extract` trait with two implementations: `ByKey` for user key functions and `Direct` for `stable_sort` |
| `Copy` records skip cached bucket IDs and initialize scratch on the first pass | `stable_sort_by_key` always caches bucket IDs; `stable_sort` recomputes library-encoded keys |
| Open `Key` trait | Sealed `RadixKey` for primitive integers, `f32`, `f64` and `Reverse<K>` |
| Wide-key narrowing casts the record slice | Narrowing wraps the extractor (`KeyHalf`) |

Free functions create tile/block scratch only after the root fast paths, so confirmed ordered and constant inputs do not allocate. Budgeted insertion on a larger unsorted input can allocate an encoded-key probe buffer before the varying-bit scan. Block metadata is created only when a node needs a block partition.

## Correctness

The tests are written for this repository and use only the public API and engine internals. They cover every key encoding, float edge cases, thresholds around the insertion limit and tile size, 128-bit key halves, over-aligned and zero-sized records, owned records (`String`, drop tracking) and `&mut` records under panicking and changing key functions, per-call scratch release and allocation-free ordered inputs.

Run locally on x86-64 Linux: Stable (debug and release), Rust 1.85, Miri, AddressSanitizer, Clippy, rustdoc and `cargo package`. GitHub CI then passed on Stable and Rust 1.85 on Linux (x86-64 and AArch64), macOS (AArch64) and Windows (x86-64), with Miri and AddressSanitizer on Linux x86-64.

## Performance

The prototype needs Nightly specialization, so the port was measured against it inside one Nightly binary of the research harness (`radix_ab`). This isolates the source port; it is not a Stable-vs-Nightly comparison. The harness and the raw data stay outside this repository because the harness is Apache-2.0 code.

Method: fixed seed, one pinned core, variant order rotated every round, input restored outside the timed region, every output checked against a stable reference, scratch allocation included in the time. Each case reports the median over rounds of `port / mean(prototype A1, prototype A2)`. The same binary also contains:

- a second copy of the prototype in another crate, to expose code-placement effects;
- the prototype forced onto cached bucket IDs, to separate the planned caching policy from the port itself.

| Matrix | API | Cases | Processes × rounds | Median ratio | Range |
|---|---|---:|---|---:|---|
| Default build, small sizes | `stable_sort_by_key` | 105 | 3 × 21 | 0.984 | 0.58–1.20 |
| Default build, small sizes | `stable_sort` | 63 | 3 × 21 | 0.995 | 0.62–1.11 |
| Default build, full matrix | `stable_sort_by_key` | 136 | 1 × 9 | 1.004 | 0.53–1.14 |
| Default build, full matrix | `stable_sort` | 92 | 1 × 9 | 0.990 | 0.60–1.23 |
| `target-cpu=native`, fat LTO | `stable_sort_by_key` | 45 | 1 × 9 | 1.001 | 0.81–1.12 |
| `target-cpu=native`, fat LTO | `stable_sort` | 27 | 1 × 9 | 0.994 | 0.84–1.07 |

The default build is the harness's thin-LTO profile. Medians weigh every case equally; they are not an application-level speedup.

Eight of the slowest cases were rerun with 3 processes × 21 rounds. The cases that stayed outside the control range:

| Case | API | Ratio | Port / cached prototype |
|---|---|---:|---:|
| `record128` zipf 1.3, n = 128 | by key | 1.168 | 1.030 |
| `u128` 256 distinct, n = 48 | by key | 1.121 | 1.028 |
| `record128` 256 distinct, n = 128 | by key | 1.113 | 1.017 |
| `record128` local random, n = 100,000 | by key | 1.067 | 1.004 |
| `u128` descending, n = 100,000 | keys | 1.275 | 1.005 |
| `u128` 256 distinct, n = 48 | keys | 1.088 | 1.021 |

The `stable_sort_by_key` cases stay within 3% of the prototype forced onto cached bucket IDs. Those differences come from the caching policy that the port adopts on purpose, plus code placement.

`u128` descending with `stable_sort` was the only rerun case that was slower in almost every round (60 of 63). A descending input takes only the order probe and the stable reversal, and the machine code of `stable_sort::<u128>` is identical to that of `stable_sort_by_key::<u128>`, which is not slower. Its instructions differ only in addresses. In a copy of the binary, redirecting the one call into the `stable_sort::<u128>` engine body to the `stable_sort_by_key::<u128>` body lowered the ratio from 1.19–1.41 to 0.99–1.05 across three alternating processes. The gap is therefore caused by code placement, not by the port.

The same intervention explains the largest gaps in either direction on ordered `u32` inputs (0.53–1.01 before, 0.98–1.01 after redirecting six instruction-identical probe functions to one address).

### Rebuild and branch-alignment check

Rebuilding the harness from the final source, which differs from the measured source only in comments, reversed these outliers. Building it with the LLVM option `-x86-branches-within-32B-boundaries`, which keeps branches from crossing or ending on a 32-byte boundary, removed them. The development machine is a Xeon Gold 6140 (Skylake-SP). Its microcode mitigation for Intel's JCC erratum keeps such branches out of the decoded-µop cache, which points to that cache as the mechanism. Each build ran 3 alternating processes × 21 rounds:

| Case | API | Measured build | Rebuild | Rebuild, branch-aligned |
|---|---|---:|---:|---:|
| `u128` descending, n = 100,000 | keys | 1.275 | 0.785 | 1.017 |
| `u32` ascending, n = 100,000 | keys | 0.605 | 1.858 | 1.000 |
| `u32` ascending, n = 100,000 | by key | 0.533 | 1.824 | 0.998 |
| `u32` descending, n = 100,000 | keys | 0.729 | 1.258 | 0.980 |
| `u32` ascending, n = 1,000,000 | keys | 0.651 | 1.725 | 1.059 |

The measured-build column comes from the full matrix (1 × 9), except the `u128` row, which is the 3 × 21 rerun above.

On these five cases, branch alignment brought the port and the cached prototype to 0.92–1.06, close to the identical prototype copy (0.92–1.01). It did not help at n ≤ 128, where the identical copy still ranged from 1.00 to 1.41. Branch alignment is also not a free speedup: across the small cases it changed the prototype's own time by −30% to +69% (`u32` random, n = 48: 20.9 to 14.5 ns per element; `u32` Zipf 1.3, n = 48: 14.5 to 24.5).

The crate therefore has nothing to fix. The final link of each program decides where a function lands, and Stable Rust has no attribute for function or branch alignment, so a source change that helps one binary can hurt another. Benchmarks control for placement instead ([source-attribution controls](../development/benchmarking.md#additional-source-attribution-controls)).

### Strictly descending inputs

The order probe now distinguishes strictly descending keys from descending keys with ties. Strictly descending records need one slice reversal; tied keys still use `reverse_stably` to restore their input order. This removes the equal-run scan for the strict case without changing the stable-sort contract.

The release comparison runner measured 22 reverse cases in one release binary: the candidate, its `tessera-aa` control, two separately linked copies of baseline `b973f09`, and a second candidate copy. Each case used nine shuffled blocks, input restoration outside the clock and stable-reference verification. The run was pinned to CPU 2 on a Ryzen 7 8845H; the report records host conditions and per-case A/A ranges. Ratios below are baseline-copy time divided by the candidate A/A mean, so values above 1 favor the change.

| Case | Candidate ns/record | Baseline A/B ratio |
|---|---:|---:|
| `u64/reverse/10000000` | 0.890 | 1.47 / 1.24 |
| `f64/reverse/10000000` | 1.021 | 1.55 / 1.56 |
| `row_u64/reverse/10000000` | 1.233 | 1.35 / 1.35 |
| `record128/reverse/1000000` | 9.334 | 1.52 / 1.52 |
| `owned/reverse/1000000` | 2.024 | 1.38 / 1.39 |

Both baseline copies are slower than the candidate outside its A/A range in 21 of 22 cases; `record128/reverse/1000` is within noise. A separate-process run had suggested a small loss there, but the paired run placed both baseline copies inside the candidate's A/A band (0.977–1.023), and the second candidate copy also tracked the primary. The small case therefore shows no established gain or regression. Baseline-copy ratios vary on some scalar cases, which exposes code-placement sensitivity; the improvement remains in both copies. Raw samples and the temporary comparison adapters stay outside the repository.

Strictness travels in the order classification, so the probe keeps the machine code it had before the change, and whole-slice reversal stays in a non-inlined helper. A paired A/B with both implementations in one binary confirmed the `u64` reverse speedup at every length and measured what an out-parameter had added to the constant-key case. Against an A/A range of ±3%, the earlier out-parameter was 4.6% then 2.4% slower on `u64/equal/65536` and 5.3% then 3.5% slower on `{u32,u32}/random/49`; strictness therefore belongs in the returned classification, not a second output. The continuous suite has no A/A control and alternated between no alert and alerts on different short cases in reruns, so it serves only as a regression screen.

### Low-fanout tiles

Plans longer than two LSD passes now select MSD when the top-digit sample contains at most two distinct buckets. Child variation scans are restricted to the remaining candidate mask so changing key functions cannot restore a processed digit.

The initial default-release measurements used the comparison runner pinned to CPU 2, with A/A and separate code-placement controls. The two baseline placements measured 20.308 and 20.157 ns/record on `u64/skew99/1000`, against 6.243 and 6.199 for the candidate; a second candidate placement measured 6.207. For `row_u64/skew99/1000`, the corresponding baseline values were 22.442 and 21.077, against 6.518 and 6.458; the placement control measured 6.468. Random controls showed no comparable shift.

After adding the candidate-mask fix, a separate paired harness checked 17 scalar and keyed-row cases in Default and Portable SIMD builds on a Ryzen 7 8845H pinned to CPU 2. Each build ran two processes of nine shuffled blocks, with two baseline placements, two original-PR placements, two revised placements and a repeated original-PR A/A call. Inputs were restored outside the clock and every result was checked against a stable reference. Both revised placements retained the short-skew improvement: baseline/original-PR ratios were 3.23–3.37 in Default and 3.09–3.23 in Portable SIMD, and revised/original-PR ratios were 0.94–1.03. The per-case A/A bounds on those skew cases ranged from 0.969 to 1.031. These figures describe the tested inputs and placements, not an application-wide speedup.

The continuous suite alerted on Default `u64/random/48` (+9.4%) and `u64/equal/65536` (+5.0%). Neither reaches the changed leaf decision. Paired measurements nevertheless exposed a real placement effect: identical original-PR copies differed by 21–22% on `random/48`, outside A/A bounds of 0.962–1.038 and 0.986–1.014. This is not discarded as timing noise.

The actual Default CI executables for baseline `e0ecd94` and candidate `54926ca` reproduced the short-input gap locally. Their order probe, root routing and budgeted insertion have identical normalized instructions; the tile's insertion path is also unchanged. A local binary intervention moved the candidate's `sort_tile` body 160 bytes to the baseline address, preserving instructions and relocating references, without changing the source or the input. Across three processes, baseline latency was 237–243 ns/sort, candidate latency 282–287, and the relocated candidate 243–247. Relocating only the budgeted insertion function did not remove the gap. This isolates tile code placement on the local host; it does not establish the Intel runner's microarchitectural mechanism.

The equal-key case did not show a consistent revised-candidate loss across the paired processes. An attempted early insertion finish for tiny inputs was rejected after the expanded matrix showed regressions on other small inputs. No source-level layout workaround or routing change is retained to tune one linked executable. Raw samples, executables and binary-intervention scripts remain outside the repository.

Two Rust tests check that scanned variation stays within the candidate and that an adversarial changing-key tile terminates while preserving records. Removing the mask makes the latter fail at a finite callback limit rather than overflowing the test process's stack. The model tests cover low fan-out, the strict cost comparison, saturated sampling, short plans and terminal leaves.

### Direct finishing for insertion-sized roots

An unordered root that already fits an insertion leaf can finish there instead of first exhausting a swap budget and scanning varying bits. The candidate keeps the order probe and the 48-record threshold, so confirmed ascending and descending inputs retain their existing return and reversal paths. The tile-fit guard preserves node routing for records whose total size exceeds a tile.

A Default-build process screen against `e26cd29`, using the maintained `compare commit` command, measured random `u64` keys and keyed rows at 48 records on CPU 2 with nine rotating baseline/control/current blocks. The initial candidate, before the explicit tile-fit guard, had current/baseline medians of 0.8894 for direct keys and 0.9426 for rows, with paired A/A ranges of 0.9894–1.0106 and 0.9864–1.0136. Both tested inputs fit a tile; this is preliminary evidence for the candidate, not exact-head or placement-controlled acceptance.

A second screen after adding the tile-fit guard measured sorted and reverse `u64` inputs at 48 records. Their medians, 0.9958 and 0.9990, were inside the respective A/A ranges, 0.9923–1.0077 and 0.9953–1.0047. The sorted case nevertheless had a block ratio of 1.1321; its cause remains unresolved. These local screens do not establish a general gain or explain the placement-sensitive alerts of other revisions. Raw results remain outside the repository under `short-insertion-random48` and `short-insertion-ordered48`; exact-head continuous results remain the performance acceptance gate.

### Ordered children and bucket order

Partition children scan their remaining candidate bits and then reuse the root's full order confirmation, and a node that already holds the two runs of its top varying bit is split there instead of being partitioned. Together these replace the specialized minority-bit/end-run detector, its 1,024-record cap and its rotation/insertion route. Tile-sized digit buckets still finish during compaction.

The two general properties are cheaper than the detector they replace. Both scans stop at their first conflict, so an unordered child pays a few key reads. The run split reads the top varying bit once: a node that is `0*1*` or `1*0*` in that bit already holds the two runs the partition would form, so one rotation at most puts them in order and no block scratch is reserved. It has no sample, no threshold and no length cap, and it applies to a block of keys at either end of the body rather than to one end.

A maintained `pnpm -s tools compare commit 8bd64d9` screen on this host (one pinned CPU, `u64`, 65,536 records, nine blocks, paired process-level A/A) measured these current/baseline ratios:

| Input | Ratio | Blocks | A/A | Screen |
|---|---:|---|---|---|
| `u64/sorted_tail/65536` | 0.3329 | 0.304–0.361 | 0.955–1.045 | faster |
| `u64/d16/65536` | 1.1214 | 1.070–1.205 | 0.903–1.097 | slower |
| `u64/random/65536` | 0.9655 | 0.920–1.060 | 0.961–1.040 | within noise |
| `u64/sorted/65536` | 0.9631 | 0.852–1.147 | 0.844–1.157 | within noise |
| `u64/sorted99/65536` | 0.9738 | 0.836–1.092 | 0.927–1.073 | within noise |
| `u64/far_swaps/65536` | 1.0197 | 0.936–1.090 | 0.920–1.080 | within noise |
| `u64/equal/65536` | 0.9589 | 0.818–1.028 | 0.942–1.058 | within noise |
| `u64/two_values/65536` | 0.9915 | 0.944–1.066 | 0.934–1.066 | within noise |

`sorted_tail/1000000` was checked with the research harness rather than the screen: 18.2–23.1 ms for the baseline executable against 3.6–5.0 ms for this one. `d16` is the one screen that reports a direction, and it reads every record the same number of times in both revisions: the whole node varies in four bits, so it takes the counted window instead of any partition this branch added. Its Portable SIMD configuration measured −1.1 % on the same head, so the direction tracks placement rather than work; it remains an open observation rather than a claimed result.

A peak-allocation harness measured the scratch of one `u64/sorted_tail/65536` sort at 8,672,352 bytes for a partitioned revision and 65,536 bytes for this one, against 89,356 bytes for `fafaaa2`. The 8.4 MB was one block-scratch allocation: the 12-bit digit that a concentrated top window selects reserves `2 x 4096` blocks of records. The run split never reaches that partition, with the run behind or ahead of the body.

An earlier revision of this branch also reordered `confirm_order` to check ascending order first, so constant children could reuse the blocked or vector scan instead of a scalar search. Callgrind counted 12,827,780 instructions for that revision against 12,037,195 for `fafaaa2` on the same three batches of `f64/sorted/65536`, a difference of about one instruction per key, and the case measured 8% slower in every local run and 12–13% slower in continuous Default. Constant children never reached the reordered backend on the continuous cases either: `u64/equal/65536` read exactly one key per record in both revisions. The reordering was reverted, which restores `fafaaa2`'s confirmation schedule; the child gate and the bucket-order split do not depend on it.

Continuous feedback for the same executable is also repeated rather than stable on the smallest cases: two runs of `a7192af` on Bencher measured `u64/equal/1000` Portable SIMD at 640 ns both times and `u64/sorted/1000` at 626 and 627 ns, while `row_u64/equal/48` moved from +7.3% to −1.2%. Those inputs read every record exactly once in both revisions, so the alerts track the executable's placement, not its work; this revision returns their code to the `fafaaa2` schedule.

### Exploratory SIMD constraints

The optional backend experiments used AVX2, seven interleaved rounds of separate binaries summarized by per-round minimum times, and a separate A/A run. They did not use the [protocol](../development/benchmarking.md#timing-and-statistics)'s nine-block paired timing or independently placed baseline copies, and did not record output-hash checks. These observations preserve design constraints, not established speedup or regression claims.

- Dispatch belongs around the insertion pass. Dispatching per scan cost more than it saved on random input, where inversions were usually in the first few pairs. A kernel outside its target-feature inlining context measured 3.5× slower on sorted `u64` and 7× slower on sorted `f64`.
- Materializing user keys on an 8,192-record near-sorted tile measured 1.4–1.6× slower than scalar callbacks: it added a key array and a second swap per move without reducing the default scan's one extraction per record. The tile finish therefore keeps callbacks; expensive extractors would need separate evidence.
- A sixteen-lane prefix sum was slower than eight lanes on AVX2 because of its cross-lane swizzle, so bucket-count prefix sums use eight lanes.
- Stable three-way compress scatter of 64K `u32` values into three buckets took 460 µs versus 147 µs for scalar cursor writes (3.1× slower). AVX-512CD conflict detection would also widen the cursor table from `u16` to `u32`, doubling its 32 KiB footprint; no scatter backend was retained.

### Scalar insertion finishing

Default direct-key finishing uses the existing scalar extractor; budgeted probes retain their blocked scan. Short ordered runs after a radix pass do not amortize a block mask.

A temporary harness outside the repository compares `26b340c` with the change in one release binary, on a Ryzen 7 8845H pinned to CPU 4, using Rust 1.99.1, the baseline x86-64 target, and no native CPU flags or LTO. It reproduces the continuous suite's 40 cases, including API and prepared-workspace modes, with SplitMix64 seed 1. Each case runs nine shuffled blocks, with the main baseline timed twice per block, an independently named copy of main, and two independently named copies of the fix. Batches target 60 ms with a 256 MiB prepared-buffer cap. Input restoration and exact output verification against a stable standard-library reference are outside the timed region; row verification includes original indices and float verification includes bits.

Ratios below are medians of `variant / mean(main A1, main A2)`. A/A is the per-block range of `A2 / A1`; placement is the range of the second main copy. Both copies of the fix agree on the direction in all nine blocks for these random `u64` cases:

| Length | Mode | Fix | Second fix copy | A/A range | Main placement range |
|---:|---|---:|---:|---|---|
| 8,193 | API | 0.9400 | 0.9158 | 0.9882–1.0115 | 0.9854–1.0006 |
| 8,193 | workspace | 0.9500 | 0.9416 | 0.9948–1.0076 | 0.9942–1.0033 |
| 65,536 | API | 0.9487 | 0.9160 | 0.9971–1.0084 | 0.9951–1.0036 |
| 65,536 | workspace | 0.9442 | 0.9181 | 0.9944–1.0020 | 0.9951–0.9991 |
| 1,000,000 | API | 0.9670 | 0.9447 | 0.9756–1.0282 | 0.9834–1.0176 |

The 99%-sorted cases retain their budgeted scan. Replacing that scan with a streaming scalar scan was rejected: in an earlier paired experiment, `u64/sorted99/65536` had a median ratio of 1.2879, outside its A/A range 0.9888–1.0105 and placement range 0.9938–1.0180.

The broad run also found a small cost on `u64/sorted_tail/65536`: fix ratios 1.0219 and 1.0108, with A/A 0.9923–1.0038 and placement 0.9763–1.0056. This is investigated separately rather than dismissed as noise. Long ordered stretches can still occur during a finish; scalar extraction trades their blocked scan for cheaper repeated short scans. No extra distribution or size gate is added to hide that trade-off. Two targeted reruns retained positive median changes, but their wider controls did not establish a regression independently. The small cost remains a point for Intel runner validation.

Ordered-input timing also moves with code placement. The root `u64` sort bodies of main and the fix have the same instruction sequence after address relocations are normalized, and sorted inputs return before the changed finishing operation. Their apparent speedup is not attributed to the scan change. Raw samples, manifests, executable snapshots and disassembly stay outside the repository. These local results do not establish the size of an improvement on Bencher's Intel runner. A paired Portable SIMD check of `u64/random/65536/s1/api` remained within its controls; the changed default finishing body is overridden for that primitive SIMD path.

At that revision, Default, no-std and Portable SIMD tests, doctests, Clippy, rustdoc and Miri passed. The local MSRV toolchain was unavailable; Rust 1.85 validation was left to CI.
