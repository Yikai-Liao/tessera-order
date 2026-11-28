// SPDX-License-Identifier: BSD-2-Clause

//! Portable, safe SIMD search kernels built on Tessera's `fearless_simd` + `SimdKey`.
//!
//! Callers dispatch once via `fearless_simd::dispatch!(Level::new(), simd => ...)` at the
//! entry of `get` / `insert` / `remove` / `range`, and every node search inside the tree
//! walk compiles to inlined native SIMD instructions across x86 (SSE/AVX2/AVX-512), Arm
//! NEON, WebAssembly SIMD128, and scalar fallback without any target-specific `cfg` or
//! `unsafe` intrinsics.

use crate::engine::key::Unsigned;
use fearless_simd::Simd;
use fearless_simd::prelude::*;

/// Counts elements in `slice` strictly less than `target` using the dispatched `fearless_simd`
/// backend. When `slice.len()` is a compile-time constant (`7`, `8`, `16`), LLVM unrolls the
/// vector loop into straight-line native SIMD instructions with zero branches.
#[inline(always)]
pub(crate) fn count_lt_slice<S: Simd, E: Unsigned>(simd: S, slice: &[E], target: E) -> usize {
    let lanes = E::lanes::<S>();
    if lanes == 1 {
        let mut count = 0usize;
        for &k in slice {
            count += usize::from(k < target);
        }
        return count;
    }
    let t = E::splat_encoded(simd, target);
    let mut count = 0usize;
    let mut i = 0usize;
    while i + lanes <= slice.len() {
        let v = E::load_encoded(simd, &slice[i..i + lanes]);
        count += v.simd_lt(t).to_bitmask().count_ones() as usize;
        i += lanes;
    }
    while i < slice.len() {
        count += usize::from(slice[i] < target);
        i += 1;
    }
    count
}

#[inline(always)]
pub(crate) fn count_lt_7<S: Simd, E: Unsigned>(simd: S, sub_max: &[E; 7], target: E) -> usize {
    count_lt_slice(simd, sub_max.as_slice(), target)
}

#[inline(always)]
pub(crate) fn count_lt_8<S: Simd, E: Unsigned>(simd: S, block: &[E; 8], target: E) -> usize {
    count_lt_slice(simd, block.as_slice(), target)
}

#[inline(always)]
pub(crate) fn count_lt_16<S: Simd, E: Unsigned>(simd: S, keys: &[E; 16], target: E) -> usize {
    count_lt_slice(simd, keys.as_slice(), target)
}
