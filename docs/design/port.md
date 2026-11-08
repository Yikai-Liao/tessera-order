# Rust engine port

This page retains measured evidence behind engine choices, not current development instructions. The initial-port results and follow-ups below describe their recorded revisions and environments; they are not current-head validation. Use the [benchmark protocol](../development/benchmarking.md#command-capabilities) for new measurements.

The engine in `rust/tessera-sort` is a Stable Rust port of the research prototype in `sort-research-rs` at commit `6c0877f0491dc3b8fc052aa529bce44bf8a53fa8`. The crate is unreleased (`0.0.0`, `publish = false`).

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
