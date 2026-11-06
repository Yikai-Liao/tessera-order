# Benchmarks

The release runner lives here once it is built; its protocol is in [`docs/development/benchmarking.md`](../docs/development/benchmarking.md). The [continuous benchmarks](../docs/development/continuous-benchmarks.md) in `rust/benches/smoke.rs` run on every PR that changes Rust code.

```text
bench/rust/     Rust runner (unpublished crate)
bench/report/   report generator (Python standard library only)
bench/cases/    suite definitions
```

Third-party sort implementations are pulled as crates.io dependencies or fetched at pinned commits during the build. Their sources are not committed to this repository.
