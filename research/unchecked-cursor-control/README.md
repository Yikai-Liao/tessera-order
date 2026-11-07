<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Unchecked cursor: negative source control

This archive changes one indexing expression and adds its safety explanation. It does not apply the candidate to production. The paired geometric ratios and six-call median times are distinct estimators; the wide Row16 checked C2/C1 asymmetry remains in RESULT.md. There is no accepted gain or precise regression magnitude.

## Exact prerequisite and replay

Use research/region-child-mask at commit be857051e4c903e461691b6d629b51513b8d3c8d. Its README and provenance pin the public 55b299b core, combined and native-validation patches, historical region root and child-dispatch patch. Run its verify.py with --export to a new directory, then apply unchecked-cursor.patch there with git apply --check and git apply. No replacement Cargo metadata is provided.

The exported checked src/lib.rs SHA256 is b4a6f446478db9ed1006c49b7ae4a85496f57fe7511fc853c4ceaa2a5fbd35fc. The resulting unchecked SHA256 is 8a9c239ede5a356cfacaad9caa48d498cf8b74d77c18c4a5a3f7f30036844e68. Independent publication replay verified the prerequisite's 25 combined Rust, 37 native-validation and 34 final control files and exact one-expression replacement. Reverse application restores the checked source. No build or timing was repeated for publication.

## Safety and interpretation

Validated radix_bits determines r = 1 << radix_bits. Masking by r - 1 bounds bucket below r. Each cursor vector is created from zipped r-entry count/start rows, has exactly r elements, and is never resized during jobs. Jobs hold exclusive mutable shards. The unchecked lookup relies on these invariants; it adds no layout, ownership or scheduling change.

Eight optimized tests and the scoped Miri success are reported with their actual limits in RESULT.md. The first two failed Miri attempts remain preserved privately, not reclassified as passes. No actual Rayon-under-Miri or broad production safety claim is made.

Exact ordinary-binary inspection confirms removal of cursor length load/compare/panic branch in both shapes. Static continuing-path counts change 22 to 19 instructions for u64 and 23 to 20 for Row16. Cursor stride remains 40 bytes, one record per iteration and the full-cursor test/copy remain. Register routing, refill code and load scheduling also change; stack reservation grows 16 bytes and Row16 payload load moves. This is not attribution of timing to one branch, nor a hardware-counter measurement.

Raw timings, detailed per-call analysis, harness, binaries, environment and execution wrappers are excluded. The original reports and source remain unchanged.
