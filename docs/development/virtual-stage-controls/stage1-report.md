# Fixed-parameter virtual stage screen

Diagnostic screening only, not acceptance. All 48 timing outputs and 16 separate allocator outputs matched complete stable references. The native-root + Tessera-leaf combination was the fastest virtual variant, but remained **8.30× slower than combined on Row16 and 13.87× slower on u64** by ratio of median totals. The three targeted changes reduce their intended stages; none closes the end-to-end gap.

## Total time medians

| Variant | Row16 ms | u64 ms | Paired original/candidate Row16 | Paired original/candidate u64 |
|---|---:|---:|---:|---:|
| Serial A1 | 155.818 | 95.370 | — | — |
| Serial A2 | 151.234 | 98.481 | — | — |
| Combined + std-yield W8 | 46.358 | 23.881 | — | — |
| Original virtual W8 | 457.434 | 408.788 | 1.0000× | 1.0000× |
| Tessera leaf only W8 | 403.820 | 363.547 | 1.1501× | 1.1510× |
| Native root only W8 | 446.708 | 391.451 | 1.0240× | 1.0362× |
| Native root + Tessera leaf W8 | 384.566 | 331.168 | 1.1715× | 1.2248× |
| Global chunk pack only W8 | 417.278 | 354.627 | 1.0797× | 1.1441× |

Paired ratios are computed within each seed/block, then the three ratios are median-aggregated. They are not ratios of independently aggregated medians. Serial A1/A2 paired ranges: Row16 **0.9836–1.0745**, u64 **0.9392–1.0129**. The modest native-only gain is not robust relative to these control ranges. Combined Row16 itself ranged 34.540–46.559 ms, reinforcing the screening-only interpretation.

## Root / child / global pack / restore medians

Times below are milliseconds. All virtual worker masks were **0xff** with no overflow. Each phase is sequentially separated by a barrier within a call, but the displayed marginal medians are not added to infer elapsed time. Raw per-call phase sums and residuals are retained in `summary.json`.

### row16

| Variant | Root | Child | Pack | Restore |
|---|---:|---:|---:|---:|
| Original virtual W8 | 134.963 | 125.579 | 87.642 | 101.414 |
| Tessera leaf only W8 | 131.242 | 73.821 | 88.830 | 104.453 |
| Native root only W8 | 76.931 | 135.235 | 98.169 | 132.234 |
| Native root + Tessera leaf W8 | 73.817 | 79.936 | 97.776 | 129.980 |
| Global chunk pack only W8 | 153.207 | 127.761 | 36.778 | 107.778 |

Native+leaf root scan / scatter / adopt-and-minmax medians: **8.525 / 8.051 / 57.098 ms**. Native-only Stats does not expose subphases; its root total includes all three.

### u64

| Variant | Root | Child | Pack | Restore |
|---|---:|---:|---:|---:|
| Original virtual W8 | 118.984 | 125.329 | 79.413 | 92.658 |
| Tessera leaf only W8 | 124.046 | 64.236 | 84.574 | 92.450 |
| Native root only W8 | 61.685 | 119.539 | 90.880 | 117.072 |
| Native root + Tessera leaf W8 | 66.114 | 64.497 | 89.556 | 111.819 |
| Global chunk pack only W8 | 124.871 | 119.828 | 25.461 | 86.669 |

Native+leaf root scan / scatter / adopt-and-minmax medians: **5.922 / 6.401 / 53.361 ms**. Native-only Stats does not expose subphases; its root total includes all three.

Native-only reduces root time but its restore stage is slower in this screen. Tessera leaf reduces child time, and chunk packing reduces global pack time. Median paired targeted-phase ratios (original/candidate) are leaf child **1.713×/1.879×**, native-only root **1.742×/1.939×**, and chunk pack **2.383×/3.211×**, Row16/u64 respectively. Do not combine these isolated savings arithmetically to predict an unmeasured combined variant.

## Separate measured live heap

Allocator-instrumented executable; these elapsed times are not performance results. Input and full stable reference allocations and reference sorting precede tracking; both full record buffers remain alive throughout the sort. Table reports additional requested live Rust heap bytes, including allocation metadata buffers and Tessera leaf workspace, not RSS, allocator usable-size overhead, or thread stack reservations.

| Variant | Row16 bytes | u64 bytes |
|---|---:|---:|
| Serial A1 | 1,849,780 | 1,318,748 |
| Serial A2 | 1,849,780 | 1,318,748 |
| Combined + std-yield W8 | 7,971,658 | 6,168,395 |
| Original virtual W8 | 23,011,933 | 14,025,309 |
| Tessera leaf only W8 | 23,013,853 | 14,027,229 |
| Native root only W8 | 27,227,834 | 16,144,058 |
| Native root + Tessera leaf W8 | 27,255,738 | 16,158,906 |
| Global chunk pack only W8 | 23,011,933 | 14,025,309 |

