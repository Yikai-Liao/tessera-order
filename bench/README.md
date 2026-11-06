# Benchmarks

The benchmark runner lives here once it is built. The protocol it implements is in [`docs/development/benchmarking.md`](../docs/development/benchmarking.md).

```text
bench/rust/     Rust runner (unpublished crate)
bench/report/   report generator (Python standard library only)
bench/cases/    suite definitions
```

Third-party sort implementations are pulled as crates.io dependencies or fetched at pinned commits during the build. Their sources are not committed to this repository.
