<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Design and proof obligations

## Scope and fixed policy

The sole candidate mechanism replaces the combined baseline's polling consumers with bounded drain-to-empty scheduling. Root digit planning, block sizes, serial kernels, tile policy, default R256 root and 128 KiB batch boundary remain unchanged. Exact source identities, including all inherited controls, are recorded in the source map. This archive is a rejected experiment, not a claim that this scheduling policy is worthwhile.

## Ownership and exact bucket intervals

The producer publishes only fully materialized contiguous bucket slices, after their source storage is retired by root compaction. Each metadata item carries that bucket's own pointer and length. No pointer is widened across neighboring buckets. The queue preserves original batch boundaries and assigns each whole batch to one resource. Child sorting uses the unchanged serial sorter and recovery guards. Root and children never share mutable scratch.

There are P fixed child slots, each with one key slot, one Workspace and a preallocated metadata vector. During root publication only P−1 child slots are available. After the root returns, the producer's original key and a separate child Workspace occupy the final slot, preserving producer participation. Reserved, running, and reentrantly suspended tasks consume the same slot; nested Rayon work does not create additional slots for this call. Explicit recursive user sort calls have independent budgets.

## Space bound, not a measured multiplier

One child Workspace retains capacity for its largest processed bucket, so its inherited auxiliary bound is O(sqrt(n)) for an input of n records. At most P such child workspaces exist for the call, plus the separate baseline root Workspace and native root shard storage. The metadata queue, batch boundaries, per-slot metadata and panic slots are preallocated; root radix bounds the total descriptors. No resource is allocated merely because another task is reentrantly suspended.

This is structural workspace accounting, not an allocator measurement, constant-factor guarantee, or measured “few times serial” result. The fixed screen did not establish heap scaling. The baseline native root's own scratch/metadata bound remains inherited rather than newly proved by this scheduler patch.

## Queue transitions and progress

Publication, resource reservation, batch acquisition, and empty-return use the same metadata mutex. A worker encountering empty atomically returns its slot before releasing the lock and then returns from the task. A racing publication either precedes that check and is consumed, or observes the returned idle slot and can reserve it. Publication may precede a separate reservation lock acquisition; if another worker has already taken the batch, there is no stranded work. Successful scope completion asserts empty queues and restored resources.

Spawned closures carry only a slot ID and a queue reference. The slot remains scheduler-owned until the closure starts, so a failed spawn cannot drop a captured user key while root records are moved out. No task waits for a key/workspace or future publication, and no metadata lock spans sorting or user key/Clone/Drop code. Reentrant Rayon work may suspend consumers but does not introduce a workspace-wait cycle. These statements rely on Rayon eventually executing scoped work, not on a wall-clock fairness guarantee.

## Clone and panic behavior

Consumer keys are cloned at the original first-batch-threshold location, before any consumer launch. Each successful clone is immediately retained in its preallocated slot. Thus failure of a later Clone cannot prematurely destroy a previously cloned consumer key. Subthreshold roots neither clone nor launch child consumers; the producer drains their final batch.

A child-key panic first unwinds through the unchanged child's ownership guards. The drain loop catches the recovered panic, stores its payload in that slot's preallocated location, and marks failure. No failed slot is reused. Subsequent bucket starts and publication stop; already running child work may finish or recover. Root compaction still completes. Producer exceptions are caught only after native root recovery. Scope joins precede payload selection, payload destruction, and retained extractor destruction; selection occurs outside metadata locks.

The final successful invariant and tests do not suppress arbitrary panics from user destructors. Multiple destructor panics may abort unwinding; the guarantee is that root ownership restoration and child joins occur before retained user state is released. Existing native-root shard/extractor destruction behavior is inherited unchanged and not repaired by this candidate.
