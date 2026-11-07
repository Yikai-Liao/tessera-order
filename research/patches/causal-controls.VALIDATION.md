<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Validation scope and preserved failures

All three source checkpoints are archived from their final validated private copies. Standard historical Cargo package/library identity is restored; runtime and test Rust bytes are preserved. Archival staging performs no cargo invocation, build, test, benchmark, commit or push.

## Native and release scopes

- **Fixed root:** 52 native library tests passed using `cargo test --lib` in the default test profile, not an optimized release test suite. They include the inherited ownership, panic and bounded nested P2/P3/P4 guards. Release harness smoke checked 96 sorts, including warmups. The main/trace experiment verified 192 sorts, including warmups.
- **Sleep50:** the same 52 native library guards passed in the default test profile, including nested public sort/join and dual-panic guards. Release harness smoke checked 64 sorts. Main and separate trace runs verified 136 sorts, including warmups.
- **Deferred:** unchanged original O passed all 52 default-test-profile library guards. D passed 49 applicable default-test-profile library tests: nine dedicated ownership/Clone/no-launch guards, nine bounded nested public sort/join guards and shared root/serial tests. Release smoke checked 64 sorts in the final gate. Main and separate trace runs verified 136 sorts, including warmups. D is not claimed to pass O's impossible-overlap dual-panic/rendezvous fixtures.

The main benchmark/smoke harness binaries were release builds. No native-library result above is a claim that an optimized release recovery suite, full integration suite, doctests or MSRV suite was rerun for each control.

## Exact deferred guards

D witnesses actual root return and the actual `drop(sender)` point separately from the old scope-exit SenderGuard. Both spawned consumers and the producer drain assert closure before child access. Dedicated tests cover owned Cell/String payloads and drop counts, exclusive `&mut` records, producer callback panic with no children, moved-child key panic after closure, first-batch Clone panic at the retained original phase, and no-threshold/no-spawn completion. Nine 35-second-bounded nested tests retain real two-worker participation at pools P2/P3/P4.

The first D native run produced 40 passes and nine nested failures. One fixture reused a per-invocation Hooks across two successive nested sorts; the second call correctly rejected stale completed-consumer counts. The repair gives each actual direct/join sort invocation fresh hooks inside its executing closure. No production scheduler or ordering/multiworker/deadline assertion was weakened. The original failed log and corrected full passes are externally retained. Initial failure is not silently relabeled as a pass.

## Miri, formatting and lint scope

Fixed-root and sleep50 initially had native-only validation. A separate **post-hoc** shared-window check at 2026-11-07 10:15:15–10:15:25Z ran two existing exclusive deferred/permutation and callback-panic ownership fixtures against each final source. All four passed strict-provenance, default Stacked Borrows Miri. The original reports received explicit appended notes; their historical scope was not rewritten.

Final D passed four focused strict-provenance/default-SB Miri fixtures: deferred owned/exclusive root ownership and delayed owned/exclusive child-slice consumption. These validate applicable serial/deferred kernel lifetimes, not actual Rayon scheduling. Actual progress uses native tests.

Final D library/tests and the external driver passed warnings-denied Clippy. Eight explicitly listed modified files passed direct rustfmt/check without formatting frozen path dependencies. Preexisting trace-dependency missing-doc warnings were not suppressed and were not represented as a whole-workspace warnings-free run. Independent final read-only source/test review found no blocker before timing. Fixed-root and sleep50 do not inherit that later D-specific Clippy claim.

## Source-only archive checks

Every patch was independently checked/applied to a fresh export of the historical base plus the prerequisite patch. Entire file/symlink trees matched the candidate, every runtime/test Rust file matched its validated private bytes, and reverse-check/reverse-apply restored the entire prerequisite tree. Standard root Cargo.toml, Cargo.lock and crate manifest remain historical baseline bytes. Parsed crate-manifest semantics match each private source after package/library-name normalization. No current public working tree was used as an apply target.
