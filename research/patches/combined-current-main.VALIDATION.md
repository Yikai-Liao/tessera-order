# Validation of the frozen current-main combined control

Source identity: `305068d9d6f4b1d0621c18426eee4945c8b45514f4b89037a276ed60fef37d70`.

## Completed host gates

Host: x86_64 Linux. Current stable: Rust1.99.1, commit `58cf8f9c35b355199727627d07ae34a4e2846d67`, LLVM23.1.1. MSRV: Rust1.85.1, commit `4eb161250e340c8f48f66e2b929ef4a5bed7c181`, LLVM19.1.7. Commands used offline dependencies, `-j1`, release tests and one test-runner thread. Validation held the shared CPU window; no timing experiment ran.

| Toolchain/configuration | Unit tests | Integration tests | Doctests |
| --- | ---: | ---: | ---: |
| Stable default | 17 | 10 | 4 |
| Stable Rayon | 56 | 23 | 6 |
| Stable Rayon + Portable SIMD | 61 | 23 | 6 |
| Stable no-default-features/no_std | 13 | 10 | 4 |
| Rust1.85 default | 17 | 10 | 4 |
| Rust1.85 Rayon | 56 | 23 | 6 |
| Rust1.85 no-default-features/no_std | 13 | 10 | 4 |

Nested progress tests run bounded child test processes. Those child invocations are not counted again in the table. Rayon builds normally ignore one explicitly untimed 10M geometry diagnostic; it was then run separately under the stable SIMD-on configuration and passed both complete output checks. All-feature/all-target Clippy with `-D warnings`, selected-crate formatting, and all-feature rustdoc with warnings denied passed.

MSRV emits the unchanged upstream unreachable-public-item warning for `Unsigned`; this was not suppressed. Rust1.85 all-features is not claimed because Portable SIMD requires Rust>=1.89. Standalone Portable SIMD without Rayon was not separately run in this checkpoint.

## Preserved behavioral coverage

The suites retain native scatter and whole-block permutation tests, deferred publication boundaries, owned String/Cell records, exclusive mutable references, changing keys, panic payload and exactly-once drop checks, simultaneous producer/consumer failure, and bounded nested Rayon progress/panic tests at P2/P3/P4. Main's new block-scatter and callback panic recovery tests are unchanged.

New adapter tests cover Direct and stable ByKey behavior, all signed/unsigned widths and Reverse, both u128 halves, and f32/f64 encoded windows including both signs, NaN payloads, infinities, zeros/subnormals and exponent windows. Hook counters establish actual root/child counting reachability rather than inferring routes from key-call totals.

## Focused Miri

Thirteen strict-provenance Miri commands, covering fifteen test invocations, passed:

- Upstream scatter and callback panic recovery.
- Narrow Direct/KeyHalf counting and the owned parallel Narrow forwarding.
- Native owned panic recovery, exclusive references and changing keys.
- Whole-block permutation interruption/recovery with owned and exclusive records.
- Deferred owned/exclusive publication and delayed callback failure after earlier consumption.
- Delayed pipeline ownership, partial insertion with Cell/exclusive records, and post-movement owned panic recovery.

Miri uses the inherited serial execution hooks for native movement. It does not execute or certify actual Rayon scheduling. The passed evidence was reused after a single test-documentation correction, with the precise source scope below; no redundant Miri sweep was represented as newly run.

## Source identity and historical failures

The source-reviewed identity was `46387bf9cc7923ed4058860f01ab0a2fa0c8ab9b048cd67aed774276b8f5401f`. Clippy later required backticks around `ByKey` in the documentation comment on line3 of `src/engine/current_main_tests.rs`. That is the only source change between the reviewed and final identities. The remaining 24 files are byte-identical; exact reverse substitution restores the original changed file. All production and executable test code is unchanged. Miri reuse is justified by this explicit scope, not by a false whole-file hash identity.

The first newly added child-counting fixture was too small: 262,145 u128 records produced tile-sized children, so correct output did not exercise the requested count hook. Its reachability assertion failed. The replacement uses 2,097,153 records, which under default root selection produces children above the 4,096-record tile threshold. It passed without changing production tuning. Initial missing test imports were also corrected before source review. Original failure logs and the failed fixture remain preserved outside the repository packet.

The initial full scalar-Rayon command was interrupted during the general parallel integration suite when research was paused. Completed unit/conformance/u128 binaries were retained; the remaining parallel/recovery/workspace binaries subsequently passed. A later test-doc-only change does not alter their executable source. Additional queued default and MSRV Rayon compilations were intentionally interrupted to yield a higher-priority CPU window, then rerun successfully. Interruption exits remain recorded and are not called successful gates. An early edited-in-flight shell wrapper also failed after a successful focused Rust test command; it was replaced by versioned immutable runners and was not reported as exiting successfully.

The previously inherited public sparse-digit early-return failure was reproduced independently in old and current-core public snapshots and fixed separately in published `d6f303b`. The native research control did not have that early return. The unfixed snapshots are not comparison baselines.

## Limits

This validates the isolated research control, not a claim of faster sorting or a replacement public API. No new heap measurement, timing experiment, cross-architecture run or real-Rayon Miri claim is made. Existing asymptotic-space reasoning and the native/std-yield design are preserved. Native actual root geometry is recorded separately; main's four-times block policy applies to generic/child partitions, not by multiplying the native root geometry.
