# Completed safety gates and limits

These results belong to the frozen e1 source pair identified in replay-proof.json. The follow-on packaging step performed only source reads, hashing and isolated apply/reverse checks. It ran no build, test, sort or timing workload and made no shared-repository edit. The source snapshots remained unchanged throughout the earlier safety gates.

## Production OFF / optional research ON

Stable1.99.1, offline locked builds, one Cargo job, incremental compilation disabled:

- Production release, all targets, features rayon,portable-simd, research OFF: original63 unit tests plus23 integration tests passed; candidate71 unit tests plus23 integration tests passed. Actual release library feature fingerprints were [default,portable-simd,rayon,std], excluding research.
- Default backend release library tests, features rayon, without portable-simd or research: original58 and candidate66 passed.
- No-default-feature library tests:15 passed for each variant.
- All-feature/all-target Clippy with -D warnings: both passed.
- Rust1.85.1 `check --tests --features rayon`: both compiled. This is an MSRV compilation gate, not a new full1.85 execution suite. The compiler emitted the previously recorded unreachable_pub warning on unchanged Unsigned. Portable-simd requires >=1.89 and was excluded from this gate.
- Separate research-ON release test binary: both focused scope tests passed, covering nested/panic TLS restoration and equal serial/parallel table/chunk geometry and values.

The original untimed10M root-geometry diagnostic was the sole ignored library test. Existing producer, key-panic, reentry, owned-record, Cell/non-Sync, exclusive-reference and recovery assertions were preserved and passed. Candidate count route, overflow, public-entry and fallback coverage also passed. No assertion was weakened to obtain these results.

## Fresh focused Miri, with precise scope

Nightly1.101.0-nightly, strict provenance and symbolic alignment checking, ran the two new candidate e1_tilecap_tests with default library features and no Rayon or portable-simd. Both passed:

- Numeric tile-cap boundaries, including zero/small counts and overflow-sized arithmetic inputs.
- Non-Copy16/128-byte rows, compared as whole records against a stable reference, at63/64/65 records. Tests call tile completion directly and node/tile routing around a separate fixed64-record tile budget. Native variants extend boundary lengths through513.

The imported engine.rs and test module are byte-identical in both controls, but the new Miri invocation was on the candidate only. This is limited serial wide-cap/tile-boundary coverage, not exhaustive wide-record validation or real-Rayon Miri cleanliness. It does not assert a new direct sealed finish_insertion Miri run.

Earlier strict scalar u64 count/prefix/fill and budget-kernel Miri evidence applies only to those unchanged kernels. That earlier evidence alone did not cover the imported wide cap; the new focused tests above supply its stated limited coverage.

## Source and delivery boundaries

The new common patches introduce no unsafe blocks or tokens. The actual >8-byte cap rule, including its possible effect on128-byte records, is what the tests assert; no unchanged128-byte claim is made. The original count storage/budget proof and allocation-layout hypothesis remain as documented in the preceding checkpoint, without new padding or a measured false-sharing assertion.

Replay proves source equivalence after Cargo-name normalization, not a fresh compiled validation of the normalized package identity. Raw logs with local paths, raw benchmark samples, harnesses, targets and binaries are excluded. Validation summaries and non-sensitive evidence hashes are in validation.json. Measurement conclusions and publication/adoption decisions remain separate.
