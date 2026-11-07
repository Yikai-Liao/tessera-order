# Combined Rayon research patch

## Scope and exact base

This is a complete, source-only research patch against commit
`60873c86558456aca496ca8c4e084007415b58b1` (validated v3), not a patch containing only the consumer wait change.

Patch: `combined-rayon.patch`

SHA256: `ac27669d2009d1b0fdadf10535beb9edb3c29d984304f0181a1512bbd09ae751`

The ten changed/added Rust files contain native sharded root scatter with segmented-scratch adoption, parallel whole-block permutation, callback-driven squeeze, publication of completed buckets to fixed consumers, the native single-digit route, and the std-thread-yield empty-queue behavior. They also contain the corresponding root/compaction tests, phase/movement witnesses, deterministic panic tests, bounded nested-public-call tests, and the pre-existing integration-test adaptation for a parallel native root.

This archive intentionally contains no whole crate, timing harness, target directory, binary, raw benchmark data or profiling metrics. The coordinator may commit this patch and its review notes as research evidence. Applying it to the default production crate is a separate review decision.

## Packaging and crate identity

Only `rust/tessera-sort/src/**` and the changed `rust/tessera-sort/tests/parallel.rs` are patched. No Cargo manifest, lockfile, repository configuration or algorithm parameter was changed during export.

The tested prototype package was named `tessera-consumer-wait` and explicitly exposed the library as `tessera_sort`. The base package `tessera-sort` already exposes exactly that Rust crate name automatically, and already has the same optional Rayon feature/dependency. Consequently no crate-alias normalization or source rewrite was needed. The prototype-only Cargo package name and explicit `[lib]` alias are not exported. The original export performed no compilation. The coordinator subsequently applied the patch to an independent archive of `52fbfa6` and completed the packaging validation recorded below.

## Independent application

Do not alter a shared checkout to test application, and do not register a worktree in its `.git` directory. For example:

```sh
repo=/path/to/tessera-rayon
base=60873c86558456aca496ca8c4e084007415b58b1
patch=/absolute/path/to/combined-rayon.patch
hashes=/absolute/path/to/combined-rayon.source-hashes.txt
work=$(mktemp -d)
git -C "$repo" archive "$base" | tar -x -C "$work"
cd "$work"
git apply --check --whitespace=error-all "$patch"
git apply --whitespace=error-all "$patch"
sha256sum -c "$hashes"
```

The mandatory check was performed on an independent `git archive` snapshot of that exact commit, without a `.git` directory. `git apply --check --whitespace=error-all` succeeded; actual application in that independent snapshot succeeded; all ten exported file hashes matched the validated source byte-for-byte; and reverse apply-check succeeded. See `combined-rayon.apply-check.txt` and `combined-rayon.source-hashes.txt`. No shared checkout, Git worktree registry, branch, commit or remote was changed by the exporter.

## Validation scope

The source exported here passed **41 native library unit tests**, with zero failures, in its isolated prototype package. This includes:

- The original ten deterministic phase-gated cases: owned strings plus exact drop counts and Send-but-not-Sync Cell records, exclusive mutable-borrow records, stable success, producer callback panic after published child movement, actual consumer-child key panic after movement, and native single-digit success/root-panic recovery.
- Simultaneous producer callback and consumer key panics, for owned+Cell and exclusive-borrow records.
- Bounded nested public `par_stable_sort` / `par_stable_sort_by_key` calls and `rayon::join` inside keys in fixed P2, P3 and P4 pools. The P2 target is one actual spawned consumer; P3/P4 target two. Native-only subprocess deadlines fail visibly rather than leaving a hanging test process.
- Existing root scatter, parallel compaction, deferred consumption, reentrant-key, clone-panic and ownership-recovery unit cases.

Two deferred exclusive-borrow kernels passed **default Stacked Borrows Miri with strict provenance**: `combined_deferred_exclusive_after_permutation` and `combined_delayed_callback_panic_exclusive_after_prior_consumption`. These use the exact root/adoption/permutation/squeeze kernels in the existing sequential/deferred configuration. They do not validate Rayon scheduling or concurrent queue execution. The known actual-Rayon Crossbeam retag limitation remains; no ignored-UB or leak workaround is represented as a clean pass.

The 41-unit-test count excludes the separate integration suite. The original metadata-only export ran no compiler or test. Packaging validation has since passed stable and Rust 1.85 all-targets: 41 unit tests, five conformance tests, 12 parallel integration tests, three panic tests, and one smoke test, for 62 top-level tests per toolchain. Six doctests and a no-default-feature check pass. Miri was not rerun as part of this packaging check; the two focused original-source Miri successes above retain their stated scope. The inherited compact.rs `for_each(&run)` Clippy warning remains, so this is not a warnings-clean production candidate. Public documentation/space-bound claims still need updating before applying the patch as a production implementation.

Suggested focused commands, only in a separately authorized validation window:

```sh
unset TESSERA_PIPELINE_ROOT_BITS TESSERA_PIPELINE_BATCH_BYTES
unset TESSERA_COMPACT_SERIAL TESSERA_COMPACT_STATS TESSERA_PROGRESS_CHILD
cargo test -j1 --offline -p tessera-sort --features rayon --lib -- --test-threads=1
MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri test -j1 --offline -p tessera-sort --features rayon --lib combined_deferred_exclusive_after_permutation
MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri test -j1 --offline -p tessera-sort --features rayon --lib combined_delayed_callback_panic_exclusive_after_prior_consumption
```

## Experimental controls and limits

These existing research controls are retained unchanged, not made into a supported production configuration API:

- `TESSERA_PIPELINE_ROOT_BITS`: optional cap on the general pipeline's selected root digit. It does not replace the separate native single-digit branch.
- `TESSERA_PIPELINE_BATCH_BYTES`: publication threshold, default 128 KiB of records.
- `TESSERA_COMPACT_SERIAL`: presence skips the optional parallel whole-block permutation stage and leaves materialization to the original compactor; the Miri path deliberately still exercises the permutation code.
- `TESSERA_COMPACT_STATS`: opt-in stderr diagnostics from the block-permutation implementation.
- `TESSERA_PROGRESS_CHILD`: native test-subprocess routing only; leave unset in ordinary validation.

Root shard scratch and worker workspaces are bounded rather than an O(n) record output buffer. The root adopts multiple scratch allocations; physical IDs are not offsets into one contiguous arena. The full experimental memory constants and public documentation still require review before production adoption.

The public-path progress argument relies on all extractor clones/root key work finishing before consumer launch, no producer user-key/Rayon-yield calls during publication, receiver locks ending before user keys, and sender destruction preceding direct draining/scope join. Tests cover cooperative nested Rayon work. They do not guarantee progress for arbitrary blocking user code or cyclic external dependencies.

This export does not implement the next Copy-only virtual-directory bridge or relax the public generic ownership guard. Root scratch segments and the adopted guard must retain one explicit owner if that boundary is extended.

Packaging-check log SHA-256: `a96f0d95e77a811265e8d015d8b967ab6a225b8f72e9d209b32e9b891845427c`. Raw logs remain outside the repository.
