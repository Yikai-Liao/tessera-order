# Owned u64-key rank-merge research branch

Narrow u64-key research prototype: initial debug/native and ordinary Miri checks
passed as recorded in VALIDATION.md. Not benchmarked or production-ready.
This isolated crate derives from the BSD-2-Clause Copy research implementation;
that workspace and all its frozen evidence remain unchanged. The proposed API
supports T: Send and F: Fn(&T)->u64 + Sync, not arbitrary RadixKey/u128, FnMut,
or cached-key semantics. No performance advantage is asserted.

## Ownership ledger

1. Local generic Tessera sorting and incremental monotone rank cuts execute
   while the original input owns all records. Existing Tessera guards handle
   local key panics; rank-planning panics have no moved-record holes.
2. Allocate checked scratch, all job/free/output metadata and restoration plan.
   Extract every initial head key from ORIGINAL input addresses before moving
   shared source blocks. Initial-head panics still leave every T in the slice.
3. Arm the internal-unwind backstop, then logically move cut blocks into
   MaybeUninit snapshots. Original copies become stale bytes. Snapshot record
   intervals belong to distinct jobs; two jobs never borrow the same T. This
   permits T:Send without T:Sync, including Cell and exclusive references.
4. Each step reads exactly one cursor-owned T, advances the cursor and frees
   only a fully consumed exclusive block, writes the T into an owned output
   slot, and commits fill/remaining/output-directory bookkeeping. Only THEN
   may the next user key be called.
5. The refresh-key call has a per-job catch_unwind. On failure its payload is
   retained and that job drains nonempty input cursors in run order, without
   calling any more keys. Existing seed capacity must cover arbitrary drains;
   source cursors, current output prefix and full output directory are the
   authoritative recovery ledger. No T is left in an unrecorded local variable
   when a user callback is invoked.
6. All jobs join. Full blocks/tail are logically moved back using bounded
   snapshots and disjoint restoration worker lists. Stale scratch is stored in
   MaybeUninit and is never dropped. Disarm the guard only when the original
   slice owns a complete permutation. Then resume one captured panic (first by
   job index, not a claimed temporal ordering across threads).

The abort guard is ONLY a last-resort backstop for unexpected implementation
invariant failures while records are moved. It must not substitute for recovery
from ANY normal user key panic, including initialization, moved-merge refresh,
and nested Rayon calls. It remains unarmed until all fallible pre-movement key
calls/allocations finish. No locks or worker-index leases are held during keys.
Standard Rust behavior still applies if panic payload destructors themselves
panic; records must already have been restored before any such destruction.

## Required verification before claims

Native tests must demonstrate exact String/wide payload permutation, exact
once-only drops, Send-but-not-Sync Cell records, exclusive &mut records, all key
phases, nested Rayon keys and inconsistent keys. Moved-merge tests use an explicit
phase gate and require catching the resumed panic in-process with phase=restored;
an aborted process is a failed test, never an acceptable recovery result.

The focused ordinary-Miri suite uses both serial and scoped std-thread merge
execution of the exact step kernel, avoiding the separately known Crossbeam
Stacked Borrows limitation. Production parallel execution remains Rayon. Generic
restoration movement also requires ordinary-Miri owned/exclusive-reference
coverage; a Copy-only restoration test does not establish that obligation.

Remaining proof/coverage work beyond the focused passing tests: broader
source-block scratch provenance coverage with &mut-containing T;
commit ordering under all phase-gated key failures; arbitrary-drain seed bound;
no same-record key sharing for Cell; record ownership through segmented cycles,
shared snapshots and partial tails; checked layouts and ZST fallback. ZSTs explicitly stay on generic Tessera
without entering raw rank movement. The first native/Miri checks now pass, but do not establish a full generic API
or exhaustive correctness over all inputs and panic positions.

Source test inventory: 216 owned stable cases, 36 phase-panic cases, three
inconsistent-key schedules, four exclusive-reference/recovery combinations,
nested Rayon panic recovery, layout overflow/ZST drop checks, six focused
owned-Miri sorting/recovery cases, and a separate scoped restoration fixture
using both String/Cell/drop-counted records and exclusive references. These checks passed in the initial debug/native suite, with the focused owned
Miri targets passing separately. Counts are derived from final source loops in
VALIDATION.md. Release and cached Rust 1.85 checks now pass; see evidence/multi-panic-validated-source/VALIDATION.md.

