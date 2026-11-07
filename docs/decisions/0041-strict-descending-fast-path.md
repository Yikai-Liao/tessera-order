<!-- SPDX-License-Identifier: BSD-2-Clause -->

# 0041: Skip stable reversal scans for strict descending keys

Date: 2026-11-08. Status: accepted.

## Context

A confirmed descending input is reversed as a whole and then scanned for equal-key runs, which are reversed back to preserve stability. When every adjacent key is strictly descending, no equal-key runs exist; the second scan adds callback and memory traffic without changing the result. `confirm_order` already visits every adjacent pair, so it can distinguish strict descent while proving the input order.

The fast path must retain the stable behavior for descending inputs with ties and preserve panic safety for records and key extractors. A size threshold or second detection scan would add policy and work to a condition already known during confirmation.

## Decision

Have `confirm_order` return `StrictDescending` only when every adjacent key is strictly descending. Route that result to a whole-slice reversal and return. Keep `reverse_stably` for all other confirmed descending inputs, including those with equal keys.

Do not add a size threshold: strict descent is established by the existing confirmation scan, and the direct reversal is the simplest correct path. On strictly descending data, the check uses one greater-than comparison per pair; equality checks are reached only when a pair is not strictly descending.

## Validation

The release comparison used 22 reverse cases: four lengths each for `u32`, `u64`, `f64` and `row_u64`, and three supported lengths each for `record128` and `owned`. One binary included the candidate, its `tessera-aa` control, two separately linked baseline copies from `b973f09`, and a second candidate copy. The runner used nine shuffled blocks per case, restored inputs outside the timed region, and checked each result against the stable reference. It ran on a Ryzen 7 8845H pinned to CPU 2; host conditions and A/A ranges are recorded in the external report.

Ratios below are median baseline-copy time divided by the candidate A/A mean, so values above one favor the change. Both baseline copies were slower outside the candidate's A/A range in 21 of 22 cases; `record128/reverse/1000` was within noise. The second candidate copy tracked the primary, with one small favorable outlier.

| Case | Candidate ns/record | Baseline A/B ratio |
|---|---:|---:|
| `u64/reverse/10000000` | 0.890 | 1.47 / 1.24 |
| `f64/reverse/10000000` | 1.021 | 1.55 / 1.56 |
| `row_u64/reverse/10000000` | 1.233 | 1.35 / 1.35 |
| `record128/reverse/1000000` | 9.334 | 1.52 / 1.52 |
| `owned/reverse/1000000` | 2.024 | 1.38 / 1.39 |

A separate-process measurement had suggested a small regression on `record128/reverse/1000`. The paired result put both baseline copies inside the candidate A/A band (0.977–1.023), so it establishes neither a gain nor a regression for that case. Baseline-copy ratios varied on some scalar cases, exposing code-placement sensitivity; both copies nevertheless showed the improvement outside the controls on the other 21 cases. Raw samples and the temporary comparison adapters stay outside the repository.

`cargo test --workspace --all-targets`, `cargo test --workspace --doc`, `cargo test -p tessera-sort --all-targets --features portable-simd`, and Clippy with warnings denied pass. The docs formatter, simulator tests, typecheck and site build also pass.

## Consequences

The public ordering and stability contract does not change. Strictly descending inputs avoid the post-reversal key scan; descending inputs with equal keys retain the existing stable reversal. Key extractors are called fewer times on strict descent, but the API does not promise an exact callback count. No routing, digit planning, tile sizing or auxiliary-memory bound changes.
