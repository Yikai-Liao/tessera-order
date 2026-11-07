<!-- SPDX-License-Identifier: BSD-2-Clause -->

# Proposal: Rayon parallel sorting

Status: optional API and serial-root/parallel-bucket execution implemented; acceptance measurements pending. Date: 2026-11-07.

Add parallel stable radix sorting as an optional capability of the existing Rust crate. The first execution stage reuses the engine's probes, digit planner, block partition and tile/node sorting. It partitions the root serially, then sorts complete child buckets in bounded Rayon jobs. It is a baseline for further root-partition work, not a speedup claim or final parallel design.

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

Ordering, stability, inconsistent-key safety and record preservation follow the [shared semantics](../../conformance/semantics.md). Key calls may eventually overlap across tasks, and neither call order nor a particular worker assignment is promised. If a task panics, all tasks that may access the input must finish their restoration before the panic reaches the caller. Rayon [`join`](https://docs.rs/rayon/1.12.0/rayon/fn.join.html) waits for both branches even if one panics; every sorting task must also preserve its own records. Which panic wins when several tasks panic is unspecified.

Do not add parallel methods to `Workspace<T>` yet. It currently owns scratch for one serial sort. Retained capacity, suspended tasks and scratch distribution need a measured design before a reusable parallel workspace becomes public.

## Source boundaries and reuse

| File | Responsibility |
|---|---|
| `src/lib.rs` | Crate documentation, feature gates and root exports |
| `src/parallel.rs` | Rayon entry points and serial fallback |
| `src/engine/parallel.rs` | Root split, complete-bucket jobs and per-job workspace reuse |
| `src/engine.rs` | Shared order probes, routing, digit planning, node partition orchestration and tile sorting |
| `src/key.rs` | Shared sealed encodings and `Direct` / `ByKey` extractors |
| `src/block.rs` | Block partition, scratch ownership and panic restoration |
| `src/workspace.rs` | Scratch for one serial sorting operation |
| `tests/parallel.rs` | Public Rayon API and, later, scheduling and recovery contracts |

Name the module `parallel` for its responsibility; name the feature `rayon` for its dependency. Keep it private and re-export its two entry points. Start with flat files, matching the crate. A separate crate, public slice extension trait or executor abstraction would introduce more integration points than these two complete operations need.

The scaffold directly calls `engine::sort` with `Direct` or `ByKey(&key)`. This preserves the existing distinction between sealed direct keys and cached user classifications. It introduces no second extractor trait, duplicate sort engine, new unsafe code or serial-engine refactor.

The parallel entry point shares the existing order probes and key encodings, but keeps the serial entry point unchanged. It uses the existing digit planner and `Sorter` for root selection and child sorting. The block engine hands each complete bucket to a scoped Rayon job during compaction, overlapping child sorting with the remaining root compaction. Each job retains the pointer and length of every bucket as a distinct region; adjacent addresses are not combined into a wider borrow. This narrow handoff uses raw pointers because the visitor's temporary borrow cannot outlive its callback. Its safety argument depends on compaction retiring each handed-off region as both destination and copy source.

Keep parallel block movement in `block.rs` while it shares the partition's ownership and recovery invariants. Split a complete mechanism into `block/parallel.rs` only if the implementation establishes a separately understandable boundary; do not create empty module trees or a second `parallel_engine.rs` in advance.

## Scratch and scheduling

The current fallback has the serial engine's documented memory bounds. That does not establish the future parallel bound. For `q` concurrent disjoint nodes of lengths `n_i`, block scratch proportional to `sum(sqrt(n_i))` is at most `sqrt(q * n)`; tile buffers, cursor tables, probe keys, bucket metadata and Rayon runtime storage must be accounted for separately.

The root digit is chosen by the existing planner and has at most eight bits, so there are at most 256 nonempty buckets and jobs. A job reuses one workspace across its buckets; the pool lock is held only to take or return a workspace. The root's O(√n) block scratch remains live during child work because it is needed for compaction. If `mᵢ` is the largest bucket handled by job `i`, the `mᵢ` are disjoint across jobs and `Σ√mᵢ ≤ √(256n)`. Thus the root reserve plus child reserves, bounded job metadata and fixed per-workspace tile storage remain O(√n) for fixed record size and key width. Reentrant key functions may suspend jobs, so workspace count is bounded by the fixed root fan-out, not by worker count. Rayon runtime storage and thread stacks are separate. A reusable parallel workspace remains deferred until these lifetimes and reuse costs are understood.

## Implementation sequence

1. **Scaffold (this change).** Optional dependency, two entry points, documented serial fallback and contract tests. Validate default, `no_std`, Rayon, Rayon without default features, and Rayon with Portable SIMD; check the Rayon MSRV. No scheduling thresholds or engine heuristics change.
2. **Overlapped root bucket completion (implementation stage).** Large inputs reuse the shared probes, digit planner and node/tile engine; complete buckets are grouped into 256 KiB jobs and sorted while the serial root compaction continues. Root fan-out stays within the existing eight-bit radix limit, and the serial path is unchanged. Contract tests cover task panics, provenance, and multiple workers. Cutoffs remain candidates pending controlled measurements.
3. **Parallel root partition.** Address the serial root classification/scatter and compaction from their stable-rank and in-place movement invariants. Reuse encoding and digit planning; account for synchronization, block movement, rollback and scratch lifetimes. The first overlap baseline still has a serial scatter critical path, so it is not an acceptance target by itself.
4. **Acceptance.** Follow the [benchmark protocol](../development/benchmarking.md), comparing the complete public call against the serial engine and Rayon's stable sort at controlled pool widths. Cover random, ordered, repeated and skewed keys, wide keys, owned records and expensive extraction; report A/A ranges and peak memory. Extend sanitizer and Miri coverage to the new mechanisms. Promote the implemented contracts to a guide and engine design, and record settled choices under the implementation pull request's number.
