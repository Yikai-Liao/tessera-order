# 0007: Optional fearless_simd backend for the inversion scan

Date: 2026-11-07. Status: accepted.

The default insertion-finishing scan is amended by [decision 0014](0014-scalar-default-insertion-finishing.md).

## Context

The insertion pass looks for the next adjacent pair whose keys descend, then moves the later record left. On near-sorted input this scan dominates the pass, and it is the one loop in the engine that vectorizes without changing routing, stability or the scratch budget: it reads adjacent keys, compares them and locates the first descending pair.

The default scan compares 32 pairs per mask check and relies on the compiler. That works for 32-bit and narrower keys, but SSE2 cannot compare 64-bit lanes, so u64 and f64 scans stay unrolled scalar code. A portable backend can use AVX2 or AVX-512 there, and can pick the widest instruction set of the running machine instead of the one the binary was compiled for.

The naive integration, dispatching once per scan call, was measured to cost more than it saves on random input: almost every call finds its inversion in the first few pairs, so the dispatch and the target-feature call dominate. Typecasting the `Simd` token through the engine does not work either: fearless_simd requires the code between `dispatch!` and the vector operations to be inlined into the target-feature function. A kernel that escaped that context measured 3.5x slower on sorted u64 and 7x slower on sorted f64.

## Decision

Add a `portable-simd` feature, off by default, that compiles the existing insertion loop against `fearless_simd`. Dispatch happens once per insertion pass, not once per scan: the whole loop runs inside the selected target-feature context and the loop and kernel are marked `#[inline(always)]`. The probe over materialized integer keys takes the same path. The feature needs `std` and a newer compiler than the default build (fearless_simd 1.2 needs Rust 1.89); the default build keeps its Rust 1.85 MSRV, `no_std` support and the autovectorized scan.

Every kernel is generic over `Simd`, so the crate contains no per-ISA branch, intrinsic or target-feature attribute. The backend list and the runtime selection belong to fearless_simd.

Both backends use the engine's insertion loop through its extractor interface. Backend selection lives in the sealed key implementations; `RadixKey` describes only the ordering encoding. Encoded unsigned probe keys are direct keys too, so they need neither a separate extractor nor a second insertion algorithm.

## Alternatives considered

- **Dispatch once per scan.** Measured net loss on random input; the inversion is usually in the peel.
- **Thread the `Simd` token from the public entry through the engine.** Correct only if the whole engine is inlined into the dispatched closure. Otherwise the token leaves the target-feature context and the kernel regresses by 3.5-7x. The measured granularity is the insertion pass.
- **Hand-written SSE2/AVX2/AVX-512 kernels.** This would put platform-specific code in this crate, against the portability goal, and duplicate what the generic kernel already expresses.
- **Choose the backend by data distribution.** Rejected: the feature must not be switched off for the inputs where it happens to be slower in one benchmark. Backend choice is per machine, not per input.
- **Materialize the ByKey tile keys.** Measured on an 8192-record near-sorted tile: extracting the keys once and moving them alongside the records is 1.4-1.6x slower than the closure path. The default scan already calls the key function once per record, so materialization pays the same extraction cost plus a key array and a second swap per move. It can only win for expensive key functions, and the probe already covers the first tile; the tile finish stays with the closure.
- **AVX-512CD scatters.** Rejected by the compress measurement: a stable three-way compress scatter of 64K u32 values into three buckets took 460us against 147us for scalar cursor writes, 3.1x slower. Conflict detection would add a wider cursor table — u16 to u32 doubles the 32 KiB table — on top of the same failure: the scalar per-bucket cursor is already cheap and the move cost, not the classification, dominates.

## Consequences

- The feature changes no ordering, stability, panic or space bound. The default build is unchanged.
- The existing AVX2 measurements are exploratory: seven interleaved rounds of separate binaries, summarized by per-round minimum times, with a separate A/A run. They do not implement the nine-block paired timing, duplicate baseline and output-hash checks required by the [benchmark protocol](../development/benchmarking.md). Formal speedup or regression claims need measurements under that protocol; continuous benchmarks provide regression triage rather than that acceptance evidence.
- The ByKey tile insertion still calls the user key function from scalar code. Materializing the keys measured slower at the tile size, so this is not a pending optimization: only the probe and Direct extraction use the vector scan.
- A sixteen-lane prefix sum measured slower than the eight-lane one on AVX2 because of its cross-lane swizzle; the bucket-count prefix sum uses eight lanes.