## Seed bound under arbitrary recovery order

Let K be the source-run count and B the physical block capacity. A worker owns
one contiguous interval in each run, and consumes each such interval monotonically,
even when recovery switches runs in an arbitrary order. For each run:

- At most its two interval endpoint blocks are unrecyclable snapshots. If both
  endpoints lie in the same block, count that block once. The final global input
  tail is included in the right endpoint category, not counted as a third block.
  Across K runs these contain at most 2KB worker-owned records in total.
- Among non-snapshot blocks, at most one per run is partially consumed. This
  category excludes every endpoint snapshot above. Their consumed prefixes total
  strictly less than KB. Every completely consumed non-snapshot full block has
  already joined that worker's free pool.

If m records have been read/consumed and q exclusive blocks have been released,
m-qB is exactly consumed records from these unreleased categories, at most 3KB.
This statement depends on monotone per-run cursors, not key order or which run
wins next. Packed output needs at most ceil(m/B) blocks, including its one current
partial output block; subtracting the q recycled blocks leaves at most 3K seed
blocks needed. The implementation reserves 3K+2, retaining two blocks of slack
for boundary/commit accounting. A record may briefly be in a local variable
between read and output commit, but no key is called in that interval; counting
that record in m also counts the output slot it is about to require.

Original slots of snapshot blocks are never recycled, and snapshot storage is
never added to a free pool. They therefore cannot accidentally contribute to q.
Non-final output tasks have B-aligned rank endpoints, so recovery introduces no
additional per-task tails. The native recovery tests and focused Miri cases passed without starvation.
Those finite cases support, but do not replace, this general capacity argument.


## Checked layouts, compatibility and callback audit

Before constructing any record pointer, the code checks run-boundary products,
seed-count arithmetic, total scratch blocks, extra*B record count, byte/isize
capacity through Layout::array<MaybeUninit<T>>, address-table size, tournament
size and free-list capacities. Standard Vec allocation checks cover metadata.
Reserved restoration snapshots/temps use checked 8P/9P and base arithmetic.
All subsequent raw pointer offsets are bounded by those allocated capacities;
T alignment comes from Vec<MaybeUninit<T>>, not an integer-address reconstruction.
The final input tail is snapshotted and is never treated as a full writable block.
ZSTs stay on existing generic Tessera rather than entering rank movement.

The used checked arithmetic, Layout::array, div_ceil and scoped-thread APIs are
available by Rust 1.85, which the Tessera path dependency declares. Actual checks
so far used stable rustc 1.99.1 and the installed nightly Miri. A later cached Rust 1.85 offline build and all 12 native groups passed; release
and all three owned Miri targets also passed. See the second validation record.

All callback sites have been classified: local sorting uses Tessera's generic
recovery before our guard is armed; rank cuts and initial heads call keys while
original slice ownership is intact; moved-merge refresh calls are caught only
after commit. Loser replay, draining, snapshotting and restoration never call
keys. No mutex or worker-index lease surrounds a callback. Native nested Rayon
panic testing passed and restored the input before resuming the user panic.


## Uncaught-unwind boundary and new multi-panic check

MovementGuard is not an all-exception recovery mechanism: an uncaught internal
or scheduler unwind after movement starts aborts. Every normal user key panic,
including one propagated by nested Rayon inside a callback, must be caught at
its callback boundary or occur before movement. It must drain/restore/resume,
not rely on this guard.

A passing test now makes each of three jobs independently panic on
its first moved-merge key refresh. Drop-counted payloads record, without panicking
inside Drop, whether destruction happened before phase 5 or after any input
record was dropped. The caller verifies the restored original permutation before
dropping the externally resumed payload, checks nonselected payloads already
dropped only after restoration, and finally checks every payload/record drops
exactly once. Native scoped/Rayon schedules and the scoped ordinary-Miri target passed in
the second validation window. The first validated freeze is unchanged.
