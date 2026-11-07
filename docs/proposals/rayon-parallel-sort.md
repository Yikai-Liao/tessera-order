<!-- SPDX-License-Identifier: BSD-2-Clause -->

# Proposal: Rayon parallel sorting

Status: API scaffold implemented; parallel execution pending. Date: 2026-11-07.

Add parallel stable radix sorting as an optional capability of the existing Rust crate. This branch establishes entry points, dependency wiring and contracts. Both entry points currently delegate the entire sort to the serial engine. No parallel speedup or parallel memory bound has been measured or claimed.

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
| `src/parallel.rs` | Rayon entry points, future task scheduling, concurrency budget and serial fallback |
| `src/engine.rs` | Shared order probes, routing, digit planning, node partition orchestration and tile sorting |
| `src/key.rs` | Shared sealed encodings and `Direct` / `ByKey` extractors |
| `src/block.rs` | Block partition, scratch ownership and panic restoration |
| `src/workspace.rs` | Scratch for one serial sorting operation |
| `tests/parallel.rs` | Public Rayon API and, later, scheduling and recovery contracts |

Name the module `parallel` for its responsibility; name the feature `rayon` for its dependency. Keep it private and re-export its two entry points. Start with flat files, matching the crate. A separate crate, public slice extension trait or executor abstraction would introduce more integration points than these two complete operations need.

The scaffold directly calls `engine::sort` with `Direct` or `ByKey(&key)`. This preserves the existing distinction between sealed direct keys and cached user classifications. It introduces no second extractor trait, duplicate sort engine, new unsafe code or serial-engine refactor.

When scheduling is introduced, extract a complete node-preparation and partition stage inside `engine.rs`, used by both serial and parallel orchestration. Keep order probes, variation masks, digit widths and adaptive shortcuts authoritative there; expose only the stage results the scheduler needs. Reuse the existing tile finish through the serial engine. Measure any changes to the serial routing path before accepting that extraction.

Partition a node completely before splitting its buckets into disjoint slices for Rayon tasks. The current block visitor borrows one mutable extractor and may run while other records still live in partition scratch. It is not a task-spawning interface. Add a narrow partition operation that returns completed bucket boundaries when needed, through the existing partition implementation, rather than exporting its raw pointers or retaining callback slices. Keep the current serial visitor for its cache-local tile finish.

Keep parallel block movement in `block.rs` while it shares the partition's ownership and recovery invariants. Split a complete mechanism into `block/parallel.rs` only if the implementation establishes a separately understandable boundary; do not create empty module trees or a second `parallel_engine.rs` in advance.

## Scratch and scheduling

The current fallback has the serial engine's documented memory bounds. That does not establish the future parallel bound. For `q` concurrent disjoint nodes of lengths `n_i`, block scratch proportional to `sum(sqrt(n_i))` is at most `sqrt(q * n)`; tile buffers, cursor tables, probe keys, bucket metadata and Rayon runtime storage must be accounted for separately.

Budget live scratch owners per call, including tasks suspended during work stealing. A thread count alone does not bound those owners. Divide a finite task allowance among child jobs, and release a parent's partition scratch before child jobs allocate theirs. Specify and test the resulting peak bound before enabling concurrent buckets. Avoid a mutex around one shared workspace and avoid allocating a workspace for every queued bucket. A reusable parallel workspace remains deferred until these lifetimes and reuse costs are understood.

## Implementation sequence

1. **Scaffold (this change).** Optional dependency, two entry points, documented serial fallback and contract tests. Validate default, `no_std`, Rayon, Rayon without default features, and Rayon with Portable SIMD; check the Rayon MSRV. No scheduling thresholds or engine heuristics change.
2. **Concurrent buckets.** Extract the shared complete node stage, then sort completed disjoint buckets in Rayon's current pool. Keep small jobs serial. Add a per-call scratch budget, task-level panic tests, inconsistent keys, owned records and exclusive borrows. Verify that multiple workers execute sorting work. Measure task granularity and the serial baseline with A/A controls; do not choose a cutoff from an unmeasured constant.
3. **Parallel partition.** Address the serial root partition with a separate ownership and recovery design. Reuse encoding and digit planning; account for histogram, block movement and synchronization memory. Concurrent child sorting alone does not establish whole-sort scalability.
4. **Acceptance.** Follow the [benchmark protocol](../development/benchmarking.md), comparing the complete public call against the serial engine and Rayon's stable sort at controlled pool widths. Cover random, ordered, repeated and skewed keys, wide keys, owned records and expensive extraction; report A/A ranges and peak memory. Extend sanitizer and Miri coverage to the new mechanisms. Promote the implemented contracts to a guide and engine design, and record settled choices under the implementation pull request's number.
