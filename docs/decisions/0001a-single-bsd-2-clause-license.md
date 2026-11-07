# 0001a: One BSD-2-Clause license for the whole repository

Date: 2026-11-06. Status: accepted.

## Context

The block partition in `rust/tessera-sort/src/block.rs`, and any later C++ version of it, is derived from Robert Clausecker's BSD-2-Clause `radixsort_permuted.c` and must keep his copyright and license notice. Tessera was developed in `sort-research-rs`, whose harness, test tools and input patterns are Lukas Bergdoll's Apache-2.0 code. A primary license other than BSD-2-Clause would make the repository mixed-license.

## Decision

The repository uses BSD-2-Clause only. `LICENSE` names both copyright holders, every source file starts with `// SPDX-License-Identifier: BSD-2-Clause`, and `block.rs` keeps the full upstream notice and revision link.

## Consequences

- Code from `sort-research-rs` other than Tessera's own engine and key code is not copied. Tests and benchmarks are rewritten.
- Third-party sorts used as baselines enter as crates.io dependencies (Rust) or are fetched at pinned commits during the build (C++). Only our own adapters are committed.
- Benchmarks that embed upstream projects, such as StarRocks and ClickHouse, live outside this repository and are linked from the README.
- The crate is published as `tessera-sort` (`tessera` is taken on crates.io).
