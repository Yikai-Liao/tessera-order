# Safety evidence and replay scope

This summarizes completed checks on the frozen isolated sources. Packaging itself performed only source reads, hashing and isolated patch apply/reverse checks, with no build, test, sort or timing run. No shared-repository files were changed. No performance acceptance is asserted.

## Final e8 paired runtime

Both sides imported the same exact upstream e8 default insertion-finishing delta. Their only differences from their respective parent freezes are that Rust delta and private Cargo package/lock identities.

Stable1.99.1, offline locked builds with one Cargo job and incremental compilation disabled:

- All-feature release, all targets: original control61 unit tests plus23 integration tests passed; candidate71 unit tests plus23 integration tests passed.
- Default backend release library tests: original56 and candidate66 passed. The candidate enabled the research scheduling feature without portable-simd, exercising the changed default finish path and count/research guards.
- All-feature/all-target Clippy with `-D warnings`: both passed.
- No-default-feature library tests:13 passed on each side.

The existing untimed10M root-geometry diagnostic was the sole ignored library test. Existing owned-record, Cell/non-Sync, exclusive-reference, key-panic, reentry and recovery guards passed. The source manifests were rechecked after validation and were unchanged from those supplied before compilation.

## Count-control coverage

Whole-array equality against Rust's stable reference covers u32/u64; widths1..8; high/low shifts including bits31/63; zero and MAX shared prefixes; skew and low cardinalities16/256; sorted, reversed and equal data; original parallel-threshold and budget neighbors; odd tails; pool sizes1/2/4/8; and nested public calls at P2/P3. Tests verify unsupported types and ByKey decline, Narrow forwarding, public route witnesses, budget overflow before allocation, serial-control geometry, and panic/nested TLS restoration.

Actual worker/phase witnesses observed multiworker count and fill and balanced output-rank chunks, not merely task submission. They also observed a concurrent nested call staying on one worker; no requirement that Rayon always uses all workers is claimed.

## Earlier parent checks, not new e8 claims

The pre-e8 count parent passed stable all-feature release70 unit tests plus23 integration tests, six doctests, no_std13 tests and Clippy. Its final package/layout-witness gate passed16 focused release tests, Clippy, a Rust1.85 test compilation and no_std check.

Rust1.85 previously passed six focused count tests. It reported an unreachable_pub warning for the unchanged Unsigned trait; stable Clippy was clean. Portable-simd requires Rust>=1.89, so this is not a1.85 all-feature claim. No new e8 MSRV run is asserted.

Earlier strict Miri, with strict provenance and symbolic alignment checks, passed the exact scalar u64 count/prefix/fill kernels and checked budget arithmetic. Those kernels are unchanged by the e8 bridge. This evidence does not claim that the imported default finish path was retested under Miri, that all u32/u64 parallel execution was interpreted, or that real Rayon is Miri-clean. The subsequent allocation witness is cfg(test)-only native instrumentation.

## Delivery boundary

`replay-proof.json` establishes source equivalence after only Cargo name normalization, not a new compiled normalized-package validation. Runtime adoption, benchmark acceptance and publication require separate decisions. Benchmark measurements, their controls and any final acceptance report are intentionally outside this packet.
