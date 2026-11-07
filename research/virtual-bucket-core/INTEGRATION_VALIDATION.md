<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Integration validation

Validated locally against `f59a424` on the experimental branch. The repository's
Rust core is unchanged from `60873c8`; latest upstream main is `52dab6a`.

- Debug and release: eight integration tests passed in each configuration.
- Rust 1.85: eight integration tests passed.
- Clippy with warnings denied: passed.
- Doc tests: passed, zero examples.
- Default Stacked Borrows Miri with strict provenance: two focused tests passed.
- At initial packaging, source, tests, manifest and Cargo.lock were byte-identical
  to the originating serial checkpoint. The follow-up below changes shard-index
  arithmetic. No parallel execution or performance claim is made.

All commands use the explicit standalone manifest, cached toolchains, offline
Cargo dependencies, and one build job. Shared CPU locking prevents overlap with
exclusive performance measurement. Reproduction commands are in README.md.

The external complete validation log has SHA-256 `0be50f87d8b052ddde73560f819a4fe939bc7906ca686817a64b8e13ac0f7a68`.
Raw logs and build products are intentionally not committed.

## Shard-index overflow follow-up

Review found that `block * shards / blocks` could overflow `usize`, assigning
later input blocks to an earlier shard and breaking stability. A concrete 32-bit
case uses 65,537 one-record blocks and 65,537 root shards. The mapper now widens
the multiplication to u128 before division. Supported pointer widths fit that
product; `block < blocks` bounds the result below `shards`.

Release and Rust 1.85 now pass one full-width arithmetic test plus the eight
integration tests. Warnings-denied Clippy passes. Default Stacked Borrows Miri
with strict provenance passes the arithmetic regression and the two existing
integration smoke fixtures. The arithmetic test exercises usize::MAX operands
without allocating enormous inputs. No native 32-bit runtime was executed.

Follow-up external log SHA-256: `55e127a967e0242b3fcc96883b49d95e545999d19e354492750a4c688c1de43b`.
