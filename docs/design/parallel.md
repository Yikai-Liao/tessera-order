# Parallel stable sorting

The optional `rayon` feature adds `par_stable_sort` and
`par_stable_sort_by_key`. It enables `std`; the default sequential build and
`no_std` configuration are unchanged. The implementation is experimental while
controlled performance measurements are evaluated.

## Ownership and scheduling

The parallel front end follows the sequential adaptive checks and detects ordered, constant and nearly ordered
inputs before partitioning. Inputs smaller than 1 MiB, and one-thread pools, run through
the sequential entry point. Eligible Direct u32/u64 single-digit windows use the bounded count route below. Other single-digit, sampled-heavy-key and outlier routes keep the existing sequential node engine.

The root stable block partition materializes buckets in address order. Once a
bucket is retired as a copy source and destination, the compaction callback
transfers it to a pending batch. At 256 KiB, that batch becomes a Rayon scoped
task. Child sorting can therefore overlap the remaining serial compaction.
Single buckets are never split, so equal keys cannot cross tasks. The root
scatter and compaction are still serial in this public implementation.

Each batch stores a separate pointer and length for every bucket. Adjacent
addresses do not permit extending one exclusive slice's pointer into the next
slice. Keeping the original bucket pointers preserves their distinct borrowing
provenance. Each task reconstructs only those exact slices. The pool mutex is
held only while taking or returning workspace storage, never while invoking a
key function or sorting records. Existing sequential movement guards restore a
child on panic, and the scope joins all children before returning to the caller.
Keys require `Fn + Sync`; records require only `Send`, not `Sync`, `Clone` or
`Copy`. The 128-bit key path keeps its existing single-half specialization.

## Space bound

Let W be the number of workspace objects simultaneously leased or retained by
the pool and n the input length. The root retains O(√n) scratch. Each child
workspace is reused across distinct buckets; if mᵢ is its largest processed
bucket, the selected maximum buckets are disjoint, so Σmᵢ ≤ n and
Σ√mᵢ ≤ √(Wn). There is also O(W) fixed tile/cursor storage and O(R) batch
metadata for R root buckets, with record size and key width fixed. The heap
bound is O(√n + √(Wn) + W + R); it is not a full-size record output buffer.

For ordinary non-reentrant extractors, no more than the pool's worker count
can hold workspaces simultaneously. That is not a hard guarantee for arbitrary
user keys: a key can enter Rayon, suspend its current job and execute another
job on the same thread. W is conservatively bounded by the number of root
bucket jobs, at most R, rather than by the worker count in that situation.
Rayon's runtime, thread stacks and work recursively requested by user code are
separate. A strict worker-count-dependent workspace cap for reentrant keys is
not implemented by this dynamic scheduler. Free calls drop the pool on return;
there is no parallel reusable-workspace API.

## Validation

`tests/parallel.rs` covers stable order, permutations, owned records and drop
counts, exclusive borrows, Send-but-not-Sync records, Sync-only key functions,
inconsistent keys, worker panics, integer and float encodings, and explicitly
sized 1/2/4-thread pools. The existing sequential recovery suite remains relevant
because tasks invoke the same record-moving kernels. Focused strict-provenance
Miri tests exercise deferred adjacent bucket pointers with scalar, owned
`String + Cell`, and exclusive `&mut Cell` records through the actual partition
callback and batch kernel. These tests execute the batch sequentially; they do
not validate Rayon scheduling. Rayon scheduler tests are
not claimed as covered by Miri; they are explicitly excluded under Miri.

Performance experiments follow the repository benchmark protocol: one binary,
randomized blocks, duplicate original-baseline samples, an independent identical-
source placement control, a split sequential control, explicit Rayon thread
counts, and full reference-output validation outside timing. Raw measurements
and the research harness remain outside this repository. A timing result without
its A/A and placement ranges is not an acceptance result.

## Bounded Direct count windows

After the existing adaptive checks, a full-input varying-bit scan proves the constant bits outside a window. Only sealed Direct u32/u64 values with one to eight varying bits can enter the parallel count route. ByKey, other primitive types and wrappers decline; sorted, reverse and equal early exits remain unchanged. Narrow forwarding preserves its bit offset but does not enable u128. If admission fails, the input is unchanged and the original serial fallback runs.

For P pool workers and R = 2^width buckets, each input chunk fills a private histogram row. A serial prefix pass then assigns disjoint output-rank chunks; filling combines each digit with the proven fixed bits. Equal unsigned values are identical records, so this retains stability without reconstructing arbitrary payloads. The route introduces no unsafe code, user callbacks or n-record scratch array. Pool, Job and root scheduling are unchanged for fallback inputs.

The vectors request exactly (P×R + R + 1)×sizeof(usize) bytes. Admission also accounts a conservative 4096×P task/frame allowance and requires the accounted total to fit 8×P×ceil(sqrt(n))×record_bytes, P≥2 and P≤n/4096, with checked arithmetic. Preparation scratch, including its existing workspace allocation, remains additional; counting creates no root/child jobs or their workspaces. This is a separate count-route bound, not the workspace-lease formula above or a measurement of Rayon stacks. Fixed P retains O(√n) auxiliary space; a worker-independent bound is not claimed.

The [public-core screen](../../research/patches/public-count-window-results/REPORT.md) preserves the complete controls: W8 supports three of four narrow n10M cases against both public and original serial baselines; u32/d256 and equal37 remain inconclusive. This experimental branch enables only the validated domain. It does not establish generic-sort gains or absence of regressions. The [source safety record](../../research/patches/public-count-window-source/SAFETY.md) and [validation scope](../../research/patches/public-count-window-source/VALIDATION.md) distinguish native tests from scalar strict-Miri coverage without real Rayon pool execution.
