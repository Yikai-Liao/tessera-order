# Rust engine port

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
| Scratch owned by each call | Optional `Workspace<T>` keeps capacity between calls |

Free functions create their scratch only after the root fast paths, so ordered and constant inputs do not allocate. Block metadata is created only when a node needs a block partition.

## Correctness

The tests are written for this repository and use only the public API and engine internals. They cover every key encoding, float edge cases, thresholds around the insertion limit and tile size, 128-bit key halves, over-aligned and zero-sized records, owned records (`String`, drop tracking) and `&mut` records under panicking and changing key functions, workspace reuse after a panic, and allocation counts of a warm workspace.

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

The crate therefore has nothing to fix. The final link of each program decides where a function lands, and Stable Rust has no attribute for function or branch alignment, so a source change that helps one binary can hurt another. Benchmarks control for placement instead ([build profiles](../development/benchmarking.md#build-profiles)).

### Strictly descending inputs

The order probe now distinguishes strictly descending keys from descending keys with ties. Strictly descending records need one slice reversal; tied keys still use `reverse_stably` to restore their input order. This removes the equal-run scan for the strict case without changing the stable-sort contract. The implementation and paired measurements are recorded in [decision 0041](../decisions/0041-strict-descending-fast-path.md).

The release comparison runner measured 22 reverse cases in one release binary: the candidate, its `tessera-aa` control, two separately linked copies of baseline `b973f09`, and a second candidate copy. Each case used nine shuffled blocks, input restoration outside the clock and stable-reference verification. The run was pinned to CPU 2 on a Ryzen 7 8845H; the report records host conditions and per-case A/A ranges. Ratios below are baseline-copy time divided by the candidate A/A mean, so values above 1 favor the change.

| Case | Candidate ns/record | Baseline A/B ratio |
|---|---:|---:|
| `u64/reverse/10000000` | 0.890 | 1.47 / 1.24 |
| `f64/reverse/10000000` | 1.021 | 1.55 / 1.56 |
| `row_u64/reverse/10000000` | 1.233 | 1.35 / 1.35 |
| `record128/reverse/1000000` | 9.334 | 1.52 / 1.52 |
| `owned/reverse/1000000` | 2.024 | 1.38 / 1.39 |

Both baseline copies are slower than the candidate outside its A/A range in 21 of 22 cases; `record128/reverse/1000` is within noise. A separate-process run had suggested a small loss there, but the paired run placed both baseline copies inside the candidate's A/A band (0.977–1.023), and the second candidate copy also tracked the primary. The small case therefore shows no established gain or regression. Baseline-copy ratios vary on some scalar cases, which exposes code-placement sensitivity; the improvement remains in both copies. Raw samples and the temporary comparison adapters stay outside the repository.

The final implementation keeps strictness as a separate order-probe result and isolates whole-slice reversal in a non-inlined helper. A paired A/B rerun of that source confirmed the `u64` reverse speedup at 65,536, 1,000,000 and 10,000,000 records; see [decision 0041](../decisions/0041-strict-descending-fast-path.md). The continuous suite has no A/A control and alternated between no alert and alerts on different short cases in reruns, so it serves only as a regression screen.

## Not yet done

- A crates.io release. See [RELEASING.md](../../RELEASING.md).
