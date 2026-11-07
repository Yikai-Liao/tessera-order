# Changelog

All notable changes to this project are documented in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Rust crate `tessera-sort`, ported to Stable Rust from the research prototype at `sort-research-rs` commit `6c0877f`: `stable_sort`, `stable_sort_by_key`, the sealed `RadixKey` trait (primitive integers, `f32`, `f64`, `Reverse<K>`) and `Workspace<T>`.
- Tests for ordering, thresholds, panicking and inconsistent key functions, owned and `&mut` records, and workspace allocation; CI runs them with Miri, AddressSanitizer and the MSRV.
- `no_std` support through a default-on `std` feature: `default-features = false` builds on `core` and `alloc`, with an unwind guard that aborts on key-function panics during block partitioning ([decision 0005](docs/decisions/0005-no-std-with-alloc.md), [decision 0006](docs/decisions/0006-no-std-unwind-guard.md)).
- Port record with the acceptance measurements against the prototype (`docs/design/port.md`), roadmap, decision records and the Python binding proposal.
- Per-language CI and release workflows, selected by changed paths; C++ and Python stay inactive until their projects exist. Release rules in `RELEASING.md`.
- `rustfmt.toml` and workspace lints for documentation and unsafe code.
- Repository structure, ordering semantics, design, benchmarking and workspace documentation.
- Contribution policy: DCO sign-off checked in CI, AI-assisted contribution rules, code of conduct, and instructions for coding agents.

### Changed

- Reuse the optional SIMD scan to confirm ascending primitive keys after the order probe.
- Use pair-window iteration for scalar inversion scans while retaining one key evaluation per scanned record.
- Give insertion finishing a unit-returning entry point shared by both backends, and skip portable SIMD dispatch when the existing scan gate guarantees scalar execution.
- Derive the block-partition block size from the scratch-versus-tables trade-off instead of a tuned multiple of it, and keep the per-block copy and bookkeeping amortised with a floor in bytes and one in records. The removed factor sat at 2.1 times the minimum auxiliary memory of the scheme; the asymptotic bound is unchanged.
