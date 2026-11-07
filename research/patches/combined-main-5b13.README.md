<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Strong combined current-main research checkpoint

## Artifacts

- Runtime/tests patch: `combined-main-5b13.patch`.
- Machine-readable lineage, changed-file list and target hashes: `combined-main-5b13.lineage.json`.
- Performance report: `combined-main-5b13.SCREEN.md`.

Patch SHA-256: `0c75b051977dab392c66dbfee70a7f32885484ca143bc15611e819bb5bf15ba3`.

The baseline, candidate and independent verification archives, complete source
manifest and raw evidence remain outside this repository. The archive names
below describe the original packaging checks, not additional included files.
The coordinator independently repeated apply, all twenty source-hash checks
and reverse-apply checks without applying the candidate to production.

## Lineage and exact scope

Baseline is shared-repository commit
`b1d36ce4e2b547ba72a0026dfb5850389e0d45e7`, with current-main
`5b13ac1f598db8875a36f0d95c37fcb9223f9f56` serial kernels and the production
child-only Rayon path. The source input is the strongest native-root plus
parallel-permutation plus standard-thread-yield bucket pipeline from frozen
`tessera-consumer-wait/candidate`, ported onto that main and validated in
`tessera-combined-main-port`.

Validated port identity:
`9586fc287e595da719ec8c958f3e428254459599b3ba2306ffe1e234058d76b8`.

The patch changes twelve Rust runtime/test files only. It carries native root
scatter, retained per-shard scratch, parallel permutation/compaction, the
standard-thread-yield pipeline, phase witnesses and safety/progress tests. It
adds exact boundary/partial-insertion and public full-u128/narrow-half tests.
The public movement-panic witness is adapted so root work cannot be mistaken
for a child that has already moved records. No existing test is removed.

There is no package renaming, Cargo patch, benchmark, raw data, harness, halo,
prefetch, virtual-bucket route, threshold change or upstream scan rewrite in
this integration patch. The upstream engine change is only the movement
witness hook. Current-main key/scan/SIMD implementations already match the
validated port. The approved inherited Clippy cleanup uses `.for_each(run)`.

## Cargo and apply checks

Parsed crate Cargo manifests are semantically identical after ignoring the
private port's package name and explicit library-target name. The repository
crate remains `tessera-sort`, with the implicit `tessera_sort` library name.
Root Cargo.toml, Cargo.lock and crate Cargo.toml are byte-identical in base,
candidate and verify archives. Existing default, rayon and portable-simd
feature/dependency definitions are preserved.

The patch passed `git apply --check` and was applied to an independently
extracted clean archive. Every resulting file and symlink matches candidate/,
not only edited files. All twenty runtime/test Rust files exactly match the
validated port. The shared working tree was not used as an apply target and
was not modified. No staging compile, commit, push or promotion was performed.

## Validation claims and limits

- Rust 1.99.1 **debug-profile** all-target suites: Rayon with SIMD off, 47 unit
  plus 22 integration tests; Rayon with SIMD on, 52 unit plus 22 integration
  tests. Six doctests passed for each parallel feature set. Serial default and
  SIMD library tests plus no_std checks also passed.
- Rust 1.85.1 default unit, Rayon all-target and no_std test/doctest suites
  passed. This compiler emits the unchanged upstream Unsigned visibility
  warning; no warning suppression was introduced.
- Current-stable all-target/all-feature Clippy with warnings denied, formatting
  and rustdoc with warnings denied passed. After the closure-borrow cleanup,
  reran both complete native library suites, MSRV Rayon check and compact plus
  deferred-ownership Miri. The earlier full integration suites were not rerun
  after this single-line cleanup; the affected native movement/scheduler and
  recovery library suites were.
- Focused nightly Miri passed partial insertion, post-movement owned panic,
  SIMD prefix/insertion, ascending confirmation, compact cycles/fault recovery,
  deferred ownership and delayed callback recovery. Miri exercises serialized
  kernel paths; native Rayon progress and simultaneous producer/consumer panic
  tests are not claimed as Miri scheduler coverage. Existing native-only witness
  helpers produce dead-code warnings in Miri.
- **Release-profile** ordinary-path validation is the optimized nine-label
  harness: eight random/d16 smoke cases, 144 corruption/fresh-input/reference
  checks, plus 36 verified warmup and 108 verified full-sized timed outputs.
  No generic full release integration suite is claimed.
- No staging build was run. Hash equality transfers the isolated source
  evidence, but does not substitute for the coordinator's final integration
  checks, documentation review or acceptance decision.

## Promotion status

The shared repository remains on its existing production child-only Rayon
path. Cargo default features remain std-only. This patch is unapplied there.
Applying it would change the Rayon implementation to the strong combined path,
so it must not be described as already promoted. The four-case, three-block
pilot is inconclusive against oldbest and SIMD-off controls. It does not provide
a full distribution/thread matrix or new parallel heap evidence. Keep those
limits explicit in any later decision to land or promote the patch.
