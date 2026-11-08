<!-- SPDX-License-Identifier: BSD-2-Clause -->

# Proposal: Rayon parallel sorting

Status: experimental Rayon API with root-sharded scatter, parallel block permutation and overlapped bucket sorting implemented. Broad acceptance measurements are pending. Date: 2026-11-08.

Add parallel stable radix sorting as an optional capability of the existing Rust crate. The implementation reuses the engine's probes, digit planner, block partition and tile/node sorting. Large roots are scattered across disjoint shards, their stable block directories are adopted in original shard order, and complete child buckets are sorted in bounded Rayon jobs while blocks are materialized. This is an experimental root-parallel stage; broad acceptance measurements are still required.

## Public API

Enable `tessera-sort`'s `rayon` Cargo feature. It enables `std` and the optional Rayon dependency, including with `default-features = false`, and keeps the crate's Rust 1.85 MSRV. Default and `no_std` consumers acquire no required dependency.

```rust
pub fn par_stable_sort<K: RadixKey + Send>(values: &mut [K]);

pub fn par_stable_sort_by_key<T, K, F>(values: &mut [T], key: F)
where
    T: Send,
    K: RadixKey,
    F: Fn(&T) -> K + Sync;
```

Export both functions at the crate root, beside `stable_sort` and `stable_sort_by_key`. Use the `par_` prefix to make the execution choice explicit at each call. Keep the existing serial entry points and their `FnMut` key functions; enabling a Cargo feature must not silently add threading bounds or change their execution policy.

