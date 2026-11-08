<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Checked hot/cold cursor: closed negative control

This source-only archive records a rejected combined cursor-layout and source-grouping hypothesis. It changes no runtime, Cargo metadata, routing policy, main branch or pull request. See RESULT.md for the paired estimates, wide A/A controls, retained Row16 native tail and validation limits. No accepted gain or measured cache benefit was found.

## Exact prerequisite and replay

Use research/region-child-mask at commit be857051e4c903e461691b6d629b51513b8d3c8d. Follow its README and provenance: public 55b299b core, combined patch, native-validation patch, child-entry patch, historical region root and root-dispatch patch. Do not use the unchecked-cursor candidate. Its verify.py reconstructs and verifies 25 combined Rust files, 37 native-validation files and 34 final control files without building.

Run that pinned verify.py with REPOSITORY --export NEW_DIRECTORY, then in the new directory run git apply --check followed by git apply with this archive's hot-cursor.patch. The checked src/lib.rs SHA256 must be b4a6f446478db9ed1006c49b7ae4a85496f57fe7511fc853c4ceaa2a5fbd35fc; the result must be 9ad13ad1555a5ea9c3b58a7274d5bf684e5d2e7a848dedd67b4a09296a46ab2d. All other reconstructed files remain unchanged. git apply --reverse --check and git apply --reverse restore the checked hash. No replacement build configuration is supplied.

Publication independently verified the pinned prerequisite, exact forward/reverse application, source and raw-evidence identities and the 64 verified calls (48 measured, 16 warmups). It did not rebuild, rerun tests, Miri or timing, or create a compiler cache. Full repository checks were not rerun for this source archive. Historical optimized tests and scoped Miri are evidence for the frozen candidate, not a fresh full-workspace CI pass.

## Failure lineage and limits

V1 emitted outlined per-record calls and failed the mechanism gate; no dynamic validation, Miri or timings ran. Its validation attempt stopped at a mistaken resource preflight. V2 inlined the helper and passed native/scoped-Miri checks, but retained a u64 lane loop and failed the unroll gate; no timings ran. V3 spelled out four/two source calls and passed the ordinary code-generation gate. It first failed its original output cap while linking the driver. The original result remains failed. A separately approved driver-only resume reused unchanged existing root and validation artifacts, unchanged source/flags, and fresh driver output. The successful bundle retains original_build_passed=false. Nothing here reclassifies that first build as passed.

The archive pins original evidence identities without publishing raw samples, harnesses, binaries, target directories, machine paths or environment snapshots. Historical failures remain retained at their original source. Only the final minimal candidate patch is distributed here. Scope is sealed intrinsic-key Copy u64/Row16/Row128, not arbitrary Owned records, callback panic recovery or actual Rayon-under-Miri validation.

Tool versions: rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1; scoped Miri used rustc 1.101.0-nightly (c980177d8 2026-11-06). Source field payload remains 40PR bytes, with 24P extra vector-header bytes and P extra allocations on the tested 64-bit target. This is not a heap-peak measurement. PROOF.md gives the ownership and space argument.
