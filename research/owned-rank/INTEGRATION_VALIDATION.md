<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Integration validation

Validated locally against `f59a424` on the experimental branch. The repository's
Rust core is unchanged from `60873c8`; latest upstream main is `52dab6a`.

- Release library tests: 12 groups passed.
- Rust 1.85 library tests: 12 groups passed.
- Default Stacked Borrows Miri `owned_miri` filter: three focused targets passed.
- The inherited unused `restore_pull` helper warning remains. Rust 1.85 also
  reports an upstream `unreachable_pub` warning. This is not warning-clean.
- Rust source and Cargo.lock bytes exactly match the staged validated source;
  only the documented dependency-path change and checkpoint documentation differ.
- No owned performance result follows from these checks.

All commands use the explicit standalone manifest, cached toolchains, offline
Cargo dependencies, and one build job. Shared CPU locking prevents overlap with
exclusive performance measurement. Reproduction commands are in README.md.

The external complete validation log has SHA-256 `0be50f87d8b052ddde73560f819a4fe939bc7906ca686817a64b8e13ac0f7a68`.
Raw logs and build products are intentionally not committed.