Every invocation returned to **zero extra live bytes** at the end of the measured call. The leaf-workspace capacity reported inside the invocation was 25,728/12,928 bytes for leaf-only Row16/u64 and 25,984/12,928 bytes for native+leaf; those are retained workspace capacities during the call, not leaks after return and not standalone process peaks. Their allocations are included in the actual measured peaks above. Native record scratch is included in auxiliary record bytes and actual heap; its separate capacities are 8,388,608/4,194,304 bytes. Do not add separately observed capacities or metadata maxima to the measured peak.

## Writes and measurement boundaries

Root, outer child-kernel, packing, and restore written-byte counters are preserved per invocation and summarized in `summary.json`. The Tessera-leaf variants count 640,000,000/320,000,000 child bytes on Row16/u64, versus approximately 3,194,000,000/1,597,000,000 for comparison-merge leaves. **Internal Tessera writes are unmeasured**, explicitly flagged in every leaf sample, so the smaller counter is not evidence for fewer total moves. Reads are not counted. Restore write traffic can vary with physical slot reuse and completion order.

Per-block profiling is disabled for every virtual candidate. Active-child high-water is emitted as null, not zero. Profile-off zero high-water fields in the underlying Stats are not memory measurements. Timing-mode `peak_extra_bytes=0` and `retained_extra_bytes=0` are unused placeholders; only records with `kind="heap"` are allocator observations.

## Fixed method and provenance

- Each type has 10,000,000 random records. Seeds 1/2/3 are paired across all eight labels, using the prior pilot’s SplitMix64 contents. Randomization seed 9123, three blocks. Warmup uses 32,768 records; the separate smoke used 65,539 records and all 16 label/type outputs passed.
- Every virtual variant uses P_root=P_child=8, B=128, K=4096, radix_bits=8, max MSD levels=2, Scheduler::Rayon, ambient W8. Native B64 defaults are never used. Serial A1/A2 use ambient W1; combined uses W8.
- Chunk-only explicitly selects GlobalPack::Chunks; all other virtual variants use their unchanged scalar packing. No hoist, fused-minmax, or pull-restore source-only fork is included.
- Every crate seals its own repr(C) Row16 with two u64s. Each is constructed separately and safely from identical key/ordinal values. Runtime layout assertions: size16, alignment8, key offset0, ordinal offset8. u64 size/alignment8. There is no cross-crate pointer or slice cast.
- Fresh input construction, first touch, stable reference sorting, and each measured invocation occur in the selected Rayon pool worker. Full reference verification is outside the timer, with all records retained. No reference storage is borrowed as algorithm scratch.
- Timing and heap runs use CPU affinity0–7. This controls the process CPU set, not placement/NUMA/cache state. Every TESSERA_* environment override is explicitly cleared and the binary asserts the set is empty. Enumerated source knobs and observed affinity are in `timing-environment.txt`.
- Universal lock device27/inode1578019 was preserved. Shared FD9 covers builds/Clippy/smoke, exclusive FD9 covers timing and separate heap run. Actual process/session windows are in logs. Timing exclusive07:22:16–07:22:54Z/session80620; heap exclusive07:24:52–07:25:05Z/session66399.
- Release/offline/-j1 builds and driver Clippy passed for timing and count-alloc configurations. Toolchain is rustc1.99.1; exact version is in `toolchain.txt`. All included algorithm copies had owner-confirmed release/reference and applicable Miri validation before timing.
- All87 copied Rust source files exactly match their frozen originals, verified before and after timing. Only private vendor manifest dependency paths/package identities were changed to avoid duplicate native package identities. `source-copy-hashes.json`, `source-recheck.txt`, and `manifest-changes.diff` contain the provenance. Original crates, prior pilot, shared branch, and repository were not changed; no commits or pushes.
- Timing/heap binary hashes: `timing-binary.sha256` and `heap-binary.sha256`; driver/source/lock hashes: `timing-inputs.sha256` and `heap-inputs.sha256`. Reproduce locally using `build.sh`, `run.sh`, `build-heap.sh`, `run-heap.sh`, then `summarize.py` after coordinating CPU windows.

Raw files: `stage-row16.jsonl`, `stage-u64.jsonl`, `heap.jsonl`; machine-readable paired summary: `summary.json`. The earlier negative pilot remains untouched in `../tessera-virtual-scale-screen/evidence/PILOT_REPORT.md`. No broad distribution matrix, placement-controlled acceptance test, or new asymptotic claim was attempted.
