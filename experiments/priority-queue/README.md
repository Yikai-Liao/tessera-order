# Priority-queue experiment

A frozen, standalone research snapshot, not a library API or the maintained sorting benchmark system. This project is outside the root Cargo workspace, is not published, and adds no dependency to the sorting library. Third-party heaps remain a pinned Cargo dependency; no upstream source is copied here. Raw measurements, executables and build artifacts stay outside Git.

The [research evidence](../../docs/design/evidence.md#priority-queue-feasibility-screen) owns the measured conclusions, losses and complexity analysis. The [roadmap](../../ROADMAP.md#benchmark-tooling-and-investigations) owns acceptance before any production proposal. The range prototype has a known repeated-equal-front rescan sequence that defeats a general logarithmic amortized-update claim. Do not treat the favorable measured traces as a complexity guarantee.

The imported algorithms and trace generator are the final external study snapshot. The import changes only project integration and allocator safety comments; a rebuild at a new source path is not new timing evidence. Exploratory variants and intentionally duplicated ablation sources are retained to preserve that snapshot, not as a proposed maintained architecture. No additional performance experiment was run when this branch was packaged.

## Scope and source

Every queue implements exact scalar `u64` min-priority behavior for arbitrary inserts. Equal values are indistinguishable: neither FIFO ties, generic payload ownership nor unwind recovery is established. The sort library's public stability and square-root scratch contracts do not transfer to these containers.

| Source | Purpose |
|---|---|
| [main.rs](src/main.rs) | Queue adapters, algorithm/trace selection, differential tests, measurement boundaries and CSV output |
| [range.rs](src/range.rs), [split.rs](src/split.rs) | Minimum live range, reusable blocks, on-demand digit/equality partitioning and bounded pending inserts |
| [buffered_runs.rs](src/buffered_runs.rs), [insert_buffer.rs](src/insert_buffer.rs) | Tessera-sorted runs, a run-head tournament and insert buffering that heapifies on demand |
| [alloc.rs](src/alloc.rs) | Requested allocation accounting, enabled only by the memory feature |

The other modules preserve the old implementations and the eager-routing, leaf-sort and run-management ablations. Algorithm labels and trace generation are defined in main.rs; there is no integration with the maintained sort-only CLI or benchmark result format.

## Validation

From the repository root:

```sh
cargo fmt --manifest-path experiments/priority-queue/Cargo.toml -- --check
cargo clippy --manifest-path experiments/priority-queue/Cargo.toml --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path experiments/priority-queue/Cargo.toml --release --locked
cargo test --manifest-path experiments/priority-queue/Cargo.toml --release --locked --features memory
```

Tests cover random interleaved operations, duplicate/extreme keys, capacity boundaries, range reopening and buffer refills against a reference heap. They do not establish asymptotic bounds or the unimplemented payload semantics.

## Running a trace

The binary accepts five positional arguments:

1. Initial scale n (default 4,096; actual occupancy depends on the trace).
2. Repetition count (default 3).
3. time or latency (default time).
4. Comma-separated algorithm labels (default all snapshot variants).
5. Comma-separated trace labels (default all snapshot traces).

Four fixed seeds run for each requested trace. Every requested output and the eventual remaining multiset are checked against a reference outside the measurement snapshots. A normal time process supplies whole-trace throughput. A latency process clocks pushes and pops individually, including timer overhead. Allocation accounting needs an independent build with --features memory; do not report its elapsed times as ordinary throughput. Peak and retained byte columns include queue records, metadata, retained capacity and sorting scratch, not just overhead; they are requested bytes, not RSS.

For a bounded manual run, build without the memory feature and retain stdout outside Git:

```sh
cargo build --manifest-path experiments/priority-queue/Cargo.toml --release --locked
experiments/priority-queue/target/release/tessera-pq-optimization \
  4096 3 time heap,heap_reserved,dary4,dary8,range8192,adaptive2048,adaptive8192,heap_aa \
  drain,mixed,top1
```

Do not silently broaden this selection into a large experiment. Choose an allowed CPU affinity for the host, retain compiler/build flags and provenance, separate allocation/latency passes, and apply the [measurement controls](../../docs/development/benchmarking.md#run-an-experiment). This lightweight runner is a prototype: it supplies no statistical verdict, independent placement control, application-level workload or mature SIMD-queue opponent.
