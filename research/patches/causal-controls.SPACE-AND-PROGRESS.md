<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Space, ownership and progress boundaries

## Shared invariants

All controls retain the historical independent materialized-bucket pointers, channel ownership transfer, scope join, child kernels and root recovery machinery. None introduces a new raw record move or widens a bucket borrow. Input storage outlives scoped consumption. Root and child workspace allocations are not inferred from RSS; no new heap measurement is claimed here. Existing nested/reentrant user code can suspend multiple workspaces on one worker, so retained child workspace count is not proved bounded by the physical worker count.

## Fixed root P4

Only general-root requested shards become `worker_count.min(4)`. Child worker count is unchanged. Root P feeds the same B formula, shard construction, partial directory, `2*P*R*B` root-scratch capacity and permutation scheduling. At 10M the complete W4 geometry is reproduced, rather than tuning only B. Below the established thresholds the original fallback rules remain. Root and children still overlap. No new wait/progress dependency is introduced; inherited native guards passed.

## Sleep50

Only an Empty receiver result changes OS yield to a fixed 50µs sleep. The receiver mutex temporary has already dropped. Channel disconnect, queue ownership, batch size, child work and panic handling are unchanged. Sleep is not a guarantee of an exact 50µs wakeup; observed acquisition delay can increase. This is not blocking `recv` under the shared receiver mutex. Native nested and dual-panic guards passed, but no guarantee is made for arbitrary user-created blocking cycles.

## Deferred overlap

The original P−1 key clones are prepared at the first original threshold crossing, preserving Clone/panic phase. Their vector lives until root completion and actual channel closure; only then are the original P−1 tasks spawned, followed by producer consumption. If the threshold is never reached, no tasks are spawned. The queue may now retain all R materialized-bucket descriptors simultaneously; this changes peak lifetime/backlog of metadata, not record layout. It is not a measured heap-bound claim.

Every queued pointer retains the provenance of its own callback slice. Root compaction retires that bucket before publication; later root work does not regain ownership of it. Delaying use does not extend one bucket pointer across adjacent buckets. Root scratch can be released independently, and the scope keeps input storage alive. On producer callback/Clone panic before launch, original compaction restores records and queued descriptors drop without dropping T. Consumer movement/key panic occurs only after root completion; scoped unwinding still joins and record guards restore ownership.

D dedicated tests never make the producer callback wait for consumers that have not been spawned. Actual root-return and sender-close witnesses cover producer drain as well as spawned consumers. Closed-channel consumers need no future producer progress and exhibit no Empty wait in the verified traces. Children remain parallel; this is not the old huge-batch serial-child control. O's simultaneous overlap-panic fixture is intentionally not a D requirement; separate producer and consumer failure phases are tested.

The policy removes overlap, root-phase polling/receiver competition and moves task-spawn overhead. It also changes cache-hot timing. These changes are a package; no memory-bandwidth cause is inferred solely from a faster squeeze. Scatter/permutation occur before the original spawn and serve as internal negative controls.
