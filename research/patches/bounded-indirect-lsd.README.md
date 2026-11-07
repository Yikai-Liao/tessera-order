# Bounded indirect LSD leaves: historical negative-result archive

**Not adopted.** This optional fixed-byte LSD index mechanism passed the bounded safety checks described here, but its same-core screen did not establish an end-to-end improvement. Serial calls were slower on every measured shape; explicit 32-byte owned records also regressed in parallel. Reduced logical leaf payload copies are not a speedup. See SCREEN.md for the fixed screen and its limitations.

This is a follow-on patch for one historical research implementation. The exact apply chain is:

1. Repository commit `b1d36ce4e2b547ba72a0026dfb5850389e0d45e7`.
2. Existing `research/patches/combined-main-5b13.patch`, SHA-256 `0c75b051977dab392c66dbfee70a7f32885484ca143bc15611e819bb5bf15ba3`.
3. This packet's `bounded-indirect-lsd.patch`, SHA-256 `ac1eb4f91d4eb3317fac270a9a04bf4ebb63716b926b275e40cb9f05e5fa5565`.

**Do not apply this historical chain to the newer public scheduler at `0e50926e873b014de1d37a67a6da66da368010d0`.** The public scheduler is a separate control, not the parent of this patch. This archive does not claim the current remote implementation remains child-only.

The prerequisite research source identity is `9586fc287e595da719ec8c958f3e428254459599b3ba2306ffe1e234058d76b8`, incorporating main kernels `5b13ac1f598db8875a36f0d95c37fcb9223f9f56` with the strongest frozen native-root, parallel-permutation and std-yield pipeline. The validated integrated private identity is `83eb25a383f33b62b50ac00a80b87cb6fe0d7788925ab07ef91e9dac2d24e9ff`.

## Reproduction and source identity

Use an isolated export of the historical commit, not the shared working checkout. Check and apply the prerequisite; verify `bounded-indirect-lsd.base-hashes.txt` (20 runtime/test Rust files); check and apply this follow-on; verify `bounded-indirect-lsd.source-hashes.txt` (23 target files). Paths in the hash lists are relative to the export root. Reverse application of the follow-on must also pass `git apply -R --check`.

These steps were performed in two independent exports. The resulting entire file/symlink trees were identical, not merely the ten changed files. All 22 final runtime/test Rust files are byte-identical to the validated 83eb freeze. The only manifest normalization restores repository package `tessera-sort` and its implicit `tessera_sort` library identity; the historical Cargo presentation is retained with only the two feature lines added. Parsed crate-manifest semantics otherwise match the validated private package after that identity normalization. Root Cargo.toml and Cargo.lock are unchanged. Consequently the archived Cargo hash differs intentionally from the private freeze; both lists are supplied, not conflated.

The patch changes only the crate manifest, the optional indirect module and its tests, common tile integration, Workspace storage/accounting, and necessary test-only witnesses/public guards. Native root movement, compaction, scheduler, key extraction, typed SIMD kernels and public type signatures remain the frozen prerequisite's bytes. Feature-off preserves the literal path. No comparison-index successor is included.

## Packet contents

All filenames below have the `bounded-indirect-lsd.` prefix:

- `patch`: exact minimal follow-on diff, ten files.
- `base-hashes.txt`, `source-hashes.txt`, `validated-frozen-hashes.txt`, `SOURCE-LINEAGE.json`: distinct base, archived target and validated private identities.
- `apply-check.txt`: completed source-only application and identity checks.
- `PROOF.md`: ordering, ownership, memory and movement accounting.
- `VALIDATION.md`: exact debug/release/Miri scope, preserved failures and evidence-backed test replacements.
- `DIAGNOSTICS.md`: actual runtime root geometry and exact-shape leaf route/copy counts.
- `SCREEN.md`: negative screen summary, controls and measured heap peaks, without raw timing samples.
- `external-evidence.json`: SHA-256 references to the retained external reports/logs. Raw logs, benchmark samples, harnesses, targets and binaries are not in this packet.

The original leaf was developed against `9ed49f0ddd2b5f75bfe239590eeae8ae6d74ae93`, which includes main `72c298494ef382246aa83d08f54193cc3896de07` and the v3 child-only adapter. That old base is provenance only; it is not a strong combined performance control. The helper retained SHA-256 `0f9a9c9894a48c104603ac654b28a6623c349136b45f0ff6e805e341cc04e416` through the combined integration.

Archival staging performed no build, test, benchmark, commit or push and did not modify a shared checkout. After the negative screen, successor development was paused before any successor source/fork/build was created. This packet is an archival research artifact, not a production recommendation.
