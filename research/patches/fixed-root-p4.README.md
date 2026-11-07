<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Fixed root geometry P4

**Not adopted.** This is an independent source-only causal-control checkpoint, not a production recommendation.

## Exact independent apply chain

1. Export repository commit `b1d36ce4e2b547ba72a0026dfb5850389e0d45e7`.
2. Check/apply existing `research/patches/combined-main-5b13.patch`, SHA-256 `0c75b051977dab392c66dbfee70a7f32885484ca143bc15611e819bb5bf15ba3`.
3. Check/apply `fixed-root-p4.patch`, SHA-256 `1ef6f148bef4cf1c2c3e8f480268a53ddef54c9136a3127fa52356efb722980e`.

Do not stack this with either other causal-control patch. Do not apply it as a drop-in to public `0e50926e873b014de1d37a67a6da66da368010d0`. The prerequisite is the historical strongest combined research identity `9586fc287e595da719ec8c958f3e428254459599b3ba2306ffe1e234058d76b8`, not the later public scheduler.

## Contents and verification

The patch includes all 1 changed runtime/test Rust files. All 20 resulting runtime/test files are byte-identical to validated private source `tessera-fixed-root-diagnostic/sources/fixed`. Standard historical package `tessera-sort` and implicit library `tessera_sort` are preserved; private package renaming, reduced private workspace membership and private generated lock identities are not copied. Root Cargo.toml, Cargo.lock and crate Cargo.toml remain prerequisite bytes. No runtime/test byte is rewritten for archival packaging.

- `fixed-root-p4.base-hashes.txt`, `fixed-root-p4.source-hashes.txt`: historical prerequisite and archived target hashes.
- `fixed-root-p4.validated-private-hashes.txt`: separately identified private source/manifests; identity differences are intentional, not conflated.
- `fixed-root-p4.SOURCE-LINEAGE.json`, `fixed-root-p4.apply-check.txt`: full independent apply, entire-tree/hash and reverse checks.
- [Space and progress](causal-controls.SPACE-AND-PROGRESS.md), [exact validation scope](causal-controls.VALIDATION.md), [aggregate findings](causal-controls.SUMMARY.md).
- `causal-controls.external-evidence.json`: hashes of external reports/logs/raw evidence; those files are not included.

No raw samples, events, harness, target directory or binary is in this packet. Archival staging ran no cargo/build/test/timing, commit or push and did not modify a shared repository. Only lightweight source application and hash/reverse checks were performed.
