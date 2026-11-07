# Parallel stable sorting

The optional `rayon` feature adds `par_stable_sort` and
`par_stable_sort_by_key`. It enables `std`; the default sequential build and
`no_std` configuration are unchanged. The implementation is experimental while
controlled performance measurements are evaluated.

## Ownership and scheduling

The parallel front end follows the sequential adaptive checks and detects ordered, constant and nearly ordered
inputs before partitioning. Inputs smaller than 1 MiB, and one-thread pools, run through
the sequential entry point. Single-digit, sampled-heavy-key and outlier routes
also keep the existing sequential node engine.

The root stable block partition completes and restores every record to the
input slice. Only then do Rayon tasks receive sub-slices. Tasks split at bucket
boundaries with `split_at_mut`; no bucket is divided and equal keys therefore
cannot cross tasks. Each task also receives exclusive workspace storage.
128-bit keys retain the sequential 64-bit specialization when only one half
varies. The temporary order-probe keys are released before partitioning.
Bucket groups are divided near the midpoint by record count, rather than by
bucket count, to limit imbalance. At most one workspace per selected task group is
allocated. The same workspace is reused for all buckets assigned to that task group.

No new unsafe record movement is needed. Existing sequential tile/block guards
restore records on panic, and `rayon::join` waits for both branches before
propagating a panic. Key functions implement `Fn + Sync`, while records only
need `Send`. The API does not require `Clone` or `Copy` records.

The first implementation does not parallelize the root partition or descend in
parallel inside a single large bucket. Those stages can limit scaling. It also
finishes small children after root compaction rather than inside its cache-hot
callback. The split single-thread control measures this scheduling boundary's
cost separately from parallel execution.

## Space bound

Let P be the number of selected workspaces and n the input length. P is bounded
by the current Rayon pool size, the input grain count and the root bucket count.
The root workspace retains O(√n) block scratch. If mᵢ is the largest child ever
processed by the i-th task group's workspace, the selected children lie in disjoint
bucket groups, so Σmᵢ ≤ n and Σ√mᵢ ≤ √(Pn). Add O(P) for per-workspace tile,
probe and fixed metadata storage, with record size and key width fixed.

Heap scratch is therefore O(√n + √(Pn) + P), excluding Rayon's runtime and thread
stacks. Fixed P preserves O(√n), but increasing the worker count increases memory.
Each active worker also has the sequential tile cursor stack bound. Free calls
drop all workspaces on return; no parallel reusable-workspace API is added yet.

## Validation

`tests/parallel.rs` covers stable order, permutations, owned records and drop
counts, exclusive borrows, Send-but-not-Sync records, Sync-only key functions,
inconsistent keys, worker panics, integer and float encodings, and explicitly
sized 1/2/4-thread pools. The existing sequential recovery suite remains relevant
because tasks invoke the same record-moving kernels. Rayon scheduler tests are
not claimed as covered by Miri; they are explicitly excluded under Miri.

Performance experiments follow the repository benchmark protocol: one binary,
randomized blocks, duplicate original-baseline samples, an independent identical-
source placement control, a split sequential control, explicit Rayon thread
counts, and full reference-output validation outside timing. Raw measurements
and the research harness remain outside this repository. A timing result without
its A/A and placement ranges is not an acceptance result.
