# Second virtual mechanism screen: old core only

All **48 timing outputs and 16 separate allocator outputs** matched complete stable references. The first screen is preserved unchanged. This second screen uses the frozen **old core**, not the new main SIMD baseline, and is diagnostic rather than acceptance evidence.

The fused root removes almost all of the separate adoption/minmax scan, and pull restoration cuts the targeted restoration stage. Hoisting has a smaller effect. The fastest virtual variant still takes **6.57× / 11.50×** the combined-control median total for Row16/u64. None of these results establishes a win over the strongest parallel control or the new SIMD core.

## Total time medians

| Variant | Row16 ms | u64 ms |
|---|---:|---:|
| Serial A1 | 157.055 | 105.885 |
| Serial A2 | 151.813 | 100.859 |
| Combined + std-yield W8 | 48.912 | 25.497 |
| Native root + Tessera leaf baseline | 402.555 | 349.355 |
| Span-hoisted adoption + Tessera leaf | 360.835 | 337.439 |
| Fused native-root ranges + Tessera leaf | 321.513 | 293.297 |
| Chunk pack + scalar restore | 415.705 | 367.197 |
| Chunk pack + pull restore | 339.991 | 293.532 |

Serial A1/A2 paired ratio ranges are **0.9994–1.2312** for Row16 and **0.9811–1.1275** for u64. These are wide controls. All samples, including the slow controls, remain in the result. Treat modest total-time differences cautiously, particularly hoist; there is no placement control, broad distribution matrix, formal significance test, or acceptance claim.

## Paired comparisons

| Change | Total ratio Row16 | Total ratio u64 | Targeted phase ratio Row16 | Targeted phase ratio u64 |
|---|---:|---:|---:|---:|
| Hoist vs native-leaf baseline (root) | 1.0950× | 1.0632× | 1.2147× | 1.1066× |
| Fused vs native-leaf baseline (root) | 1.2285× | 1.1427× | 3.8317× | 4.5945× |
| Pull vs scalar restoration (restore) | 1.2183× | 1.2510× | 2.7561× | 4.6983× |

Each ratio is control/candidate within the same seed/block, followed by the median of the three paired ratios. Individual ratios and absolute samples are in `summary.json`. Do not multiply or add independent optimizations to predict an unmeasured combined implementation.

## Phase medians

Times below are milliseconds. All virtual child worker masks were **0xff**, with no overflow. Root, child, pack and restore are barrier-separated within an invocation. Displayed marginal medians must not be summed into a fabricated total; per-invocation stage sums and residuals are retained.

### row16

| Variant | Root | Child | Pack | Restore |
|---|---:|---:|---:|---:|
| Native root + Tessera leaf baseline | 83.605 | 76.278 | 101.087 | 132.555 |
| Span-hoisted adoption + Tessera leaf | 68.829 | 71.310 | 91.642 | 126.850 |
| Fused native-root ranges + Tessera leaf | 21.401 | 78.406 | 90.569 | 128.550 |
| Chunk pack + scalar restore | 126.373 | 130.866 | 36.479 | 119.081 |
| Chunk pack + pull restore | 128.768 | 130.189 | 35.879 | 44.625 |

Root subphases, scan / scatter / adopt-and-minmax:

- Native root + Tessera leaf baseline: 8.858 / 12.608 / 61.260 ms.
- Span-hoisted adoption + Tessera leaf: 8.519 / 7.875 / 52.090 ms.
- Fused native-root ranges + Tessera leaf: 9.978 / 10.676 / 0.372 ms.

### u64

| Variant | Root | Child | Pack | Restore |
|---|---:|---:|---:|---:|
| Native root + Tessera leaf baseline | 72.315 | 66.634 | 88.005 | 118.047 |
| Span-hoisted adoption + Tessera leaf | 65.348 | 70.712 | 85.471 | 113.652 |
| Fused native-root ranges + Tessera leaf | 15.352 | 69.359 | 92.235 | 115.900 |
| Chunk pack + scalar restore | 119.386 | 122.607 | 23.064 | 92.428 |
| Chunk pack + pull restore | 122.513 | 129.582 | 23.071 | 17.960 |

Root subphases, scan / scatter / adopt-and-minmax:

- Native root + Tessera leaf baseline: 6.338 / 6.508 / 58.217 ms.
- Span-hoisted adoption + Tessera leaf: 5.029 / 8.124 / 49.909 ms.
- Fused native-root ranges + Tessera leaf: 6.325 / 8.469 / 0.387 ms.

The fused root moves range accumulation into native scattering. Its adoption phase falls to 0.372/0.387 ms, versus 61.260/58.217 ms for the old native-leaf baseline. In contrast, span hoisting still performs the per-record min/max scan. Pull restoration falls to 44.625/17.960 ms, versus 119.081/92.428 ms for scalar restoration. These large targeted-stage effects are useful mechanism evidence despite noisy total-time controls.

## Separate additional-live-heap measurements

One separate allocator-instrumented sample per label/type, seed1. Input and the full stable reference are allocated and reference-sorted before tracking, remain live throughout the sort, and are not borrowed as algorithm scratch. Measurements include all Rust allocator requested bytes during the call, including workspace and metadata, but not RSS, allocator usable-size overhead, or thread stacks. Instrumented elapsed times are not used for performance comparisons.

