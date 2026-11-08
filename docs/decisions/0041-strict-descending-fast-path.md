<!-- SPDX-License-Identifier: BSD-2-Clause -->

# 0041: Skip stable reversal scans for strict descending keys

Date: 2026-11-08. Status: accepted.

## Context

A confirmed descending input is reversed as a whole and then scanned for equal-key runs, which are reversed back to preserve stability. When every adjacent key is strictly descending, no equal-key runs exist; the second scan adds callback and memory traffic without changing the result. `confirm_order` already visits every adjacent pair, so it can distinguish strict descent while proving the input order.

The fast path must retain the stable behavior for descending inputs with ties and preserve panic safety for records and key extractors. A size threshold or second detection scan would add policy and work to a condition already known during confirmation.

## Decision

Carry strictness in the order classification: `confirm_order` returns `Order::Descending { strict }` and clears `strict` as soon as an adjacent pair ties, and `probe_order` keeps the signature it had before this change. Route strict descent to `reverse_strictly` and return; keep `reverse_stably` for all other confirmed descending inputs, including those confirmed after a flat sample. Keep the whole-slice reversal loop in a `#[inline(never)]` helper so it stays out of the general sort path.

The first implementation reported strictness through a `&mut bool` out-parameter instead. Every input then paid for a stack slot and a store on the way into the probe, and the compiler allocated the sampling loop's registers differently than it does in the previous engine. That moved `u64/equal/65536`, whose time is mostly that loop, by about 5% in a paired same-binary comparison, so strictness moved into the returned classification and the probe's machine code returned to what it was. A pointer-free result also describes the classification completely, instead of returning a value the flat-sample caller has to ignore.

Do not add a size threshold: strict descent is established by the existing confirmation scan, and the direct reversal is the simplest correct path. On strictly descending data, the check uses one greater-than comparison per pair; equality checks are reached only when a pair is not strictly descending.

## Validation

The release comparison used 22 reverse cases: four lengths each for `u32`, `u64`, `f64` and `row_u64`, and three supported lengths each for `record128` and `owned`. One binary included the candidate, its `tessera-aa` control, two separately linked baseline copies from `b973f09`, and a second candidate copy. The runner used nine shuffled blocks per case, restored inputs outside the timed region, and checked each result against the stable reference. It ran on a Ryzen 7 8845H pinned to CPU 2.

Ratios below are median baseline-copy time divided by the candidate A/A mean, so values above one favor the change. Both baseline copies were slower outside the candidate's A/A range in 21 of 22 cases; `record128/reverse/1000` was within noise.

| Case | Candidate ns/record | Baseline A/B ratio |
|---|---:|---:|
| `u64/reverse/10000000` | 0.890 | 1.47 / 1.24 |
| `f64/reverse/10000000` | 1.021 | 1.55 / 1.56 |
| `row_u64/reverse/10000000` | 1.233 | 1.35 / 1.35 |
| `record128/reverse/1000000` | 9.334 | 1.52 / 1.52 |
| `owned/reverse/1000000` | 2.024 | 1.38 / 1.39 |

A second binary held the classification design, two baseline copies, an A/A copy of the classification design and a copy of the earlier out-parameter design. On `u64` reverse, the baseline ratios at 1,000, 65,536, 1,000,000 and 10,000,000 records were 1.17/1.17, 1.18/1.18, 1.33/1.34 and 1.24/1.24 in one run and 1.16/1.16, 1.20/1.20, 1.34/1.34 and 1.27/1.18 in a rerun. The two implementations were within one percent of each other from 65,536 records up.

That binary also exposed what the out-parameter cost. Against an A/A range of ±3%, the out-parameter copy was 4.6% then 2.4% slower on `u64/equal/65536` and 5.3% then 3.5% slower on `{u32,u32}/random/49`; the baseline copies were 3.8% and 5.7% slower than the classification copy on `u64/equal/65536`. No other case left the A/A range. The continuous suite has no A/A control, and two runs of the same engine source (`bedd921`, then the docs-only `ee5b9e4`) reported different alerts, including +14.8% on `u64/sorted99/65536` in the Portable SIMD build, so those alerts on unchanged paths are not performance evidence; see the [continuous benchmark protocol](../development/continuous-benchmarks.md). Raw samples and the temporary comparison adapters stay outside the repository.

`cargo test --workspace --all-targets`, `cargo test --workspace --doc`, `cargo test -p tessera-sort --all-targets --features portable-simd`, and Clippy with warnings denied pass. The docs formatter, simulator tests, typecheck and site build also pass.

## Consequences

The public ordering and stability contract does not change. Strictly descending inputs avoid the post-reversal key scan; descending inputs with equal keys retain the existing stable reversal. Key extractors are called fewer times on strict descent, but the API does not promise an exact callback count. No routing, digit planning, tile sizing or auxiliary-memory bound changes, and the order probe keeps the code it had before the change.
