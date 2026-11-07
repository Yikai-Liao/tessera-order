<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Integration validation

Validated locally against `f59a424` on the experimental branch. The repository's
Rust core is unchanged from `60873c8`; latest upstream main is `52dab6a`.

- Debug and release: eight integration tests passed in each configuration.
- Rust 1.85: eight integration tests passed.
- Clippy with warnings denied: passed.
- Doc tests: passed, zero examples.
- Default Stacked Borrows Miri with strict provenance: two focused tests passed.
- Source, tests, manifest and Cargo.lock are byte-identical to the originating
  serial checkpoint. No parallel execution or performance claim is made.

All commands use the explicit standalone manifest, cached toolchains, offline
Cargo dependencies, and one build job. Shared CPU locking prevents overlap with
exclusive performance measurement. Reproduction commands are in README.md.

The external complete validation log has SHA-256 `0be50f87d8b052ddde73560f819a4fe939bc7906ca686817a64b8e13ac0f7a68`.
Raw logs and build products are intentionally not committed.
