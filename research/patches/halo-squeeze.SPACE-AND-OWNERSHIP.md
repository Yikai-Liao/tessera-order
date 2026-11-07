<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Halo movement and space argument

This is the argument for the checked research implementation, not a claim that
all arbitrary unsafe Rust executions or arbitrary blocking callbacks are proven.

## Native source geometry

Let n be records, B records per block, P_root adopted root shards, R semantic
buckets, F = floor(n/B), H = P_root R, and C = F + 2H physical blocks.
After the existing canonical permutation, active logical block k (relative to
the free prefix H) has source input[kB..(k+1)B] when k < F, otherwise scratch
physical block k - F. Scratch comprises separate retained shard allocations;
virtual block coordinates do not permit pointer arithmetic between them.
The original short input tail [FB,n) is stale, never an authoritative source.

Let ell[k] be the initialized prefix length, 0 <= ell[k] <= B, and
r(k) = sum of preceding prefix lengths. Each input record at kB + u belongs
at r(k) + u. Since r(k) <= kB and the accumulated holes kB - r(k) never decrease,
this is forward leftward compaction in the input portion.

## Threatened intervals and concurrent movement

Choose consecutive groups at semantic bucket boundaries. A group with source
blocks [a,b) owns final destination [r(a),r(b)). Later groups' destinations have
union [r(b),n). The only live input sources threatened by later groups are
valid prefix cells of this group at addresses >= r(b).

Before any movement, snapshot the raw interval

    [max(aB, r(b)), min(b,F)B)

when nonempty. The implementation clamps scratch-only groups before multiplying
virtual coordinates. This interval may contain holes and is copied as raw
MaybeUninit cells, without typed reads of padding. Full raw length, including
holes, is charged to the snapshot budget. Distinct groups' halos are disjoint.
Only originally valid prefixes are ever moved as records.

Snapshot staging transfers sole authority for threatened live records; their
original bytes become stale copies, not additional owners. Reads within the
halo use snapshot offsets; other input reads use the original raw ancestor;
scratch sources remain in their separately addressed allocations.

Earlier groups cannot overwrite later input sources: r(b) <= bB. Later groups
cannot overwrite an earlier group's unsnapshotted sources, which lie below
that earlier destination's end. Group destinations are pairwise disjoint.
Within a group, valid prefixes are processed in increasing logical order, with
a split at the halo boundary if needed. Forward memmove cannot destroy the
next authoritative source in that group. Scratch reads are disjoint from all
input destinations.

A group invokes its child visitor only after its entire destination is valid.
No remaining group source or destination touches that interval. Its contiguous
semantic buckets may therefore be sorted while other groups still compact.
This permits Send-only ownership transfers without sharing references to T.

## Panic boundary

The old Sorter directory ceases to describe authoritative ownership during
squeeze; running its legacy compactor then would be wrong. The dedicated guard
instead owns all committed-prefix cursors. Prefix copy plus cursor commit has
no allocation, callback or panic point. Recovery resumes unfinished prefixes,
never completed/published groups. All group jobs and child calls join before
remaining copies are finished, the whole input is marked materialized, and
saved payloads/destructors run. This is separately tested under native Rayon
and under serial/scoped-thread strict-provenance Stacked Borrows Miri.

## Explicit record-cell budget

For caller worker budget P, the policy fixed before measurement is

    K = min(32 P ceil(sqrt(n)), floor(n/8)).

The constant 32 is independent of P. Candidate Q is at most min(P,R). Equal
semantic-bucket cuts must also satisfy largest group <= 2 ceil(n/Q). Sum every
raw halo cell and check arithmetic/layout before movement. Accept only actual
snapshot Vec capacity <= K. Reduce Q if necessary; if no Q >= 2 passes, use
the existing serial squeeze. Rejected allocations are dropped before fallback.

Thus incremental record storage is bounded by 32 P ceil(sqrt(n)), with an
explicit n/8 guard against a new full-input record buffer. It is not a hidden
Q times P_root R B allocation and does not use an O(n) prefix-copy-op table.
The capacity claim excludes allocator size-class slack and bookkeeping not
reported by Vec.

Existing root scratch has nominal 2 P_root R B cells. With record width t and
f = max(floor(1024/t),32,1), the current block rule gives

    B = max(ceil(sqrt(10n/(P_root R t))), f)
    2 P_root R B <= 2 sqrt(10 P_root R n/t) + 2 P_root R (f + 1).

For fixed supported radix and record width, P_root <= P and n >= 1, root scratch
plus the accepted halo remain O(P sqrt(n)) with a constant independent of P.
Large radix/floor constants at small n remain real costs; reducing Q cannot
remove root scratch already allocated by scatter.

Metadata adds O(Q) groups/jobs/cursors/panic slots, O(P_root) raw segment
addresses, and O(P) boxed visitors. Existing directories, partial descriptors,
semantic ends, segment headers and retained caller storage remain live. The
transient Group and Job vectors can coexist during adoption. Every visitor
owns a child workspace, which may allocate while other groups still copy.
Those capacities and allocator bookkeeping must be included in a complete heap
measurement. The old permutation's temporary buffers have already been freed.

## Observed 10M/W8 geometry

Separate diagnostics on the frozen candidate confirmed R = 256, P_root = 8,
Q = 8, cap = 809,728 cells and eight actual movement workers for both types:

- u64: B = 128, snapshot length/capacity 457,279 cells, 3,658,232 bytes.
- Row16: B = 64, snapshot length/capacity 231,615 cells, 3,705,840 bytes.

Each has 4,194,304 nominal root-scratch bytes. These are allocation components,
not complete operation peak heap; concurrent child storage and metadata must
not be omitted. The diagnostics matched the pre-implementation reconstruction.
