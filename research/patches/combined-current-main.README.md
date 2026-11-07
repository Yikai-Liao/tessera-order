# Research combined control on frozen current main

This checkpoint preserves an already validated research scheduler on newer upstream kernels. It adds no new sorting algorithm, makes no performance-improvement claim, and does not replace the public parallel entry.

## Identities

- Exact patch base: pure upstream main `0bc80b49d5efd5bd8b62165b170015e5803226b7`.
- Validated 25-file candidate identity: `305068d9d6f4b1d0621c18426eee4945c8b45514f4b89037a276ed60fef37d70`.
- Patch SHA-256: `0d375cfb6f962fb6032c2a962833d9d003d15951403e220b69a84e0b1e0f6b7d`.
- Previous validated research lineage: the combined port on `5b13ac1`, source identity `9586fc287e595da719ec8c958f3e428254459599b3ba2306ffe1e234058d76b8`.
- Public comparison reference: `d6f303b74a0f19b6ae3c14a781539e3c39b9c4c0`, including the separate sparse-root correctness fix. This patch is not based on that public overlay.

The source manifest hashes every crate source/test file and the three Cargo metadata files. Its identity is SHA-256 of sorted lines `file_sha256 + two spaces + relative_path + newline`. Apply and reverse checks passed against a fresh archive of the exact pure-main base; all 25 applied files matched the manifest, and reversal restored every file, symlink kind and file mode in the complete 137-file archive.

## Reproduce in an isolated tree

Do not apply this research patch over an existing public or experimental overlay. From a checkout containing the base commit:

```sh
PATCH="$(pwd)/research/patches/combined-current-main.patch"
WORK="$(mktemp -d)"
git archive 0bc80b49d5efd5bd8b62165b170015e5803226b7 | tar -x -C "$WORK"
(cd "$WORK" && git apply --check "$PATCH" && git apply "$PATCH")
(cd "$WORK" && cargo test --release --all-features --all-targets)
(cd "$WORK" && cargo test --release --all-features --doc)
```

This recreates the private crate-only workspace that was validated. The root workspace intentionally omits `bench/rust`; the package name is `tessera-combined-current-main`, with the library name `tessera_sort`. The private lockfile therefore differs from the standard upstream benchmark workspace. The patch does not delete upstream benchmark files, but those files are not workspace members in this control.

## What changed from the previous research control

The shared engine takes main's default block-size multiplier of four, narrow Direct-window counting, KeyHalf forwarding and new scatter/callback panic tests. Scan, Portable SIMD, workspace and unwind kernels remain exact upstream files.

The research scheduler remains native parallel root scatter, parallel whole-block permutation, original serial squeeze, and fixed std-yield consumers. Its original native block geometry is not multiplied by four. The candidate does not include map elision, sleep waiting, deferred consumers, halo squeeze, indirect leaves or the later Regions prototype.

Two small adapter changes keep the research path aligned with the new Direct optimization:

1. Owned parallel `Narrow<E, SHIFT>` forwards the count window by `low + SHIFT`. Preflight has already established that the other encoded half is constant.
2. A single-digit native root tries that same count hook before scatter. Direct keys may use it; ByKey declines and preserves payload-bearing native partitioning.

Published public `d6f303b` intentionally does not contain the owned Narrow forwarding. That difference is explicit, not hidden behind a claim that the two schedulers have identical source or routes. The candidate also retains its existing segmented scratch and native/test-only witnesses.

## Geometry and validation

The existing untimed diagnostic ran against complete stable-reference outputs at random 10M records, W8, with `rayon,portable-simd` and every `TESSERA_*` variable cleared:

- u64: radix 256, block size 128 records, 8 shards.
- Row16: radix 256, block size 64 records, 8 shards.

Both agree with the previous research geometry and the unchanged native balanced-root formula. Serial and child block partitions use main's separate four-times-balanced default. These observations are not timings or heap measurements.

See `combined-current-main.VALIDATION.md` for exact gates and historical failures/interruption handling. All required host safety, compatibility, formatting and documentation gates completed. Focused Miri does not validate actual Rayon scheduling. Portable SIMD requires Rust 1.89 or newer; it is not claimed on MSRV1.85.

For comparisons, match the chosen features, toolchain and target flags across every control. The SIMD-on configuration is `rayon,portable-simd`; scalar Rayon is also validated. Clear all `TESSERA_*`, especially root-bit cap, batch-size and compact overrides. No timing or new heap experiment is included in this checkpoint.

The packet contains source, hashes, source-only apply/reverse checks and concise validation/geometry metadata. It excludes build output, dependency/vendor trees, raw logs and benchmark data.
