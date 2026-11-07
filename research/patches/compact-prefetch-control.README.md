# Rejected compact source-prefetch control

Apply this research-only patch after `combined-rayon.patch` on its explicit historical base. It adds an optional `prefetch-compact` feature to the existing combined kernel; it is not applied to the default crate and is not a performance recommendation. The measured source retained the old core, not current SIMD main.

The feature prefetches every cache line in the source block four copy positions ahead on x86-64, with no change to physical copy order or workspace. A shared inherited closure-borrow lint is also fixed. Feature-on/off benchmark packages have byte-identical Rust sources. All41 native release library tests and warnings-denied Clippy passed. Two default Stacked Borrows strict-provenance moving-kernel Miri tests passed, but hardware intrinsics are explicitly disabled under Miri.

The [negative screen report](../../docs/development/compact-prefetch-control.md) records84 measured and28 warmup stable-reference checks. No case establishes a favorable effect beyond controls; u64 W8 is adverse. No distance tuning was pursued. Raw data, profiles, harness and binaries stay outside Git.

Frozen compact source SHA256: `f92e23158f6410bc1f4bad0576d2aa119886047e05033964ec5247aac12de426`. Source report SHA256: `1163b7bfe4514c29ccea55726ed7b3ea9432f5297678434831eb498e06ae9970`.
