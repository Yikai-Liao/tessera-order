<p align="center">
  <img src="https://raw.githubusercontent.com/Yikai-Liao/tessera-sort/main/assets/logo.svg" width="144" alt="Tessera logo: a mosaic rosette">
</p>

<h1 align="center">Tessera</h1>

<p align="center">
  <strong>Stable radix sort with O(√n) auxiliary memory.</strong><br>
  Linear time, adaptive to real data, and safe for records that own resources.
</p>

<p align="center">
  <a href="https://github.com/Yikai-Liao/tessera-sort/actions/workflows/ci.yml"><img src="https://github.com/Yikai-Liao/tessera-sort/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-BSD--2--Clause-blue.svg" alt="License: BSD-2-Clause"></a>
  <img src="https://img.shields.io/badge/MSRV-1.89-orange.svg" alt="MSRV 1.89">
  <img src="https://img.shields.io/badge/status-pre--release-lightgrey.svg" alt="Status: pre-release">
  <a href="https://yikai-liao.github.io/tessera-sort/"><img src="https://img.shields.io/badge/docs-online-8b5cf6.svg" alt="Documentation"></a>
</p>

> [!IMPORTANT]
> Tessera is in pre-release. The Rust engine has been ported to Stable Rust and checked against the research prototype. Nothing is on crates.io yet; see the [roadmap](ROADMAP.md).

## Why Tessera

An LSD radix sort is stable and runs in linear time, but it needs a second buffer as large as the input. Comparison sorts avoid that buffer and pay O(n log n) comparisons instead. Tessera keeps the radix sort's linear time and stability and bounds the extra memory to O(√n).

- **Stable**, also for descending keys.
- **O(√n) auxiliary memory**, through a stable in-place block partition and a bounded tile buffer.
- **Adaptive.** Sorted, reverse-sorted, nearly sorted and heavily repeated inputs take shortcuts, and constant key bits are skipped.
- **Any record type.** No `Copy`, `Clone`, `Default` or `Ord` needed. Records are moved, never duplicated, even if the key function misbehaves or panics.
- **Wide keys.** Integers up to 128 bits, `f32` and `f64` in `total_cmp` order, and `Reverse<K>`.
- **One portable backend.** Uses `fearless_simd` for vector scans and prefix sums, selecting supported instructions automatically. Requires `std` and Rust 1.89; Rayon is optional.

## Quick start

> The crate is not published yet. The example uses the implemented 0.1 API.

```toml
[dependencies]
tessera-sort = "0.1"
```

SIMD is always enabled; there is no `portable-simd` or `std` feature. Disabling default features does not change the backend or remove the `std` requirement.

```rust
use std::cmp::Reverse;

let mut values: Vec<u64> = vec![42, 7, 19, 7, 3];
tessera_sort::stable_sort(&mut values);
assert_eq!(values, [3, 7, 7, 19, 42]);

// Sort records by a key; equal keys keep their input order.
let mut rows = vec![(2u32, "first"), (1, "middle"), (2, "last")];
tessera_sort::stable_sort_by_key(&mut rows, |row| Reverse(row.0));
assert_eq!(rows, [(2, "first"), (2, "last"), (1, "middle")]);
```

| API | Use |
|---|---|
| `stable_sort(&mut [K])` | Sort keys themselves |
| `stable_sort_by_key(&mut [T], F)` | Sort any records by a key computed by `F` |

Tessera does not take a comparator: an arbitrary `Ordering` cannot be turned into radix digits.

The optional `rayon` feature adds `par_stable_sort` and `par_stable_sort_by_key`, requiring `Send` records and a shared `Fn + Sync` key function. Large inputs scatter disjoint root shards in the caller's Rayon pool. Bounded buckets sort in staging before protected final writes; other layouts use block permutation and compaction before completing the buckets. Small inputs and one-thread pools use the serial engine. See [parallel usage](docs/guide/getting-started.md#parallel-sorting) and the [parallel contract](conformance/semantics.md#rust-rayon-entry-points).

## Performance

Track Rust benchmark trends on the [public Bencher dashboard](https://bencher.dev/perf/yikai-liao-s-project). These continuous measurements flag changes to investigate; use the [benchmark protocol](docs/development/benchmarking.md#command-capabilities) for paired algorithm comparisons and source-change attribution.

> [!NOTE]
> The first complete release comparison remains on the [roadmap](ROADMAP.md). The preview below tracks Tessera on main, not an algorithm ranking.

Main trend: the geometric mean of each mode's run-time change since the first run on the current runner specification; lower is faster. The [interactive page](https://yikai-liao.github.io/tessera-sort/benchmarks/) shows every case, absolute times and selectable time windows.

[![Main performance trend](https://raw.githubusercontent.com/Yikai-Liao/tessera-sort/benchmark-data/main/preview.svg)](https://yikai-liao.github.io/tessera-sort/benchmarks/)

## When to use something else

- **Orders defined by a comparator**, strings, or other variable-length keys: use `slice::sort_by`.
- **Parallel sorting without the optional `rayon` feature**: Tessera is single-threaded.
- **No allocator**: Tessera needs O(√n) scratch memory, so it cannot run without `alloc` (or, in C++, without a heap).

## Status by language

Each language package is versioned and released separately ([`RELEASING.md`](RELEASING.md)). No package has been released yet.

| Capability | Rust (unreleased) | C++ |
|---|---|---|
| Stable sort | Implemented | Planned |
| Unstable sort | Not in 0.1 | Planned |
| 128-bit keys | Signed and unsigned | Planned |
| Records that own resources or hold `&mut` | Supported | Trivially copyable records only (planned) |

A Python binding is planned as well; see the [roadmap](ROADMAP.md).

## Learn more

| Topic | Document |
|---|---|
| Ordering, key functions, floats, panics and memory bounds | [`conformance/semantics.md`](conformance/semantics.md) |
| How the algorithm works, with interactive figures | [Algorithm explanation](https://yikai-liao.github.io/tessera-sort/explain/) |
| How the engine works | [`docs/design/engine.md`](docs/design/engine.md) |
| Why user key functions cache bucket IDs | [Record handling](docs/design/engine.md#record-handling) |
| All documentation | [Documentation site](https://yikai-liao.github.io/tessera-sort/) · [Source index](docs/README.md) |

## Contributing

Bug reports, benchmark results from other machines, and integration reports are welcome, and so are AI-assisted contributions, as long as a person has reviewed the change and takes responsibility for it. Please read [`CONTRIBUTING.md`](CONTRIBUTING.md) first. Commits are signed off under the [Developer Certificate of Origin](https://developercertificate.org/), and everyone in the project follows the [code of conduct](CODE_OF_CONDUCT.md). Report security issues privately as described in [`SECURITY.md`](SECURITY.md).

## Acknowledgements

The stable block partition is adapted from Robert Clausecker's [`radixsort_permuted.c`](https://github.com/clausecker/radsort/blob/f69e816c3cd79d312cd67aea5b9cf1c338c1b371/radixsort_permuted.c), described in [*Parallel O(√n) Overhead LSD Radix Sort*](https://arxiv.org/abs/2607.05302) by Robert Clausecker and Florian Schintke.

Tessera was developed and measured with Lukas Bergdoll's [sort-research-rs](https://github.com/Voultapher/sort-research-rs) benchmark harness.

## License

Tessera is licensed under the [BSD 2-Clause License](LICENSE). This covers the code adapted from `radixsort_permuted.c`, which is distributed under the same license.
