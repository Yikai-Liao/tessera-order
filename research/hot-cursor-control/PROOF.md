<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Checked hot-writer explicit group realization

## Scope and hypothesis

This isolated research-only patch starts from the checked aligned root manifest `00c9e7a791d3148843dda806d7a37139d73d8a93d8cf3b0fa8b3291a68d0bcd8`. It changes only the cursor representation and ordered bounded source-loop schedule. It does not incorporate the negative unchecked-index experiment. All source files other than lib.rs are identical to the checked baseline. The archived patch reconstructs the final candidate exactly from that baseline.

The hypothesis is that a smaller hot writer array together with native-style source grouping may reduce ordinary scatter cost. This is one combined core hypothesis, not an attribution to either feature alone. It still performs N scatter key calls and N scatter payload writes, plus the unchanged histogram and restore passes. No speedup, cache improvement, or emitted unroll is established by source alone. Actual ordinary executable disassembly must be checked before timing.

## State relation and initialization

Each old Cursor maps bijectively to a Writer(next, end) and Segment(logical, first_offset, end_rank) at the same shard/bucket indices. Both vectors are initialized directly, with exactly R elements; no old cursor vector coexists. Writers start with null next/end. Segment starts and counts are the unchanged checked rank plan. All bucket lookups remain checked. The zero-count segment is never written and remains null; final validation tests the same rank equality. Compile-time assertions verify Writer is two machine words for each supported record and Segment is three machine words.

The new writer allocation happens before any payload movement. Allocation or initialization failure cannot introduce uninitialized input records. Existing research semantics still apply after movement starts: an allocation or scheduler panic may leave initialized but duplicated/unsorted Copy records. No arbitrary changing/panicking user-key, owned-record, or generic panic-recovery contract is claimed.

## Step correspondence and safety

For each source block, complete = source_begin + floor((source_end-source_begin)/group)*group, where group = clamp(4*sizeof(usize)/max(sizeof(T),1),1,4). Explicit groups enumerate four u64 or two Row16 source positions in increasing order; the fallback enumerates each group offset and the scalar tail enumerates complete through source_end-1. No group crosses the source endpoint, including B=1, odd B, and a short final block. There is no extra payload staging. The size-specialized group is 4 for u64,2 for Row16 and 1 for Row128 on the 64-bit target.

Each helper invocation completes the current record write before the next invocation. It holds no writer reference or precomputed destination across invocations, so repeated same-bucket records see the updated writer state. The first write and every exhausted fragment perform lazy refill BEFORE writing. The cold segment is accessed only for this refill during the hot traversal. The checked base rank, segment end cap, first offset, boundary-map lookup, free-pop order, directory assignment and physical pointer derivations are unchanged. In particular, a completed final fragment does not eagerly allocate its successor. Source blocks enter the free pool only after every record has been consumed, with the short tail never recycled as a full block.

By induction over this identical source order within each shard, the old and new states have equal writer pointers and cold rank fields before and after every record. They therefore have the same refill events, physical destination IDs, free-pop/push sequence and directory writes. Parallel shard independence is unchanged. Final cursor/segment checks, natural-path restore and Public child groups are unchanged. This structural proof is supplemented by exact stable output and complete diagnostic directory signature comparisons, not by an added production trace.

## Space accounting

On the 64-bit target the old field payload was 40PR bytes, now 16PR writer bytes plus 24PR segment bytes. There are P extra Vec headers (24P bytes on this target), P extra allocations and their allocator bookkeeping/rounding. These are real additions; neither unchanged heap peak nor an exact allocator charge is claimed. No new T cells are allocated. The five-word payload vectors remain live through restore just as the old cursor vectors did; conservatively count all headers for that lifetime. Existing O(PRB) record scratch and O(n/B+PR) word metadata bounds remain unchanged. At R=256, per-shard hot field payload is 4096 bytes rather than 10240 bytes, a source-layout fact and not a measured cache benefit.
