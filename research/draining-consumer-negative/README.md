<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Rejected bounded-draining scheduler experiment

This is a source-only research archive, not a runtime integration or adoption recommendation. The fixed screen found no accepted scheduler gain. Stop this candidate without parameter tuning or further algorithm work.

## Reproducible source chain

The public base is commit `55b299b3716cd1af9e997f30b1a3c1813455d8be` in [tessera-sort](https://github.com/Yikai-Liao/tessera-sort). Apply these repository-relative patches in order to a disposable checkout of that commit:

1. `patches/01-public-to-combined.patch` recreates the previously evaluated current-core/native-root combined port.
2. `patches/02-combined-to-draining.patch` changes only its production consumer scheduling and adds scheduler-specific tests.

`source-map.json` contains SHA256 maps of every Rust source/test file at all three stages, plus patch and public manifest hashes. There are 18 public, 25 combined, and 26 draining Rust files. The second patch changes exactly one inherited production file, `rust/tessera-sort/src/engine/parallel.rs`, and adds `engine/parallel/draining_tests.rs`. Root/scatter/permutation/compact/Extract/core bytes do not change between combined and draining.

Run `python3 verify.py PATH_TO_PUBLIC_REPOSITORY --output verify-result.json` to independently export the declared commit into temporary storage, check and apply both patches, compare every Rust hash after each stage, and verify that public manifests remain unchanged. The supplied verification report records the completed source-only replay. This verifier does not modify the original repository and does not build, test, benchmark, or fetch anything. Its disposable partial export contains the crate and checked root metadata, not an entire buildable public workspace.

## Only packaging normalizations

The private source wrappers used crate name `generic-combined`, an isolated root workspace, and a shortened private lockfile. Those private workspace wrappers are not copied. Both archival stages retain the public root `Cargo.toml` and `Cargo.lock` unchanged. Normalizing only the crate package name to `tessera-sort` makes its crate manifest exactly equal to the public manifest, so neither patch needs a manifest edit. Integration-test imports consequently use the public crate name without modifying their Rust bytes. No Rust source bytes are normalized. A separate lockfile comparison confirmed that every private package/version/checksum/dependency entry equals an entry in the public lock after this package-name normalization; the public lock additionally contains the other workspace/benchmark packages.

This public-workspace normalization was verified only by apply/hash checks, not by a new compilation or integration suite. Prior validation was performed on the private wrapper and is scoped accordingly in `VALIDATION.md`.

## Contents and exclusions

`DESIGN.md` describes ownership, workspace accounting, progress, and panic behavior. `VALIDATION.md` states exact validation scope and artifact-preservation limits. `SCREEN.md` summarizes the negative fixed screen without raw repetitions. `PROVENANCE.json` records the small evidence identities needed to connect this archive to retained external evidence.

Original BSD-2-Clause source/test notices are preserved and `LICENSE` is included. This packet contains no timing harness, raw CSV/JSONL traces, machine-specific paths, targets, executables, or raw performance data. Original evidence remains outside the packet, unchanged. No commit, push, or runtime merge is performed by this archive.

## Inherited research controls

Exact old/new Rust bytes retain preexisting `TESSERA_COMPACT_STATS`, `TESSERA_COMPACT_SERIAL`, root-digit and batch-size environment controls, including the baseline compact-statistics reporting statements. These are explicitly inherited research controls, not newly added instrumentation; the executed screen cleared all `TESSERA_*` variables. Original test-only phase, ownership, and bounded-timeout witnesses also remain. No later profile/depth trace clone or separate diagnostic/timing harness is copied. Stripping inherited controls would create an untested runtime layer, so none were removed.
