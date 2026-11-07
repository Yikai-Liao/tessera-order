# Executed validation and reproducibility

All results below predate this source-only packaging. Packaging ran no builds, tests or timings. Independent read-only review verified the final whole-file manifests, exact diff reconstruction, lock records and the new fixtures; it found no blocking issue.

## Tested source and Cargo normalization

The tested private baseline identity is `19659df2fd57dfcde3b82892bb01b84a6a5770cadd5094bb9815a6cfd70d9f17`; candidate identity is `b4c51ddca807a94260e6399800f17f06b4463fde6a4162f07a9f6df094d84f45`. These identities hash sorted JSON dictionaries of all source-package file hashes, including private Cargo identities.

Initial source-ready manifests were retained. The first offline Cargo invocation pruned packages belonging only to the omitted benchmark workspace and normalized each private package name. No common registry dependency record changed. Final locks were compared by `(name,version,source)` including complete checksums and dependency lists: both retained exactly crossbeam-deque0.8.8, crossbeam-epoch0.9.21, crossbeam-utils0.8.23, either1.19.0, fearless_simd1.2.0, rayon1.12.0 and rayon-core1.13.0, identical to the corresponding public lock records. Later validation used --locked. Final tested manifests were regenerated and independently verified after normalization and final test additions.

This packet deliberately excludes private Cargo edits and lock pruning. Public replay retains the original published Cargo files. All18 candidate Rust files match the tested candidate byte for byte, including unchanged native tests; identities.json provides the full dictionary. Independent source-only replay applied the patch, verified the complete Rust file set and hashes, reversed it and verified all15 baseline Rust files, then applied it again and reverified all18. Cargo files were checked unchanged at every step. See replay.json.

## Native gates

Rust1.99.1 (58cf8f9c3,2026-10-13), offline -j1, incremental compilation disabled:

- Release all targets, rayon+portable-simd: baseline22 unit+23 integration tests; final candidate32 unit+23 integration tests, all passed.
- Release all targets, rayon/default backend: baseline17 unit+23 integration; final candidate27 unit+23 integration, all passed.
- Plain default-feature release library:11 tests each passed.
- No-default-feature library:8 each passed. Later test additions are cfg(test,rayon) and excluded here.
- All-target/all-feature Clippy with -D warnings passed for both, repeated after final candidate test additions.
- Rust1.85.1 (4eb161250,2025-03-15) check --tests --features rayon passed for both, repeated after final candidate test additions. This is compilation evidence, not an executed MSRV suite. The inherited Unsigned unreachable_pub warning remains. Portable SIMD requires>=1.89 and was not included in the1.85 check.

Release fingerprints confirmed [default,portable-simd,rayon,std] and [default,rayon,std]. No research feature exists in this candidate.

Original public owned/Cell/exclusive-borrow, panic, inconsistent-key, movement/reentry-sensitive, sparse-root and bucket provenance guards were retained without weakened assertions. New tests cover widths1..8, upper windows including low31/63, MAX/fixed bits, skew, empty buckets, uneven tails/rank boundaries, threshold and budget declines, checked overflow, pools1/2/3/4/8 and nested calls. Negative routes cover fullrandom, sorted/reverse/equal, unsupported primitives/wrappers and owned ByKey payloads.

A late high-bit sentinel is explicitly absent from the actual probe_order and sample_keys read sets and beyond the actual insertion-probe prefix. Its public sort preserves the complete stable reference and does not enter count. Large nonmonotone f32/f64 fixtures use adjacent positive finite raw bits; assertions prove encoded width<=8, public size eligibility and both ascending/descending adjacent pairs, yet the new hook remains unused.

Untimed nested work observation recorded actual multiple worker IDs in P3 count and fill, with output rank lengths349527/349527/349525. In the P2 nested observation, each sort's phases happened to run on one worker. Task creation alone is not evidence of actual parallel execution, and no universal occupancy guarantee is claimed.

## Fresh strict Miri scope

Rust1.101.0-nightly (c980177d8,2026-11-06), strict provenance and symbolic alignment checking, rayon compiled but no Rayon pool execution:

- scalar_count_prefix_rank_fill:1 passed, exact scalar kernels across windows and output boundaries.
- checked_budget_rejects_overflow_and_small_inputs:1 passed.
- unsupported_types_and_payload_extractors_decline:1 passed; invalid windows return before Rayon/allocation.
- Original bucket_borrow_tests:3 passed, actual deferred callback/Job handling with owned Cell stability/drops and exclusive Cell borrows, without spawn execution.

This is not real-Rayon Miri coverage or exhaustive public parallel execution coverage. Test-only unused-code warnings arise from native-only fixtures excluded under Miri.

All validation CPU sessions held the same shared lock throughout; final validation released it at2026-11-07T16:00:26Z with exit0. This packet makes no performance, acceptance or default-adoption claim.
