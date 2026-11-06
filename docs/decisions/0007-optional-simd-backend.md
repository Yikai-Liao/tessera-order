# 0007: Optional fearless_simd backend for the inversion scan

Date: 2026-11-07. Status: accepted.

## Context

The insertion pass looks for the next adjacent pair whose keys descend, then
moves the later record left. On near-sorted input this scan dominates the
pass, and it is the one loop in the engine that vectorizes without changing
routing, stability or the scratch budget: it reads adjacent keys, compares
them and locates the first descending pair.

The default scan compares 32 pairs per mask check and relies on the
compiler. That works for 32-bit and narrower keys, but SSE2 cannot compare
64-bit lanes, so u64 and f64 scans stay unrolled scalar code. A portable
backend can use AVX2 or AVX-512 there, and can pick the widest instruction set
of the running machine instead of the one the binary was compiled for.

The naive integration, dispatching once per scan call, was measured to cost
more than it saves on random input: almost every call finds its inversion in
the first few pairs, so the dispatch and the target-feature call dominate.
Typecasting the `Simd` token through the engine does not work either:
fearless_simd requires the code between `dispatch!` and the vector operations
to be inlined into the target-feature function. A kernel that escaped that
context measured 3.5x slower on sorted u64 and 7x slower on sorted f64.

## Decision

Add a `portable-simd` feature, off by default, that compiles the existing
insertion loop against `fearless_simd`. Dispatch happens once per insertion
pass, not once per scan: the whole loop runs inside the selected
target-feature context and the loop and kernel are marked
`#[inline(always)]`. The probe over materialized integer keys takes the same
path. The feature needs `std` and a newer compiler than the default build
(fearless_simd 1.2 needs Rust 1.89); the default build keeps its Rust 1.85
MSRV, `no_std` support and the autovectorized scan.

Every kernel is generic over `Simd`, so the crate contains no per-ISA
branch, intrinsic or target-feature attribute. The backend list and the
runtime selection belong to fearless_simd.

## Alternatives considered

- **Dispatch once per scan.** Measured net loss on random input; the
  inversion is usually in the peel.
- **Thread the `Simd` token from the public entry through the engine.**
  Correct only if the whole engine is inlined into the dispatched closure.
  Otherwise the token leaves the target-feature context and the kernel
  regresses by 3.5-7x. The measured granularity is the insertion pass.
- **Hand-written SSE2/AVX2/AVX-512 kernels.** This would put
  platform-specific code in this crate, against the portability goal, and
  duplicate what the generic kernel already expresses.
- **Choose the backend by data distribution.** Rejected: the feature must not
  be switched off for the inputs where it happens to be slower in one
  benchmark. Backend choice is per machine, not per input.
- **AVX-512CD scatters.** Future work: conflict detection could assign output
  slots for many buckets, but it needs a wider cursor table and a newer
  toolchain, and the block scatter is memory-bound.

## Consequences

- The feature changes no ordering, stability, panic or space bound. The
  default build is unchanged.
- Measured on the AVX2 build of the development machine (7 interleaved
  rounds, A/A range 0.98-1.03, raw data outside the repository): near-sorted
  inputs are 24-36% faster; most random inputs are 3-13% faster; f64 random
  is within noise. The bucket-count prefix sum is about 1.4x faster than the
  scalar chain in its own microbenchmark and moves the end-to-end random
  cases further in the same direction.
- The ByKey tile insertion still calls the user key function from scalar
  code; only the probe and Direct extraction use the vector scan. Extending
  it needs a materialized key array and is left to future work.
- A sixteen-lane prefix sum measured slower than the eight-lane one on AVX2
  because of its cross-lane swizzle; the bucket-count prefix sum uses eight
  lanes.