| Variant | Row16 extra bytes | u64 extra bytes |
|---|---:|---:|
| Serial A1 | 1,849,780 | 1,318,748 |
| Serial A2 | 1,849,780 | 1,318,748 |
| Combined + std-yield W8 | 7,971,658 | 6,168,395 |
| Native root + Tessera leaf baseline | 27,255,594 | 16,158,818 |
| Span-hoisted adoption + Tessera leaf | 27,255,658 | 16,158,882 |
| Fused native-root ranges + Tessera leaf | 27,255,642 | 16,158,930 |
| Chunk pack + scalar restore | 23,011,933 | 14,025,309 |
| Chunk pack + pull restore | 23,016,029 | 14,027,357 |

All 16 allocator runs returned to **zero extra live bytes** at call end. Native/hoist/fused contain 22,167,552/11,083,776 bytes of auxiliary record capacity, including 8,388,608/4,194,304 bytes of native segmented scratch. Leaf workspace capacities, approximately 25,800/12,900 bytes, are reported separately but included in actual heap peaks. The fused 36,864-byte range arrays are already included in native metadata. Do not count these arrays twice or sum separately observed capacities into a synthetic peak.

Pull adds 4,096/2,048 bytes to this run’s overall peak compared with same-crate scalar restoration. Its stage-boundary observed metadata is larger, but not all allocations are live at the same phase peak. The actual measured peak, not a sum of independent maxima, is the memory result. Small native/hoist/fused peak differences also reflect per-worker workspace capacities and are not evidence of a categorical memory reduction.

## Write counters and limits

Per-call root, outer-child, pack, and restoration written-byte counters are preserved. Pull’s median restoration writes are160,002,048/80,002,048 bytes, versus298,938,368/149,423,104 bytes for same-crate scalar restoration (Row16/u64). Physical block reuse and cycles can change these values slightly between invocations. Reads are not counted.

All native-root variants use Tessera leaves. Their child-write counters exclude **unknown internal Tessera writes** and explicitly report `leaf_internal_writes_unmeasured=true`. Smaller child counters must not be described as fewer total moves. Per-block profiling is disabled; high-water fields are null/unavailable, never interpreted as zero memory. Timing-mode heap-zero placeholders are unmeasured; only `kind="heap"` rows contain allocator observations.

## Fixed experiment and provenance

- Eight labels, each 10,000,000 random records for Row16 and u64, three randomized blocks. Paired input seeds1/2/3 and order seed9123 reproduce the first screen’s generator and contents. Full stable references follow every measured output. Warmup 32,768 records; reference smoke 65,539 records passed all 16 outputs.
- Fixed virtual P_root=P_child=8, B=128, K=4096, radix_bits=8, max MSD levels2, Rayon W8. Serial controls use W1; combined uses W8. All native candidates explicitly enable leaf-tessera. Native B64 defaults are never used.
- The restoration comparison uses the **same frozen pull-control crate** with GlobalPack::Chunks and RestoreMode::Scalar versus RestoreMode::Pull. It is not a comparison against a moving checkout or a different chunk implementation. Native-root variants retain scalar global packing/restoration.
- Every crate’s sealed repr(C) Row16 is safely constructed from the same key/ordinal values. Runtime layout assertions check size16, alignment8, offsets0/8. u64 size/alignment8. No cross-crate pointer/slice casts occur.
- Each fresh input is constructed and first-touched in the actual pool worker before either measurement window. Stable reference sorting is also worker-local and outside timing/tracking. Verification is outside timing and compares every record, including ordinal.
- CPU affinity0–7 for both timed and heap binaries. This is not controlled NUMA/cache placement. All TESSERA_* overrides are cleared and the binary asserts an empty set; source knob names and actual affinity are recorded in `timing-environment.txt`.
- Universal lock device27/inode1578019 was preserved. Shared FD9 covers builds/Clippy/smoke; exclusive FD9 covers timing and heap separately. Timing exclusive07:41:20–07:41:56Z/session36214. Heap waited at07:43:17Z, acquired07:43:34Z, released07:43:48Z/session25119.
- Release/offline/-j1 builds and driver Clippy passed in both configurations. Owner-confirmed feature/base reference suites and applicable default-SB Miri validation passed for every candidate before timing. Exact rustc1.99.1 version is in `toolchain.txt`.
- All 102 copied Rust source files match their frozen originals before/after timing. Only private vendor manifests changed dependency identities/paths to avoid native package collisions. `source-copy-hashes.json`, `source-recheck.txt`, `manifest-changes.diff`, `timing-inputs.sha256`, `heap-inputs.sha256`, and binary hash files preserve provenance.
- First-screen artifact hashes were rechecked successfully; it was not edited. Original frozen algorithms, shared repository, and shared branch were not edited. No commits or pushes.
- Raw evidence: `stage-row16.jsonl`, `stage-u64.jsonl`, `heap.jsonl`. Paired summary: `summary.json`. Reproduction scripts are sibling `build.sh`, `run.sh`, `build-heap.sh`, `run-heap.sh`, `summarize.py`, and `write_report.py`; coordinate shared/exclusive CPU windows before rerunning.
