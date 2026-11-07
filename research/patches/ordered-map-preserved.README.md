# Historical preserved-generic ordered-map control

Research-only historical checkpoint. Not adopted and not a patch or validation for current main 0bc80b49 (including df56).

Apply independently to a clean export of b1d36ce4e2b547ba72a0026dfb5850389e0d45e7:

1. Apply the existing combined-main-5b13.patch, SHA-256 0c75b051977dab392c66dbfee70a7f32885484ca143bc15611e819bb5bf15ba3.
2. Apply ordered-map-preserved.patch.

Do not apply or stack the older ordered-map.patch. This is the historical combined-main-5b13 lineage, not the coordinator-reported newer main df56f2c with a 4x block-size policy change. No rebase or validation against that newer policy is claimed.

The entire original block.rs is preserved byte-for-byte. Only the successful-permutation dispatcher branch selects a separate ordered implementation. Generic compaction, serial fallback, one-shard fallback, Drop and permutation-fault recovery retain the original code. The three-way parity fixtures do not instrument the original generic production function.

SOURCE-LINEAGE.json records reconstruction and reverse-application checks. GENERIC-PRESERVATION.json and REFERENCE-NORMALIZATION.diff provide exact function ranges, hashes and reversible trace normalization. VALIDATION.json records the actual final gates and feature/toolchain limits. The source hash lists cover 20 base and 23 target runtime/test files. The private and standard manifests explicitly distinguish their package/configuration identities.

The validated private crate is named tessera-combined-main-port. The reconstructed historical package remains tessera-sort, with all Cargo files unchanged by this follow-on. All 23 runtime/test files are identical to validated private source; no build was performed in staging. The private workspace excludes bench/rust, so the 78 release-test result does not validate the separate benchmark member. The private lockfile has all 8 package/dependency records identical to the historical lockfile after package-name normalization. A pre-existing benchmark-source difference is recorded in the lineage, but neither benchmark harness is copied into this packet.

The appended [SCREEN.md](ordered-map-preserved.SCREEN.md) records the completed three-case historical screen: all G/O comparisons remain inconclusive under the declared controls, with no adoption or proved regression fix. It retains every reported ratio range and tail. Raw logs, per-sort data, benchmark harnesses and binaries remain external; [external-evidence.json](ordered-map-preserved.external-evidence.json) records their identities. Source preservation does not guarantee identical generated code.

The [revision note](ordered-map-preserved.REVISION.md) preserves the original 13-file packet hash and identifies this documentation-only extension. The unchanged source proof/lineage files retain their original source-only packaging scope; SCREEN.md adds the later historical measurements. The packet manifest hashes every other packet file and intentionally excludes itself.
