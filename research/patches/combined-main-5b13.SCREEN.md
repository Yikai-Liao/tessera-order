# Combined current-main port: four-case directional pilot

## Result

The port is validated and the optimized pilot completed, but this pilot does not establish a speedup over the frozen strongest combined candidate or establish a SIMD benefit. The first three cases have 17.6–22.4% two-sided repeat/placement control envelopes. The u64/W8 case contains a retained 1,129.698 ms oldbest C2 outlier (C1 was 35.838 ms in the same block), making its control envelope 31.522×. No outliers were removed or attributed to an unverified cause.

All four cases have identical seed-1 10M random keys (FNV1a64 c2333e730de5dd2c), one verified randomized warmup block and three measured randomized blocks. There are 36 verified warmup and 108 verified measured outputs. The CPU affinity mask is 0–7; individual Rayon workers are not pinned to specific CPUs.

## Paired timing ratios

Ratios are calculated within each block before taking the median. O is new-port SIMD off, V is new-port SIMD on, and C is frozen oldbest. Each is the mean of its two repeated labels in that block. A ratio below 1 favors its numerator. These are exploratory observations, not accepted performance claims.

| Case | Median O/C | Median V/C | Median V/O | Control envelope |
| --- | ---: | ---: | ---: | ---: |
| row16 W4 | 1.0289 | 0.9105 | 0.9673 | 1.1755× |
| row16 W8 | 1.0612 | 1.0272 | 1.0410 | 1.2158× |
| u64 W4 | 0.8910 | 0.8900 | 0.9203 | 1.2244× |
| u64 W8 | 0.9832 | 0.8131 | 0.9381 | 31.5219× |

The same-binary current-main serial controls were substantially longer in these random cases: paired median main/O ratios range from 2.334 to 3.220 and simd/V from 2.467 to 3.457. This does not establish the port as better than oldbest, and the u64/W8 anomaly limits that case. Three blocks do not satisfy the nine-block/8-of-9 acceptance rule.

## Absolute times and all control ranges

Absolute entries below are per-label median elapsed milliseconds across the three measured blocks. They are not ratios of medians. All raw samples, including outliers, remain in the aggregate.

| Case | C1 ms | C2 ms | Placement ms | O1 ms | O2 ms | V1 ms | V2 ms | Main ms | SIMD main ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| row16 W4 | 61.647 | 71.364 | 65.130 | 63.220 | 64.502 | 60.775 | 60.554 | 159.655 | 139.117 |
| row16 W8 | 48.937 | 47.923 | 50.939 | 48.690 | 48.366 | 50.927 | 49.790 | 139.807 | 138.192 |
| u64 W4 | 41.279 | 44.277 | 48.968 | 40.173 | 42.824 | 38.821 | 37.557 | 96.856 | 94.466 |
| u64 W8 | 34.076 | 26.211 | 35.792 | 26.512 | 32.521 | 31.026 | 24.649 | 98.843 | 88.538 |

| Case | C2/C1 range | Placement/C range | O2/O1 range | V2/V1 range |
| --- | ---: | ---: | ---: | ---: |
| row16 W4 | 1.003221–1.175500 | 0.971266–1.047546 | 0.913714–1.059913 | 0.888669–1.005706 |
| row16 W8 | 0.848772–0.995260 | 0.868465–1.215791 | 0.890168–1.030445 | 0.962538–1.036922 |
| u64 W4 | 1.043839–1.072636 | 0.816733–1.188780 | 0.966138–1.066002 | 0.837942–1.050455 |
| u64 W8 | 0.749729–31.521925 | 0.076053–1.187410 | 0.854039–1.458200 | 0.746915–1.103864 |

Exact outlier: u64/W8, measured block 0, slot 8, C2 = 1,129.698204 ms. The same implementation at block 0, slot 7, C1 = 35.838490 ms. Indices are zero-based. C2/C1 = 31.521925. The C2 outlier is included in its block mean and every reported statistic. This is not a diagnosed failure or an assumed OS-noise event; its cause is unverified.

## Validation scope

- Before the pilot, current-stable debug/native all-target tests passed with Rayon off-SIMD (47 unit + 22 integration) and Rayon with SIMD (52 unit + 22 integration), plus six doctests per control.
- Default/SIMD serial unit checks, no_std checks, Rust 1.85.1 default unit, Rayon all-target and no_std test/doc suites passed. Rust 1.85 emits the inherited upstream Unsigned visibility warning.
- Stable all-target/all-feature Clippy with warnings denied, formatting, and rustdoc with warnings denied passed after one approved inherited closure-borrow cleanup. Affected native unit suites and compact/deferred Miri were rerun after it.
- Focused nightly Miri passed insertion partial failure/panic, SIMD prefix sums and insertion, ascending confirmation, compact cycles/fault recovery, deferred ownership and delayed callback recovery. Real Rayon scheduling was covered by native tests, not Miri.
- The release pilot binary passed eight ordinary-path smoke cases: random and d16, both u64 and Row16, W4 and W8, n=131073. All 144 checks validated fresh inputs, stable references and rejection of a deliberately corrupted result. Smoke timers are not performance evidence.
- Full public integration tests were debug-profile tests; no generic full release integration suite is claimed. Release smoke plus all 144 full-sized warmup/measured outputs are the optimized ordinary-path gate.

## Source and dependency identity

- Serial upstream: 5b13ac1f598db8875a36f0d95c37fcb9223f9f56.
- Validated port identity: 9586fc287e595da719ec8c958f3e428254459599b3ba2306ffe1e234058d76b8.
- C1/C2, O1/O2, V1/V2 each share one exact match arm and by-key closure. Private package names isolate off/on features in the same binary. Oldbest/placement, off/on, and main/simd Rust sources match within each pair.
- Every Rust file, build manifest/lockfile and driver/protocol file is hashed. The current freeze contains 153 files; source-inputs.json SHA-256 24ea9236cf25d98b87a42fa02fc388664b5cb42860cf65ee81197321ddc55890.
- The first --locked build rejected the source-draft lockfile before compilation. Offline normalization only removed inactive fearless_simd edges from main/off. All registry versions and checksums exactly match the completed halo and profile harness locks. The original freeze and rejected draft remain in evidence/. The successful build retained --locked.
- Binary SHA256: 91115f9af1fc7637c3609393063c1e5a348649bd421ebc8fc8533bbba48abc29.
- Compiler: rustc 1.99.1, default release, no inherited flags or LTO override. Host CPU: AMD EPYC 9V74 80-Core Processor.

## CPU window and artifacts

- Successful shared build: 08:59:25–09:00:06 UTC. Shared smoke: 09:00:06–09:00:08 UTC. Exclusive pilot: 09:00:08–09:00:32 UTC, 2026-11-07.
- Acquisition/release are logged in evidence/build.log and evidence/window.log. The fixed shared CPU lock inode was never replaced.
- Raw results: evidence/{u64,row16}-random-n10000000-w{4,8}.jsonl.
- Verified aggregate: evidence/aggregate.json; all paired values and control ranges are retained.
- Manifest: evidence/manifest.json. Source and binary hashes: evidence/source-inputs.json, evidence/compiled-sources.json and evidence/binary.sha256.

## Limits

No extended thread/distribution matrix, standalone sorting-algorithm baselines, new heap measurement or further timing run was performed. No halo, prefetch or virtual-bucket route was added. The optional SIMD backend does not vectorize arbitrary ByKey record scans. No publish/merge action was performed by this worker.
