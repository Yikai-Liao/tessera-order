# Validation scope

All outcomes refer to the pinned original combined candidate plus test-only patch, not the failed draining candidate. No production changes were made. Stable tools were rustc/cargo 1.99.1 (58cf8f9c3, 2026-10-13), optimized tests used release opt-level 3/debug 0/incremental disabled, with std+rayon; offline, locked, -j1. All research controls TESSERA_PIPELINE_ROOT_BITS, TESSERA_PIPELINE_BATCH_BYTES, TESSERA_COMPACT_SERIAL and TESSERA_COMPACT_STATS were cleared. These are correctness runs, not benchmark timings.

## Passed

- Optimized native lib: 68 passed, 0 failed, 1 ignored (10M root-geometry diagnostic).
- Optimized integrations: conformance 6, recovery 3, workspace_memory 1, parallel 13, duplicate-width 1; total 24 passed, 0 failed/ignored.
- Strict-provenance Miri: 10 selected cases (3 new adjacent-borrow tests plus 7 existing serial-config/scoped adoption/compaction/delayed-consumption recovery guards), all passed with nonzero exact counts. Nightly rustc 1.101.0 c980177d8 and Miri 0.1.0 c980177d81 (2026-11-06), default Stacked Borrows, `-Zmiri-strict-provenance -Zmiri-symbolic-alignment-check`. Existing sysroot reused unchanged. No actual Rayon scheduling under Miri.
- Stable library checks: default, no-default-features, rayon, all-features (four configurations).
- Stable strict Clippy: -D warnings for rayon lib and test targets (two gates), no benches.
- Cached +1.85 resolved to rustc/cargo 1.85.1: default/no-default/rayon library checks (three gates), not 1.85.0 or runtime tests.
- Stable doctests: default 6 and rayon 8 passed, configuration-specific counts (not 14 distinct examples).

The new sparse unit tests assert 512 root buckets, at least 2 actual shards, zero remaining child bits, and post-move root panic witnesses for owned Cell/String and exclusive Cell records. Duplicate-width integration includes integer/signed/reverse keys and payload IDs; existing parallel integration covers float total ordering, changing keys, drop accounting, exclusive borrows, API constraints and multiple-worker use. This does not imply every duplicate-width case uses the native route.

## Warnings and independent review

Miri retains eight cfg(miri) dead-code warning groups: engine/count_window_witness.rs:8; key/parallel_count/tests.rs:284; test_witness.rs fields 38–40, new/key 45/60, has_moved 98, with_hooks 101, assert_user_allowed 285, progress/progress_arrive 293/299. Rust 1.85.1 reports inherited unreachable_pub at key.rs:90 (Unsigned) in each configuration. No suppressions or production fixes. Stable strict Clippy was clean; those separate warning-bearing gates are not described as warning-clean.

An independent source-only reviewer checked original 25-file equivalence after stripping only two test declarations, test route witnesses, and the ownership-space ledger. No scoped production defect was found. Review corrected requested S versus actual Q, duplicate-width witness limits, and per-consumer versus per-physical-thread accounting. It did not independently reproduce the runtime commands or prove a general reentrant per-thread space bound.

## Unexecuted or incomplete

Default debug lib executable build was stopped before linking; not validated. No SIMD runtime suite, no no_std abort subprocess test, no MSRV runtime tests, no complete Miri suite, no separately rerun public Job/Pool-control suite in this stage. A no_std check is not abort-path execution; an all-features check is not SIMD runtime coverage. Public-origin integration files were executed under normal package identity.

Formal same-platform/input performance against current public original entry and strongest public/native serial controls, distribution/P-scaling, and whole-process heap evidence remain pending. workspace_memory covers retained serial Workspace only. Unique random keys do not themselves witness stability ties. Literal per-physical-thread scratch under reentrant user callbacks remains unproved. No runtime adoption or broad performance improvement claim follows from this packet.

## Resource events and evidence retention

The initial debug build hit its disk guard before linking. Exactly 9 new unfinished objects (79933440 allocated bytes) were audited and reclaimed; logs/source/frozen artifacts remained. The first Rayon doctest compilation grew between 0.1-second polls: sampled increment 16060416 bytes exceeded its 15 MiB grant by 331776 bytes, while the 150 MiB free reserve held. Exactly 4 unexecuted doc temporary files (13434880 allocated bytes) were later audited/reclaimed. Exactly 6 isolated MSRV check-only .rmeta files (4243456 allocated bytes) were audited/reclaimed to enable a separately approved retry. No source, logs, frozen binaries, other task files or Miri sysroot were deleted.

The successful Rayon-doc retry used a 30 MiB grant with 0.02-second sampling: increment 24121344 bytes, minimum free 165392384 bytes, within that grant/reserve. Historical overshoot evidence remains retained separately, not overwritten by success. Polling metrics are sampled, not a continuous hard-quota guarantee. All source/release identities were rechecked after later stages. Local raw logs, audit records and executable freezes are retained outside this public source packet.