Tasks will own disjoint mutable record slices, so records need `Send`, not `Sync`. Tasks share a borrow of the key function, so it needs `Fn + Sync`, not `Send` or `Clone`. Encoded keys remain local to each task. This follows the ownership model of [Rayon's by-key slice sort](https://docs.rs/rayon/1.12.0/rayon/slice/trait.ParallelSliceMut.html#method.par_sort_by_key). Do not widen these bounds to simplify an internal implementation.

Keep thread-pool selection with the caller. Once scheduling is implemented, use Rayon's current pool, including a pool selected by [`ThreadPool::install`](https://docs.rs/rayon/1.12.0/rayon/struct.ThreadPool.html#method.install); outside a pool, use Rayon's global pool. Do not create a pool per sort or add a thread-count parameter. Callers can already compose the entry point with a custom pool:

```rust
let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
let mut keys = vec![3u64, 1, 2];
pool.install(|| tessera_sort::par_stable_sort(&mut keys));
assert_eq!(keys, [1, 2, 3]);
```

Ordering, stability, inconsistent-key safety and record preservation follow the [shared semantics](../../conformance/semantics.md). Key calls may overlap across tasks, and neither call order nor a particular worker assignment is promised. If a task panics, all tasks that may access the input must finish their restoration before the panic reaches the caller. Rayon [`join`](https://docs.rs/rayon/1.12.0/rayon/fn.join.html) waits for both branches even if one panics; every sorting task must also preserve its own records. Which panic wins when several tasks panic is unspecified.

Keep parallel scratch internal. Retained capacity, suspended tasks and scratch distribution need a measured design before a reusable parallel workspace becomes public.

## Source boundaries and reuse

| File | Responsibility |
|---|---|
| `src/lib.rs` | Crate documentation, feature gates and root exports |
| `src/parallel.rs` | Rayon entry points and serial fallback |
| `src/engine/parallel.rs` | Root split, shard count, complete-bucket jobs and per-job scratch reuse |
| `src/engine.rs` | Shared order probes, routing, digit planning, node partition orchestration and tile sorting |
| `src/key.rs` | Shared sealed encodings and `Direct` / `ByKey` extractors |
| `src/block.rs` | Shared block partition, scratch ownership and panic restoration |
| `src/block/parallel.rs` | Disjoint root shards, stable directory adoption and scratch transfer |
| `src/block/parallel/compact.rs` | Parallel block-cycle permutation and bounded boundary snapshots |
| `tests/parallel.rs` | Public Rayon API and, later, scheduling and recovery contracts |

Name the entry-point module `parallel` and the feature `rayon`. Keep the module private and re-export only the two entry points. The root partition and block permutation live in `block/parallel.rs` and its `compact` submodule because they form a separate ownership stage with independent stability, panic-recovery and permutation tests; the serial partition remains in `block.rs`.

The scaffold directly calls `engine::sort` with `Direct` or `ByKey(&key)`. This preserves the existing distinction between sealed direct keys and cached user classifications. It introduces no second extractor trait, duplicate sort engine, new unsafe code or serial-engine refactor.

The parallel entry point shares order probes, key encodings, digit planning and child sorting with the serial engine. Each root shard owns a disjoint contiguous input range and runs the existing block partition. The global directory concatenates the shard bucket ranges in original shard order, so equal keys remain stable; it adopts each shard's scratch allocation as a segment rather than copying live records into another arena. A parallel cycle permutation moves full blocks into global order, snapshots only bounded cycle boundaries, and then reuses the existing squeeze. During that squeeze, complete buckets are handed to scoped Rayon jobs while root materialization continues. Each job retains the pointer and length of every bucket as a distinct region; adjacent addresses are never combined into a wider borrow. The raw-pointer handoff is safe because compaction retires every handed-off region as both destination and copy source.

## Scratch and scheduling

For `P` root shards, `R` buckets, `n` records and `s` bytes per record, the block size is approximately `sqrt(20 n / (2 P R s))`. The `20` accounts for local and global block directories coexisting during adoption; each shard contributes `2 R` scratch blocks of that size. Thus root scratch and directories are O(√(nP)) for fixed record size and key width, which is O(√n) when the caller's pool width is fixed. `P` never exceeds the current Rayon pool size, and no n-record output buffer is allocated.

The root digit has at most twelve bits, so there are at most 4096 buckets. A job reuses one scratch allocation set across its buckets; the pool lock is held only to take or return scratch. If `mᵢ` is the largest bucket handled by job `i`, the `mᵢ` are disjoint and `Σ√mᵢ ≤ √(4096n)`. Child scratch therefore remains O(√n) for fixed key width; root scratch dominates the pool-dependent factor. Reentrant key functions may suspend jobs, so workspace count is bounded by the fixed root fan-out, not worker count. Rayon runtime storage and thread stacks are separate. A reusable parallel workspace remains deferred until these lifetimes and reuse costs are understood.

## Local measurement path

Use `pnpm tools compare algorithms --plan parallel` for local iteration, with the host's physical-core CPU mask and explicit input overrides. It defaults to eight-worker Tessera and Rayon stable sort beside the serial controls, using the maintained paired harness, memory worker and resumable reports. The [measurement contract](../development/benchmarking.md#local-parallel-comparisons) defines timing, A/A and memory scope; cloud continuous scheduling is unchanged.

## Focused measurements

On 10M full-width random 16-byte records with four pinned Xeon Gold 6140 cores, the retained serial-root/parallel-bucket path had a 2.27× median whole-call speedup over the serial API in 27 randomized blocks (95% block-bootstrap interval 2.15–2.31). The `A2/A1` control range was 0.88–1.23 for serial and 0.76–1.32 for Rayon-4. Capping the root digit at six bits did not improve that path (adaptive/capped paired ratio 0.999; 95% bootstrap interval 0.968–1.037). Adding the parallel root-block permutation to the same capped path was slower: the stage-one path divided by the stage-two path had a median ratio of 0.969 (95% interval 0.959–1.001). The apparent 1.16× gain from the cap against the permutation variant was therefore an interaction with that slower stage, not an independent root-width win. The current implementation keeps the existing planner and serial block compaction; these focused results are not broad acceptance. A per-bucket equality flag collected during serial scatter was rejected: on 10M 16-byte sparse64 records and four pinned cores, its Rayon-4/base median ratio was 1.190 (95% block-bootstrap interval 1.156–1.214; 0 of 27 blocks faster; candidate A/A range 0.900–1.131). Full-width random measured 0.980 (95% interval 0.936–1.003), inconclusive. The sparse slowdown is consistent with moving per-record work from parallel child scans into the serial scatter.

Nearby job sizes did not justify changing the 256 KiB cutoff. On 10M full-width random 16-byte records, four pinned cores and the branch-aligned profile, candidate/base Rayon-4 median time ratios were 1.019 for 128 KiB (27 blocks, 8 faster; 95% block-bootstrap interval 1.003–1.061) and 0.997 for 512 KiB (54 blocks, 29 faster; 0.974–1.010). Both remain within their baseline/candidate A/A envelopes: 0.864–1.130/0.810–1.243 and 0.801–1.249/0.724–1.271, respectively. Neither meets the protocol's directional-win requirement; keep 256 KiB.

## Implementation sequence

1. **Scaffold (complete).** The optional dependency, two entry points, documented serial fallback and contract tests are in place. Continue to validate default, `no_std`, Rayon, Rayon without default features, and Rayon with Portable SIMD; check the Rayon MSRV as the engine evolves. No scheduling thresholds or serial engine heuristics change.
2. **Overlapped root bucket completion (complete).** Large inputs reuse the shared probes, digit planner and node/tile engine; complete buckets are grouped into 256 KiB jobs and sorted while the serial root compaction continues. Root fan-out is bounded by the existing 12-bit radix maximum, and the serial path is unchanged. Contract tests cover task panics, provenance, and multiple workers. The job cutoff remains subject to controlled measurement.
3. **Parallel root partition (implemented experimentally).** Root shards reuse the existing block partition, merge bucket directories stably, adopt shard scratch, and parallelize the resulting block permutation before the existing squeeze and overlapping bucket jobs. Contract tests cover ownership, stability, exclusive borrows and panic recovery. Controlled A/B, peak-scratch and broad-distribution acceptance remain in progress.
4. **Acceptance.** Follow the [benchmark protocol](../development/benchmarking.md), comparing the complete public call against the serial engine and Rayon's stable sort at controlled pool widths. Cover random, ordered, repeated and skewed keys, wide keys, owned records and expensive extraction; report A/A ranges and peak memory. Extend sanitizer and Miri coverage to the new mechanisms. Promote the implemented contracts to a guide and engine design, and keep settled rationale with the engine design.
