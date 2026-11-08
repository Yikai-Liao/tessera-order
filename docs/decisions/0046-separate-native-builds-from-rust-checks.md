# 0046: Separate native builds from Rust checks

Date: 2026-11-08. Status: accepted.

## Context

The benchmark's `compare-native` feature enables Rust FFI adapters and builds their pinned C/C++ implementations in `build.rs`. Workspace-wide Clippy and rustdoc with `--all-features` enable this feature too, although neither command links the benchmark executables. They therefore compile native competitors in separate Cargo build directories before checking Rust code. The comparison job already builds, links and exercises those competitors.

Caching workspace build products could reduce repeated compilation on a cache hit, but cold caches and invalidation would still make static Rust checks build foreign libraries. Excluding the benchmark or its native feature would avoid the work while losing Rust check coverage.

## Decision

Keep all workspace features enabled in both Clippy and rustdoc. Set `TESSERA_BENCH_CHECK_ONLY=1` only for these non-linking commands. In this mode the benchmark build script omits native compilation, native compiler provenance queries and native linker inputs, while Cargo still checks the native-facing Rust code. Track the variable with `cargo:rerun-if-env-changed` so changing modes reruns the build script.

Remove native tool installation and source fetching from the Rust lint job. Leave the comparison job in normal build mode, with its native tests, release build, sequential-linkage assertions, smoke validation and report generation unchanged. Do not change cache policy.

## Consequences

Static Rust checks need neither downloaded native sources nor a native compiler, even with a cold Cargo cache. Native compilation and runtime validation remain the comparison job's responsibility; Clippy and rustdoc retain their Rust feature coverage.

The check-only variable must be unset for commands that link native competitors. It is an explicit build-script mode rather than an inferred Clippy or rustdoc environment, so local static checks can use the same contract as CI.
