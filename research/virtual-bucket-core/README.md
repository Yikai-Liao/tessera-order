# Serial virtual-bucket foundation: research checkpoint

**Validated serial ownership foundation, not a competitive parallel sorter.** The standalone `[workspace]`
keeps it out of the repository workspace's default tests. It has no dependencies.

Exact validated `src/lib.rs`, integration tests, Cargo.toml and Cargo.lock are
preserved. Only the benchmark `src/main.rs` and evidence/build artifacts are
excluded. Package name stays `virtual-bucket-prototype`, preserving test imports.

## What it establishes

Sealed Copy u64/Row16 records stay in caller blocks and bounded scratch while
root directories are adopted, fixed-depth virtual MSD/LSD processing sorts them,
and global repacking plus separate physical restoration produce a stable slice.
Scheduling is serial. Small leaves use a bounded comparison merge, not Tessera's
cache radix kernel. No adapter to independently allocated combined-root shards
is included, and no generic owned-record/key-panic guarantee is established.

The only n-record allocation belongs to the caller. Extra record capacity is
exactly `((P_root+3)R+8)B + 2K`, including scratch retained in output, with each
leaf buffer capped at 64 KiB. Metadata is O(n/B+P_root R) at fixed depth.
`PROOF.md` preserves the originating serial slot-budget/stability argument.
Reported stage-boundary metadata is not an actual allocator or RSS peak.

## Originating validation

- Debug and release: eight tests, 3,643 full-reference comparisons per suite
  (3,636 Row16 and seven u64 cases).
- Clippy with warnings denied: passed.
- Doc tests: passed, zero examples.
- Ordinary Stacked Borrows Miri with strict provenance: two focused targets,
  ten full-reference boundary cases.

The tested source covers stable ties, MAX/equal/skew/low-cardinality keys,
fragmented root tails, recycled input/scratch blocks, fallback depth, and final
restoration. Full Miri-suite or parallel-performance claims are not made.
The originating crate had not been run on Rust 1.85. Packaging validation now
passes its eight tests on cached Rust 1.85; see `INTEGRATION_VALIDATION.md`. Source hashes and lineage are in SOURCE_LINEAGE.json.
Originating evidence: `tessera-virtual-bucket-prototype/evidence/VALIDATION.md`.
Raw logs, benchmarks and binaries are excluded from this checkpoint.

## Reproducing packaging validation

    cargo test --manifest-path research/virtual-bucket-core/Cargo.toml --offline -j1 --all-targets
    cargo test --manifest-path research/virtual-bucket-core/Cargo.toml --release --offline -j1 --all-targets
    cargo clippy --manifest-path research/virtual-bucket-core/Cargo.toml --offline -j1 --all-targets -- -D warnings
    cargo test --manifest-path research/virtual-bucket-core/Cargo.toml --offline -j1 --doc
    MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri test --manifest-path research/virtual-bucket-core/Cargo.toml --offline -j1 --test stability miri_smoke

Packaging validation also ran the cached Rust 1.85 toolchain. No performance
measurement was performed as part of packaging.
