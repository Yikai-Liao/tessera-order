# Packaging validation and stop

This external archive preserves the completed old-core stage1/stage2 experiments.
It does not continue the virtual-bucket research line, and no further runs are
scheduled. The user redirected further work to the best combined implementation.

## Passed without new timing

- Every copied Rust source, driver, Cargo manifest, Cargo.lock, and selected
  historical evidence file matches its frozen source byte-for-byte.
- All 87 stage1 and 102 stage2 vendored Rust file hashes match the original
  measurement provenance, plus 2 unchanged driver files per stage.
- All 11 stage1 and 12 stage2 Cargo path references resolve within their stage;
  no manifest edits or absolute dependency paths were introduced.
- Each stage's summary.json was recomputed from copied raw timing/heap JSONL
  with the unchanged summarizer and matched the historical file byte-for-byte.
- Historical data checks confirm 48 verified timing outputs and 16 verified
  allocator outputs per stage, with zero end-of-call extra live heap balances.
- The portable Bash launcher passes bash -n. The SHA verifier passes.

## Relocated compilation: intentionally partial

The shared CPU lock was acquired at 2026-11-07 07:46:45 UTC. Stage1's release,
locked, offline, -j1 timing build completed successfully (46.70 seconds), followed
by driver Clippy with -D warnings (2.92 seconds). The count-alloc build had begun
when the user redirected the work; the pipeline was interrupted immediately
with exit 130. Its partial output is retained in validation/stage1-build.log.

No successful relocated count-alloc build or count-alloc Clippy is claimed.
No relocated stage2 build, Clippy, smoke, timing, or allocator run was started.
No relocated stage1 smoke, timing, or allocator run was started. No new Miri,
MSRV, full algorithm tests, documentation build, or acceptance benchmark was run.
Historical owner reports describe earlier checks on the original frozen copies,
not these missing packaging checks.

The pipeline interruption closed its lock descriptor. A separate nonblocking
exclusive lock check succeeded on the same device 27 / inode 1578019 and released
it at 07:47:57 UTC, proving that the interrupted pipeline no longer held the
shared lock. This check performed no build or benchmark. See
validation/stop-and-release.log; the interrupted pipeline's EXIT trap did not
produce a RELEASE line, so none is fabricated in its preserved log.

## Portability limitations

The package preserves five pre-existing dangling native-crate README symlinks.
These are documentation links, not missing Cargo dependencies or Rust sources.
Binary reproducibility is not bit-for-bit guaranteed after relocation, since
release debug information contains paths. Registry crate sources are not bundled:
offline reproduction needs the locked crates cached and a compatible toolchain.
Original host-specific launch scripts and hash paths are provenance only.

The full archive must remain outside the repository under its no-raw-data,
no-profile, and no-research-harness rule. The separate repo-checkpoint contains
only English reports, source/evidence/binary hashes, and these validation limits.
