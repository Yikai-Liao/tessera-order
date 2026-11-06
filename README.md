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
  <img src="https://img.shields.io/badge/MSRV-1.85-orange.svg" alt="MSRV 1.85">
  <img src="https://img.shields.io/badge/status-pre--release-lightgrey.svg" alt="Status: pre-release">
</p>

> [!IMPORTANT]
> Tessera is in pre-release. The engine is being ported from its research prototype to stable Rust, and the API shown below is the design for 0.1. Nothing is on crates.io yet; see the [roadmap](#roadmap).

## Why Tessera

An LSD radix sort is stable and runs in linear time, but it needs a second buffer as large as the input. Comparison sorts avoid that buffer and pay O(n log n) comparisons instead. Tessera keeps the radix sort's linear time and stability and bounds the extra memory to O(√n).

- **Stable.** Records with equal keys keep their input order, including under `Reverse` keys.
- **O(√n) auxiliary memory.** Large inputs are split by a stable in-place block partition; every piece that fits a 64 KiB tile is finished in a bounded scratch buffer.
- **Adaptive.** Sorted and reverse-sorted inputs are detected and finished without radix passes, nearly sorted inputs take a budgeted insertion pass, constant key bits are skipped, and heavily repeated keys are split off by sampling.
- **Any record type.** Records do not need `Copy`, `Clone`, `Default` or `Ord`. Values such as `String`, `Box<T>` or `&mut T` are moved, never duplicated, even if the key function misbehaves, and a key function that panics leaves every record in the slice.
- **Wide keys.** All primitive integers up to 128 bits, `f32` and `f64` in `total_cmp` order, and `std::cmp::Reverse<K>` for descending order.
- **No dependencies.** Pure Rust, `std` only.

## Quick start

> The crate is not published yet. This is the planned 0.1 usage.

```toml
[dependencies]
tessera-sort = "0.1"
```

Sort plain keys:

```rust
let mut values: Vec<u64> = vec![42, 7, 19, 7, 3];
tessera_sort::stable_sort(&mut values);
assert_eq!(values, [3, 7, 7, 19, 42]);
```

Sort records by a key; equal keys keep their order:

```rust
use std::cmp::Reverse;

struct Row {
    timestamp: i64,
    name: String,
}

let mut rows: Vec<Row> = load_rows();

// Ascending by timestamp.
tessera_sort::stable_sort_by_key(&mut rows, |row| row.timestamp);

// Descending by timestamp; ties still keep their input order.
tessera_sort::stable_sort_by_key(&mut rows, |row| Reverse(row.timestamp));
```

## API

| Function | Records | Notes |
|---|---|---|
| `stable_sort(&mut [K])` | Supported key types | Sorts the keys themselves |
| `stable_sort_by_key(&mut [T], F)` | Any `T` | Sorts records by a key computed by `F` |
| `Workspace<T>` | Same as above | The same sorts as methods, reusing scratch memory across calls |

`F` is `FnMut(&T) -> K` where `K` is a supported key. The key types are fixed by a sealed `RadixKey` trait, so an unsupported key is a compile error rather than a runtime panic. Tessera does not take a comparator: an arbitrary `Ordering` cannot be turned into radix digits. Use `slice::sort_by` for those orders.

### Untrusted key functions

A key function is ordinary safe code, so Tessera cannot assume it returns the same key every time it is called. `stable_sort_by_key` therefore computes each record's bucket once per pass, stores it as a two-byte number, and moves records by the stored number. Every output slot is written exactly once, so records are never duplicated or lost, whatever the key function does. `stable_sort` needs none of this: its keys are encoded by Tessera itself, so it moves records by recomputing them.

In the research prototype, plain-data (`Copy`) records skipped the stored bucket numbers. Measured against that path on the research prototype (Xeon Gold 6140, paired in one binary with a code-placement control), storing them costs at most about 4.5% and is faster in some cases: 16-byte records are up to 3.5% slower on random keys from 16K records up and 2–3.4% faster on skewed keys at 1K–4K, and 128-byte records show no difference. A single entry point for every record type is worth that cost.

### Reusing scratch memory

Each free function allocates scratch memory at the start of a call and frees it before returning. When you sort many slices of the same record type, a `Workspace<T>` keeps that memory between calls, so only the first sort, or one that needs more room, allocates:

```rust
use tessera_sort::Workspace;

#[derive(Clone, Copy)]
struct Entry {
    key: u64,
    row: u32,
}

let mut ws = Workspace::<Entry>::new();
for batch in batches.iter_mut() {
    ws.stable_sort_by_key(batch, |e| e.key);
}

// A workspace keeps its largest capacity until released.
if ws.retained_bytes() > 4 << 20 {
    ws.shrink_to_fit();
}
```

A workspace keeps capacity only, never records: every call treats scratch as empty, so results never depend on earlier sorts. It is typed by the record, not the key, and serves one sort at a time; use one per thread. More examples, including thread-local reuse, are in [`docs/workspace.md`](docs/workspace.md).

## Ordering semantics

- **Stability.** Equal keys keep their input order.
- **Floats.** `f32` and `f64` follow `total_cmp`: `-0.0` sorts before `+0.0`, and NaNs sort by sign and payload. If your data needs all NaNs equal or both zeros equal, normalize the key in the key function.
- **Key functions.** A key function may be called several times per record, in any order, and should return the same key for the same record during one sort. If it does not, the order is unspecified, but the slice still holds every record exactly once. If it panics, the panic propagates and the slice still holds every record, in an unspecified order.
- **Descending order.** Wrap the key in `Reverse`. Reversing an ascending result would reverse the order of ties as well.

The full rules, including 128-bit and signed encodings, are in [`conformance/semantics.md`](conformance/semantics.md).

## Memory

| Storage | Bound |
|---|---|
| Block partition | O(√n) records for a fixed record size |
| Tile scratch | 64 KiB of records (at least one record), plus one `u16` cursor per bucket |
| Stack | Up to 32 KiB of cursor table per tile-local MSD level. Depth is at most ⌈key bits / 6⌉, which is 22 levels for 128-bit keys. Threads with small stacks must budget for this. |
| `Workspace` | Keeps the largest capacity used so far until `shrink_to_fit` |

## How it works

1. **Probe the order.** A sampled check finds ascending and descending inputs; ascending returns at once and descending is reversed stably. Other inputs get an insertion pass with a swap budget scaled to the input length; it gives up as soon as the budget runs out.
2. **Find the varying bits.** One scan computes which key bits change. Constant bits are skipped, and a constant input returns without moving anything.
3. **Partition large nodes.** A slice larger than one tile is split by a stable block partition on the top digit, on a sampled repeated key, or on an end run separated by the highest varying bit. The block partition reuses consumed input blocks and records their logical order, so it needs O(√n) extra space instead of a full buffer.
4. **Finish tiles.** A slice that fits 64 KiB is finished by insertion sort, a tile-local MSD partition, or LSD counting passes. The choice and the digit widths (6 to 14 bits) come from a cost model of passes and bucket tables.

[`docs/design.md`](docs/design.md) describes each stage and its invariants.

## Performance

These numbers are from the research prototype, not from a release of this crate. They were measured inside two databases' own benchmark programs, where Tessera replaced the existing sort. The ratio is Tessera's time divided by the existing sort's time; **below 1 means Tessera is faster**.

**ClickHouse** `radix_sort` example, `UInt64` keys, against ClickHouse's LSD radix sort (median of 9 paired blocks):

| Rows | 4,096 | 65,536 | 1,000,000 | 10,000,000 |
|---|---:|---:|---:|---:|
| Tessera / LSD radix | 0.971 (within noise) | 0.553 | 0.267 | 0.348 |

**StarRocks** radix sort benchmark from PR #81472, uniformly random keys with a `u32` row index, against the PR's radix sort with pre-allocated scratch. Both sides include the benchmark's input copy and tie detection.

| Key type | 4,096 rows | 65,536 rows | 1,000,000 rows |
|---|---:|---:|---:|
| Int32 | 1.608 | 1.204 | 0.505 |
| Int64 | 0.575 | 0.553 | 0.298 |
| Float | 1.685 | 1.519 | 0.859 |
| Double | 1.352 | 0.720 | 0.467 |

Tessera wins on large batches and on 64-bit keys. On batches of a few thousand 32-bit keys, a radix sort specialized for that size is faster.

Conditions: research commit `6804fcb`; Intel Xeon Gold 6140 under KVM; one pinned core; Rust nightly with `target-cpu=native` and thin LTO; Clang 19 for the C++ side. The setup, A/A noise ranges and raw data are in the [research record](https://github.com/Yikai-Liao/sort-research-rs/tree/6c0877f0491dc3b8fc052aa529bce44bf8a53fa8/results/tessera/public-projects-20261105). Release benchmarks will follow the protocol in [`docs/benchmarking.md`](docs/benchmarking.md).

## When to use something else

- **Orders defined by a comparator**, strings, or other variable-length keys: use `slice::sort_by`.
- **Many small arrays of a few thousand 32-bit keys** in a hot loop: a radix sort tuned for that size, or `slice::sort_unstable`, may be faster.
- **Parallel sorting**: Tessera is single-threaded.
- **`no_std`**: Tessera needs `std` for now.

## Roadmap

- [x] Research prototype, safety validation (Miri, ASan) and external benchmarks
- [ ] Port the engine to stable Rust with the 0.1 API
- [ ] Acceptance benchmark: the port matches the research prototype
- [ ] Benchmark runner and the first published result matrix
- [ ] Release 0.1 on crates.io
- [ ] Native C++17 header-only implementation sharing the same semantics and test vectors
- [ ] A dedicated unstable kernel

## Repository layout

```text
rust/tessera-sort/   the Rust crate
conformance/         ordering semantics shared by all implementations
bench/               benchmark runners and reports
docs/                design and benchmarking documentation
assets/              logo
```

## Contributing

Bug reports, benchmark results from other machines, and integration reports are welcome, and so are AI-assisted contributions, as long as a person has reviewed the change and takes responsibility for it. Please read [`CONTRIBUTING.md`](CONTRIBUTING.md) first. Commits are signed off under the [Developer Certificate of Origin](https://developercertificate.org/), and everyone in the project follows the [code of conduct](CODE_OF_CONDUCT.md). Report security issues privately as described in [`SECURITY.md`](SECURITY.md).

## Acknowledgements

The stable block partition is adapted from Robert Clausecker's [`radixsort_permuted.c`](https://github.com/clausecker/radsort/blob/f69e816c3cd79d312cd67aea5b9cf1c338c1b371/radixsort_permuted.c), described in [*Parallel O(√n) Overhead LSD Radix Sort*](https://arxiv.org/abs/2607.05302) by Robert Clausecker and Florian Schintke.

Tessera was developed and measured with Lukas Bergdoll's [sort-research-rs](https://github.com/Voultapher/sort-research-rs) benchmark harness.

## License

Tessera is licensed under the [BSD 2-Clause License](LICENSE). This covers the code adapted from `radixsort_permuted.c`, which is distributed under the same license.
