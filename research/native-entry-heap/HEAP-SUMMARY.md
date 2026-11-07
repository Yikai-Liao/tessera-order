<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Native-root Tessera: requested-heap observations

This research compares the existing native-root combined implementation with public revision55b299b3716cd1af9e997f30b1a3c1813455d8be. It does not introduce another algorithm or claim improvement over the existing combined implementation. The libraries used default/std/rayon/portable-simd features. Three fresh repetitions of four primary APIs across four canonical inputs produced48 observations on the same eight-worker pool configuration.

## What passed

All48 observations passed exact full-reference comparison. Owned String records also preserved each allocation's pointer, length and capacity. All API calls ended with zero captured requested bytes and zero captured header/padding bytes retained. No pre-capture allocation was freed or reallocated during a measured API call. The accounting balances passed; there is no observed ownership-accounting failure.

Inputs were SplitMix64 seed1:10million u64 values;10million16-byte key/index records;262144128-byte records with u64 key, index and112-byte payload; and26214432-byte u64-key/String records. Input/reference/String cloning was outside capture. All root, metadata, queue, permutation and consumer heap requests inside the whole API call were counted across threads. Shared root storage was counted once, not multiplied by consumer count.

## Requested peak bytes

Ranges span all three repetitions, not a selected best run. Both serial controls had identical requested peaks for every case.

| Input | Public parallel | Native parallel | Public serial | Native serial |
|---|---:|---:|---:|---:|
| u64 |3222462–3259260|6168395|2770254|2770254|
|16-byte record|4673594–4686156|7971658|3902036|3902036|
|128-byte record|2459950–2548194|17553330–17555208|2219040|2219040|
|String record|1060810–1093226|4564656–4599888|955690|955690|

Native peaks were2.226653× serial for u64,2.042948× for16-byte records,7.910326–7.911172× for128-byte records, and4.776294–4.813159× for String records. Relative to public parallel, native peaks were1.892575–1.914187×,1.701108–1.705680×,6.889243–7.135645× and4.207628–4.335479× respectively. Both serial denominators are retained; neither was selected after seeing results.

Native successful allocation/reallocation request counts were627–646,646–664,495–503 and307–313 in that same input order. Total native requested bytes were9718219–10136679,13127574–13346956,18941970–19031624 and5645536–5846560. These totals sum complete successful request sizes; they are not live peaks.

The accounting header/padding is excluded from every requested-byte figure above. Its independently observed native peak was984–1632bytes,992–1000bytes,968–992bytes and728bytes respectively. Header/padding peak and requested peak need not occur together; they must not be added and presented as a measured joint maximum. Allocator internal overhead, fragmentation, RSS and process stacks were not measured. No performance inference is made from this instrumented allocator.

## Repetition consistency, precisely

Native u64 and16-byte requested peaks were exactly identical across all three runs, even though request counts and cumulative requested bytes varied. Native128-byte peak spread was1878bytes (about0.011% of its minimum); String peak spread was35232bytes (about0.772%). Public parallel requested peaks also varied. All serial recorded allocation metrics repeated exactly.

The observed ranges are valid results of balanced captures; variation does not make the epoch counter nondeterministic or invalidate the48 observations. However, the predeclared protocol left inconsistent repetitions as an acceptance qualification. The earlier result is unchanged: accounting correctness passed, while the causes of the parallel differences have not been traced. That qualification remains open rather than being waived after observing the data. There were no rescue reruns.

## What unchanged source permits

Native consumers each construct an initially empty Workspace, retain it while consuming batches, and drop it when their queue loop finishes. Which consumer receives which batch depends on scheduling. Workspace tile scratch uses capacity checks and reserve_exact; block buffers resize/reuse their Vec storage across passes. Different batch-to-consumer assignments and growth histories can therefore change cumulative requests, retained capacity within a call, and overlap among live workspaces. Empty consumers need not allocate the same buffers as consumers that process children.

Public parallel jobs acquire a Workspace by popping a shared free pool, or create an empty one when none is available; completed jobs return it. Scheduling can change how many workspaces are needed concurrently and which existing capacities a later job reuses. Public Job bucket vectors and native batch vectors are heap-owned scheduling data. Native publication transfers each batch Vec through a shared std channel; the batch lifetime extends until its consumer completes that batch. Producer/consumer timing can change the overlap of live batch, queue, task and workspace storage. These are source-supported mechanisms, not a claim that a particular mechanism caused a specific measured byte delta. The standard channel's internal block policy was not independently audited here, so no exact channel-block attribution is claimed.

Native root geometry is computed from input size, record size, radix and requested shard count; actual scatter shards are then limited by available blocks. Adoption moves the existing shard scratch Vecs into shared root storage rather than allocating another copy of every shard scratch. That storage, directory and permutation bookkeeping are counted once. Scheduling-dependent lifetime overlap can affect aggregate high water without changing this ownership rule.

## Bounds and remaining limits

For requested consumers S, workers P and actual scatter shards Q, Q<=S<=P. The existing proof gives aggregate consumer Workspace<=S F(n), with F(n) the per-consumer scratch bound, plus separately accounted shared root/metadata storage. Root record scratch is2QRBt for radix R, block length B and record size t; shared scratch is not a per-consumer charge. This does not establish strict one-workspace-per-physical-thread usage: a reentrant key rayon::join can suspend a consumer and execute another on that thread. Independent nested sorts require separate invocation accounting.

Three observed repetitions do not prove worst-case or asymptotic space bounds. The results do not establish a universal small constant over serial, and no post-hoc definition of “few times” is introduced. Broader distributions, scaling, unresolved repetition attribution and the physical-thread qualification remain separate acceptance questions.

## Evidence and proof lineage

The original frozen generic source identity, public revision, native patch digest, primary/placement library identities, measured executable digest and evidence digests accompany this summary. Exact dependency/source identities were checked before and after execution. The allocation-accounting design assigns ownership epochs to individual requested layouts, so freeing a preexisting allocation cannot mask newly requested high water; retained capture ownership prevents epoch reset. Successful realloc preserves System.realloc behavior and logical replacement accounting. Independent allocator boundary, realloc-failure, alignment and cross-thread tests passed natively and under direct strict-provenance/default-Stacked-Borrows Miri. Small tied-key tests exercised all four APIs and exact owned-record preservation. This is focused accounting validation, not a formal proof or full native/Rayon Miri coverage.
