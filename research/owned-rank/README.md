# Owned-record rank merge: research checkpoint

**Narrow validated research kernel, not a production API.** It is isolated by its own `[workspace]`, so the repository
workspace's default test command does not automatically include it.

The originating kernel passed release and Rust 1.85 native suites (12 groups)
and ordinary Stacked Borrows Miri (three focused owned targets). Exact Rust/test
source bytes and Cargo.lock are preserved here. The only code-configuration
change is the dependency path: `../../rust/tessera-sort`. No core is vendored.
The intended repository layout must supply the current generic Tessera serial
kernel and its panic recovery. The package was revalidated against integration commit `f59a424`; see
`INTEGRATION_VALIDATION.md`.

## Scope

- `T: Send`, with `F: Fn(&T) -> u64 + Sync`; no Copy, Clone or T:Sync requirement.
- Stable local Tessera runs, monotone rank cuts, bounded block recycling, loser
  tree, and bounded parallel restoration.
- Normal key panics are caught, drained without further keys, joined, restored,
  then resumed. Tests include String/wide payloads, Cell, exclusive references,
  inconsistent/nested keys and independently panicking jobs with payload drops.
- Uncaught internal/scheduler unwinding during movement aborts as a memory-safety
  backstop. This is not recovery from every possible exception.
- No full RadixKey/u128/FnMut/cached-key API or owned-record performance claim.

`OWNERSHIP.md` contains the ownership ledger, checked-layout and seed-capacity
arguments. Record scratch is O(K²B), with O(n/B+K²) metadata; B near sqrt(n)/K
provides O(K sqrt(n)) auxiliary space. Parameter choices and small-input dispatch
remain research concerns, not an accepted production policy.

## Evidence and limitations

The originating native suite includes 216 stable owned cases, 36 phase-panic
cases, four exclusive-reference cases and three inconsistent-key schedules.
The multi-panic checks exercise three independently failing jobs under scoped
and actual Rayon schedules. Miri covers six focused sort/recovery cases, one
three-job panic batch, and exact scoped restoration with owned and exclusive
reference records. These are focused checks, not exhaustive panic-position proof.

Scoped Miri deliberately tests the same worker kernel without Crossbeam. It does
not clear the separately reproduced Rayon/Crossbeam Stacked Borrows limitation.
An unused inherited serial helper warning remains; no warning-clean claim is made.
The source's older “Unvalidated source” banner is retained to preserve exact
validated source bytes; this README describes the later recorded validation.

`SOURCE_LINEAGE.json` records original and staged hashes. Raw logs, executable
artifacts and benchmark tooling are intentionally excluded. The originating
record is `tessera-rank-owned-prototype/evidence/multi-panic-validated-source/`.

## Reproducing packaging validation

After placing this directory at the repository's `research/owned-rank`:

    cargo test --manifest-path research/owned-rank/Cargo.toml --release --offline -j1 --lib -- --test-threads=1
    cargo +1.85 test --manifest-path research/owned-rank/Cargo.toml --offline -j1 --lib -- --test-threads=1
    cargo +nightly miri test --manifest-path research/owned-rank/Cargo.toml --offline -j1 --lib owned_miri -- --test-threads=1

Use installed/cached toolchains; report missing dependencies rather than silently
installing or changing the integration tree. Format only explicitly named owned
files with rustfmt. Do not run `cargo fmt --all` through local path dependencies.
