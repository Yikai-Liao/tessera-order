<!-- SPDX-License-Identifier: BSD-2-Clause -->

# Proposal: Rayon parallel sorting

Status: optional API and overlapped root-bucket sorting implemented. Root scatter remains serial; broad acceptance measurements are pending. Date: 2026-11-08.

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

Ordering, stability, inconsistent-key safety and record preservation follow the [shared semantics](../../conformance/semantics.md). Key calls may overlap across tasks, and neither call order nor a particular worker assignment is promised. If a task panics, all tasks that may access the input must finish their restoration before the panic reaches the caller. Rayon [`join`](https://docs.rs/rayon/1.12.0/rayon/fn.join.html) waits for both branches even if one panics; every sorting task must also preserve its own records. Which panic wins when several tasks panic is unspecified.

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

Keep block movement with the partition while it shares the partition's ownership and recovery invariants. Split a complete mechanism into `block/parallel.rs` only if a future measured design establishes a separately understandable boundary; do not create empty module trees or a second `parallel_engine.rs` in advance.

## Scratch and scheduling

The current fallback has the serial engine's documented memory bounds. That does not establish the future parallel bound. For `q` concurrent disjoint nodes of lengths `n_i`, block scratch proportional to `sum(sqrt(n_i))` is at most `sqrt(q * n)`; tile buffers, cursor tables, probe keys, bucket metadata and Rayon runtime storage must be accounted for separately.

The root digit is chosen by the existing planner and has at most twelve bits, so there are at most 4096 nonempty buckets and jobs. A job reuses one workspace across its buckets; the pool lock is held only to take or return a workspace. The root's O(√n) block scratch remains live during child work because it is needed for compaction. If `mᵢ` is the largest bucket handled by job `i`, the `mᵢ` are disjoint across jobs and `Σ√mᵢ ≤ √(4096n)`. Thus the root reserve plus child reserves, bounded job metadata and fixed per-workspace tile storage remain O(√n) for fixed record size and key width. Reentrant key functions may suspend jobs, so workspace count is bounded by the fixed root fan-out, not by worker count. Rayon runtime storage and thread stacks are separate. A reusable parallel workspace remains deferred until these lifetimes and reuse costs are understood.

## Focused measurements

On 10M full-width random 16-byte records with four pinned Xeon Gold 6140 cores, the retained serial-root/parallel-bucket path had a 2.27× median whole-call speedup over the serial API in 27 randomized blocks (95% block-bootstrap interval 2.15–2.31). The `A2/A1` control range was 0.88–1.23 for serial and 0.76–1.32 for Rayon-4. Capping the root digit at six bits did not improve that path (adaptive/capped paired ratio 0.999; 95% bootstrap interval 0.968–1.037). Adding the parallel root-block permutation to the same capped path was slower: the stage-one path divided by the stage-two path had a median ratio of 0.969 (95% interval 0.959–1.001). The apparent 1.16× gain from the cap against the permutation variant was therefore an interaction with that slower stage, not an independent root-width win. The current implementation keeps the existing planner and serial block compaction; these focused results are not broad acceptance. A per-bucket equality flag collected during serial scatter was rejected: on 10M 16-byte sparse64 records and four pinned cores, its Rayon-4/base median ratio was 1.190 (95% block-bootstrap interval 1.156–1.214; 0 of 27 blocks faster; candidate A/A range 0.900–1.131). Full-width random measured 0.980 (95% interval 0.936–1.003), inconclusive. The sparse slowdown is consistent with moving per-record work from parallel child scans into the serial scatter.

Nearby job sizes did not justify changing the 256 KiB cutoff. On 10M full-width random 16-byte records, four pinned cores and the branch-aligned profile, candidate/base Rayon-4 median time ratios were 1.019 for 128 KiB (27 blocks, 8 faster; 95% block-bootstrap interval 1.003–1.061) and 0.997 for 512 KiB (54 blocks, 29 faster; 0.974–1.010). Both remain within their baseline/candidate A/A envelopes: 0.864–1.130/0.810–1.243 and 0.801–1.249/0.724–1.271, respectively. Neither meets the protocol's directional-win requirement; keep 256 KiB.

## Implementation sequence

1. **Scaffold (complete).** The optional dependency, two entry points, documented serial fallback and contract tests are in place. Continue to validate default, `no_std`, Rayon, Rayon without default features, and Rayon with Portable SIMD; check the Rayon MSRV as the engine evolves. No scheduling thresholds or serial engine heuristics change.
2. **Overlapped root bucket completion (complete).** Large inputs reuse the shared probes, digit planner and node/tile engine; complete buckets are grouped into 256 KiB jobs and sorted while the serial root compaction continues. Root fan-out is bounded by the existing 12-bit radix maximum, and the serial path is unchanged. Contract tests cover task panics, provenance, and multiple workers. The job cutoff remains subject to controlled measurement.
3. **Parallel root partition (in progress).** Address the serial root classification/scatter and compaction from their stable-rank and in-place movement invariants. Reuse encoding and digit planning; account for synchronization, block movement, rollback and scratch lifetimes. The overlap implementation is a measured baseline, not an acceptance target by itself.
4. **Acceptance.** Follow the [benchmark protocol](../development/benchmarking.md), comparing the complete public call against the serial engine and Rayon's stable sort at controlled pool widths. Cover random, ordered, repeated and skewed keys, wide keys, owned records and expensive extraction; report A/A ranges and peak memory. Extend sanitizer and Miri coverage to the new mechanisms. Promote the implemented contracts to a guide and engine design, and record settled choices under the implementation pull request's number.
