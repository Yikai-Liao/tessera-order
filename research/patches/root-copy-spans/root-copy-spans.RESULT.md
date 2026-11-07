# Copy-span final screen: inconclusive

This is a results-only follow-up to [the archived source checkpoint](root-copy-spans.README.md). The source patch, proof and original manifests remain byte-identical. The runtime is frozen on pure main `0bc80b49d5efd5bd8b62165b170015e5803226b7` plus the published combined research patch, not later main `e8e5906` or subsequent PRs. No public runtime is changed.

O means untouched original combined control, A means ordered one-prefix, and C means ordered coalesced. Rust 1.99.1 (`58cf8f9c35b355199727627d07ae34a4e2846d67`), LLVM 23.1.1, x86_64-unknown-linux-gnu.

## Decision

**Inconclusive. Do not adopt. Stop this screen without retuning or extending the matrix.** Lower paired medians did not clear both algorithms' own A/A and source-identical placement controls. No hardware-causal explanation follows from these elapsed times.

## Fixed experiment

- Frozen original: `305068d9d6f4b1d0621c18426eee4945c8b45514f4b89037a276ed60fef37d70`.
- Frozen candidate: `3903f8d1dfc75231eeddcb2cee4e2d72e72c495bb37f179e92bf6adc1495a41b`, safety gates and independent review confirmed before vendor preparation/build.
- `u64` Direct and shared `#[repr(C)] Row16 { key: u64, id: u64 }` ByKey; 10,000,000 random SplitMix64 records, seed 1. u64 size/alignment 8/8; Row16 16/8, key offset 0, original-ID offset 8. All Row16 calls use the same named function-pointer key type. No layout casts.
- Six separately named private package identities, with byte-identical copied `.rs` files: original O, ordered-one A, ordered-coalesced C, and an independent placement of each. Only private Cargo metadata changed. No feature union between algorithms.
- Nine labels O1/O2/O_placement, A1/A2/A_placement, C1/C2/C_placement. Each repeated pair has the same function address; each corresponding placement has a distinct address, asserted before measurement.
- Process affinity 0–7, exactly eight Rayon workers, no worker pinning. Runtime Fearless SIMD dispatch logged as Avx512 before each case. All algorithms have portable-simd enabled. No native target override or TESSERA environment controls.
- Stable standard-library reference generated once per case. Fresh clone inside executing worker before each timer. Full API scratch allocation/planning is timed; full output equality checks are outside every timer. No counting allocator or copy trace in timed binary.
- One warmup per label per case, nine deterministic randomized blocks. Every sample has two fresh sorts and at least 25 ms accumulated duration. All 162 samples and 324 individual timed durations are retained, including slow tails. All sorts passed full equality against reference. Each label also passed a separate full-size smoke before timing.
- Offline release build, one Cargo job, Rust/compiler/features and binary hashes retained. Build took 2m18s. Initial 13:27:36–37 metadata-only failure is retained, not classified as a sort failure. Explicit workspace exclusions repaired inheritance; algorithms remained untouched. Allocation binary emitted a harmless unused `id` method warning.

## Paired results

Ratios use each block's mean of C1/C2 divided by the mean of the denominator pair. Median is over nine block ratios, not a ratio of overall medians.

| Case | C/A median | Favorable blocks | C/O median | Favorable blocks |
|---|---:|---:|---:|---:|
| u64 | 0.939249 | 6/9 | 0.860936 | 8/9 |
| Row16 | 0.906082 | 8/9 | 0.836956 | 8/9 |

C/A isolates copy grouping with identical O(C) preflight/map-free entry. C/O checks practical end-to-end impact against the untouched original, including entry/preflight/map changes. Neither comparison establishes a benefit.

## Controls

A/A is second label / first label within the same block; placement is independent placement / mean of its corresponding pair. All ranges are raw min–max across nine blocks.

| Case | Algorithm | A/A range | Placement range | Median pair sort ms |
|---|---|---|---|---:|
| u64 | O | 0.796792–1.645819 | 0.677446–1.099990 | 31.705324 |
| u64 | A | 0.732983–3.422351 | 0.762340–1.402315 | 28.245984 |
| u64 | C | 0.723941–3.366350 | 0.527814–1.380802 | 29.141414 |
| Row16 | O | 0.869792–1.402056 | 0.871219–1.076683 | 53.404876 |
| Row16 | A | 0.899921–1.120450 | 0.850288–1.631111 | 50.285126 |
| Row16 | C | 0.928621–1.245185 | 0.863380–1.353039 | 45.801599 |

The evaluator conservatively includes reciprocal directions of both algorithms' control ratios. Combined symmetric control envelopes: u64 C/A 0.292197–3.422351, C/O 0.297058–3.366350; Row16 C/A 0.613079–1.631111, C/O 0.713238–1.402056. Every median falls inside its envelope. Row16 C/A also fails the raw A placement lower-bound test, so the conclusion does not depend on reciprocal symmetrization. Wide controls are retained as uncertainty, not censored or explained away.

## Separate allocation observations

One complete sort per algorithm/type; input/reference allocation excluded, pool prewarmed and broadcast before measurement. All six outputs verified and retained heap delta was zero. These are full-sort requested-heap measurements, not root-only estimates, RSS, or timing evidence. Stack/unmeasured allocator overhead is excluded. Allocation calls and totals can vary with parallel scheduling; one sample does not establish a deterministic allocation-count difference.

| Type | Algorithm | Peak extra bytes | Allocation calls | Requested bytes | Retained delta |
|---|---|---:|---:|---:|---:|
| u64 | O | 6,168,395 | 639 | 9,827,867 | 0 |
| u64 | A | 6,168,395 | 638 | 9,974,681 | 0 |
| u64 | C | 6,168,395 | 626 | 9,700,097 | 0 |
| Row16 | O | 7,971,658 | 643 | 13,064,190 | 0 |
| Row16 | A | 7,971,658 | 666 | 13,341,988 | 0 |
| Row16 | C | 7,971,658 | 667 | 13,411,590 | 0 |

## Evidence and publication checks

Frequency, thermal, throttling and per-worker telemetry were not measured. Timing window: 2026-11-07 13:32:37–13:33:14 UTC. No hardware-causal claim is made.

[Artifact identities](root-copy-spans.RESULT-ARTIFACTS.json) identify externally retained evidence; these are not public download links. The archive includes six private vendor copies and the research harness, so it is intentionally not committed. Raw samples, binary content and local environment logs stay external. Slow tails remain in the source evidence, without filtering.

Publication independently verified every source-evidence manifest hash, both source file maps against the published source packet, 162 sample counts and 324 full-reference flags, per-sort sums, paired medians, all A/A and placement extrema, and the complete-sort allocation records. No build, benchmark rerun or source modification occurred. No benefit is established; do not adopt or extend this screen.
