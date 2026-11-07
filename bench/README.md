# Benchmarks

The [comparison runner](../docs/development/benchmarking.md) measures Tessera against Rust, C and C++ sorting algorithms with paired blocks, A/A controls and separate allocation measurement. Its offline report generates release figures, tables and optional Bencher summaries. The [continuous benchmarks](../docs/development/continuous-benchmarks.md) use Criterion and the same deterministic input generator for regression tracking.

```text
bench/rust/src/         shared inputs and comparison runner
bench/rust/benches/     Criterion regression suite
bench/rust/native/      repository-owned shims and pinned source manifest
bench/fetch.py          native source acquisition and verification
bench/report/           TypeScript report and synthetic chart fixtures
bench/vendor/           ignored third-party source trees
bench/results/          ignored raw runs and generated local reports
```

```sh
python3 bench/fetch.py
cargo build --release -p tessera-bench --features compare,compare-native
target/release/compare validate --suite smoke
target/release/compare run --suite smoke --out bench/results/smoke
npm ci
npm run bench:report -- bench/results/smoke
```

Use the full release procedure in the protocol for published comparisons. `npm run bench:test` and `npm run bench:typecheck` check reporting; `cargo test -p tessera-bench --features compare --lib --bin memory` checks the harness without native compilers. Third-party sources and raw measurements are not committed.
