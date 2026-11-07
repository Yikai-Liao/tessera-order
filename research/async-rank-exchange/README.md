<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Asynchronous rank exchange: research archive

This is the tested sealed-Copy research root and its focused source tests, not a production integration or accepted optimization. The first-direction screen found no accepted benefit; see SCREEN.md. No runtime source or Cargo configuration is changed by this archive.

## Exact prerequisite and source layout

Use the pinned region-child-mask archive at commit be857051e4c903e461691b6d629b51513b8d3c8d, path research/region-child-mask. Its verifier reconstructs the public 55b299b source, published combined patch, native-validation test patch and current region root/leaf chain. Export that verified layout into a new disposable directory, then replace only its src directory with this archive's src. Keep its leaf/src bytes unchanged. source-map.json pins all leaf and root hashes. The four inherited root modules are dereferenced exact copies, not unresolved local links.

The original native commands used direct rustc with this root as a library, the ordinary frozen leaf as extern tessera and cached Rayon 1.12.0. No Cargo wrapper is supplied or silently normalized. Build orchestration, dependency resolution and compilation are separate steps; this archive's verifier checks source only. Run `python3 verify.py REPOSITORY` to replay pinned prerequisites and compare exact hashes in disposable storage. It never builds or executes sorts.

## Evidence chronology and limits

PROOF.md is preserved from source preparation and contains historical statements that tests, memory and timing had not yet run. VALIDATION.md supersedes that execution status with 12 checked/release async tests, 13 preserved checked regressions, and ten focused strict scoped/scalar Miri cases. It does not establish actual Rayon/SIMD under Miri. Its pending screen/memory statements describe that earlier stage; SCREEN.md records the later bounded screen and conservative requested-live observations. No claim of arbitrary owned records, inconsistent callbacks, broad performance acceptance or strict per-physical-thread space follows.

model/PROOF.md and model/RESULTS.md preserve the separate token-model proof and bounded outcomes. They are not native weak-memory or pointer validation. No model harness, raw model inputs, performance driver, execution wrapper, raw logs, binary, cache, target or machine-path configuration is included. Original private artifacts remain unchanged outside this repository. Packet manifest hashes cover the published files; semantic source/evidence identities identify omitted artifacts rather than links to local files.
