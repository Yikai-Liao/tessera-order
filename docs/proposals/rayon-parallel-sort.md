<!-- SPDX-License-Identifier: BSD-2-Clause -->

# Rayon parallel sorting

Status: implemented and locally validated. Cross-host tuning and source-attribution experiments remain planned.

The optional Rayon entry points keep the slice API, stable ordering and generic records. They share encodings, probes, digit planning and child sorting with the serial engine. Large roots scatter through disjoint shards; bounded buckets can be sorted before materialization and written directly to their final bands. No platform-specific allocation or page remapping is used.

## Public API

Enable the crate's `rayon` feature. It enables `std` and the optional Rayon dependency, including with `default-features = false`. Default and `no_std` builds acquire no required dependency.

```rust
pub fn par_stable_sort<K: RadixKey + Send>(values: &mut [K]);

pub fn par_stable_sort_by_key<T, K, F>(values: &mut [T], key: F)
where
    T: Send,
    K: RadixKey,
    F: Fn(&T) -> K + Sync;
```

Records need `Send`, not `Sync`, `Copy` or `Clone`. Tasks share a borrow of the key function, so it needs `Fn + Sync`, not `Send` or `Clone`. Key values remain local to each task. Calls may overlap and their order is unspecified. Inconsistent keys leave ordering unspecified but preserve every record. All tasks restore their records before a panic reaches the caller; which panic wins is unspecified.

The caller selects the pool; sorting never creates one:

```rust
let pool = rayon::ThreadPoolBuilder::new().num_threads(8).build().unwrap();
let mut records = [(2u32, "first"), (1, "middle"), (2, "last")];
pool.install(|| tessera_sort::par_stable_sort_by_key(&mut records, |record| record.0));
assert_eq!(records, [(1, "middle"), (2, "first"), (2, "last")]);
```

Inputs below 1 MiB and one-thread pools use the serial engine. Ordered and constant inputs retain their root fast paths; this does not promise parallel acceleration for those paths. Scratch remains internal.

## Implementation

| Module | Responsibility |
|---|---|
| `src/parallel.rs` | Public entry points and contracts |
| `src/engine/parallel.rs` | Root routing, shard count, bounded bucket eligibility and scratch reuse |
| `src/key.rs` | Encodings, cached user classifications and parallel counting of small direct-key windows |
| `src/block/parallel.rs` | Independent root shards and stable directory adoption |
| `src/block/parallel/finish.rs` | Bounded staging, source relocation, final-band writes and recovery |
| `src/block/parallel/compact.rs` | Full-block permutation for the materialized-bucket fallback |

Each shard reuses the serial block scatter on its own mutable input. Adoption concatenates sources in bucket-major, original-shard order and retains each scratch allocation as a segment. Completion order never determines record order.

Bounded finishing is offered for at least 2²¹ records of at most 16 bytes. Its per-bucket limit is `16 * floor(sqrt(n))`; it is disabled when all buckets at that limit could not cover the input. Actual counts must also fit the limit, or the whole root takes the existing permutation/compaction path. Eligible layouts use four times the balanced block size; fallback-only layouts retain their original block size. A bounded child may use a tile as large as its bucket, still capped at 65,535 records. The [engine reference](../design/engine.md#parallel-root-completion) explains ownership and progress.

For direct keys whose remaining variation spans at most eight bits, workers instead build local counts and representatives, combine counts, and fill disjoint runs in parallel. Equal sealed encodings imply identical record bits, including float NaN payloads; this shortcut is never used for payload-bearing user-key records.

## Scratch and scheduling

Let `P` be the pool width and `R ≤ 4096` the root bucket count. Root scratch and directories use O(√(nP)) space for fixed record size and key width. Their balanced block size is approximately `sqrt(20 n / (2 P R s))`, before the eligible-layout multiplier and existing floors; the 20 accounts for local and adopted directories coexisting.

Each bounded bucket stages at most `16 * floor(sqrt(n))` records. Ordinarily only active tasks retain staging, but reentrant callbacks can suspend tasks: the worst-case count is bounded by `R`, not by the number of workers. Staging and enlarged child tiles therefore add O(R√n). Direct counting uses O(PR) bounded histogram storage. With fixed key width, the conservative total upper bound is O(P√n). No contiguous n-record output buffer is allocated; aggregate suspended staging can nevertheless approach the input size at eligible finite lengths. Rayon runtime storage and thread stacks are separate.

Completed staging is released immediately. Jobs recycle serial scratch without holding its pool lock across user code. Every freed-block cache remains registered until all finishing and recovery has joined, including caches belonging to suspended or completed tasks.

## Measurements and remaining work

Use `pnpm tools compare algorithms --plan parallel` with the host's physical-core CPU mask and explicit input overrides. The shared runner verifies outputs and records nine shuffled blocks, serial Tessera A/A, memory, executable provenance and resumable reports. The two existing cloud configurations additionally measure the continuous plan's four-thread cases, strictly one case at a time after serial workers exit; see [continuous coverage and rollout](../development/continuous-benchmarks.md#pipeline).

The [eight-core measurement record](../design/port.md#eight-core-parallel-completion) reports current whole-call results and limitations. They compare algorithms in the measured executable, not isolated source revisions. Broader host coverage, independent placement controls, larger records and faster skew/ordered paths remain work; fivefold speedup is not a guarantee for every type or input.
