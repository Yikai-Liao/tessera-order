# 0037: Measure release comparisons locally with an offline report

Date: 2026-11-07.

## Context

The continuous Bencher suite answers whether a change regresses Tessera. Release comparisons need multiple algorithms, larger inputs, independent allocation measurements and a single environment across all lengths. Hosted job limits would otherwise determine the measurement groups and restrict absolute-time comparisons.

## Decision

Run release comparisons on one maintainer-controlled machine in one session. The same commands work on a contributor's machine. Record host conditions without changing them. Store build provenance, the host fingerprint, verified block samples and allocation peaks in a results directory; a TypeScript report derives statistics, SVGs and optional Bencher summaries from those files alone.

Link pinned C and C++ competitors through native-buffer shims in the Rust runner. Fetch third-party sources separately into an ignored directory and verify their archive and tree hashes. Use fixed baseline compiler targets, with upstream runtime ISA dispatch for SIMD sorts. Unsupported task combinations remain explicit capability gaps.

Use separate default and Portable SIMD executables, each with Tessera's A/A pair and a shared standard-library stable-sort anchor. Measure allocations in another executable so its instrumentation does not affect timing. Keep statistical definitions in the report instead of duplicating them in the runner.

## Consequences

Release figures can show absolute scaling on one recorded environment. Groups support interrupted-run recovery; parts merge only with matching host fingerprints and build provenance. Results archives contain enough evidence to regenerate every published number. CI checks correctness and report generation; the release-candidate run and its performance claims require a complete local session.

Hosted comparison runs can reuse the runner and result format later. The existing continuous suite, histories and case IDs keep their regression role.
