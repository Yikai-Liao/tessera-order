<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Asynchronous rank exchange: source-only model

Status: source prepared, never parsed, built, or executed. This is a safe Python
token/record model, not a Rust mover, performance predictor, or accepted candidate.
The existing sealed-Copy root is unchanged. No shared repository was edited.

## Invariant and transaction

Let F=floor(n/B), M=F+scratch_blocks, and P be fixed worker slots. Each logical
full-block token d contains the B record identities at stable output ranks
[dB,(d+1)B). The initial directory is injective. `pos[d]` is atomic in the future
kernel; `inverse[s]` and physical payload are protected by physical lock s.
Positions and inverse agree whenever the relevant endpoints are unlocked.

The sole static target owner loads speculative s=pos[d], locks min(d,s) then
max(d,s), or one lock if equal, and revalidates BOTH pos[d]==s and inverse[s]==d.
No payload is read before that revalidation. A failed attempt releases locks and
retries. A free destination uses one block copy. A live displaced token x uses
one private B-record temporary and three block copies. Each copy is split into
record microsteps in the model. All four position/inverse stores are separate
microsteps, with four representative store orders. Both locks remain held until
all copies and map stores finish. Identity targets also lock and revalidate.

Unlocked endpoints again contain exactly the mapped token. The temporary is
authoritative for x while its original destination is being overwritten; neither
endpoint is available for another validated transaction during this interval.
The model uses only safe values and explicit scheduling, never raw T storage.

## Retirement and early children

After token d reaches physical d, no other logical token has d as a valid source.
Only d's static owner targets d. A future contender with a stale source d may
lock/read metadata but cannot pass inverse validation, so it cannot read or write
the payload. Metadata allocations and locks must remain alive until all root
workers join, even when their payload has been given to children.

Only the static target owner marks d retired, after unlock, exactly once. A token
incidentally put home by another swap still needs this owner's identity check
and accounting. Every root payload access asserts that its physical block is
not retired. Children destructively change record markers, including singleton
buckets, and stably reorder repeated lower keys within their contiguous bucket.

For each worker/bucket pair, local remaining counts include every owned full
target block intersecting that nonempty bucket. A shared boundary block can
contribute to many tiny buckets. Only a transition of that local count to zero
performs one global AcqRel decrement. The last contributor runs the child inline,
with no locks held. A ghost release-chain bitset checks that the last decrement
has acquired every contributor's publication. This is a model of the required
memory-order argument, not verification of compiled Rust atomics.

The reserved short-tail source is a distinct scratch physical ID excluded from
all full sources and full destinations. Its initialized prefix is copied once
to disjoint original-tail storage, after the tail owner's range completes. This
does not require all other workers to finish. The tail contributes to EVERY
nonempty bucket intersecting [FB,n), potentially several tiny buckets, before
their readiness counters are initialized. Padding is never read. Empty buckets
are vacuously complete and perform no child payload work.

## Progress and limits

Strict physical-ID lock order excludes a wait cycle. It does not provide fair
mutex acquisition, wait freedom, bounded contention, or a real-time guarantee.
Assume eventual scheduling/acquisition, finite critical sections, and finite
children. Every successful nontrivial transaction permanently fixes its target,
so there are at most F such transactions. A current-source speculative read can
fail revalidation only after another transaction changes its source token.
Once changes stop, a fresh read succeeds. No child retains a root lock or waits
for another bucket, so pausing its worker creates no root resource dependency.

True return-to-an-old-location ABA for one token is ruled out by monotone fixing:
each displacement vacates a target that becomes permanently fixed. The directed
model nevertheless forces the dangerous ABA-shaped situation: a saved source
number now names a different, retired, child-mutated token. Optional historical
pos-value injection is separately labeled, bounded, and is not an exhaustive
weak-memory proof. The exact scheduler is an SC interleaving model; it explores
speculative loads during copy and individual map-store steps.

Successful copy work is recomputed per schedule: B*(direct+3*swaps)+tail, at most
3FB+tail. Old wave-broker totals cannot be reused. None of these facts establish
a speedup; extra copies, lock traffic and false sharing can reject the kernel.

## Explicit production capacity ledger

Let S=PR+Q scratch blocks, Q<=PR, and W(m) be one ordinary child workspace's
retained bytes for its maximum assigned child. Root record storage is (S+P)B*T
bytes, before adding up to P child workspaces. This record storage remains live
while children run. There is no production-sized n-record output buffer.

Conservative metadata, excluding allocator/header overhead, is:

- Atomic positions: (F+tail_present)*sizeof(AtomicUsize).
- Inverse: M*sizeof(usize).
- Compact physical locks: M*sizeof(AtomicBool), not M padded mutexes.
- Local completion counts: PR*sizeof(usize), plus R AtomicUsize writer counts.
- Root directory if not actually reused/dropped: ceil(n/B)*sizeof(usize).
- Existing boundary_map: ceil(n/B)*sizeof(usize).
- Existing free-pool capacities: up to (F+PR)*sizeof(usize).
- Existing counts/starts/cursors, worker descriptors, bucket ends, capacities,
  and all child W(m) allocations must also be charged at their overlapping peak.

On a concrete 64-bit layout with 8-byte AtomicUsize and 1-byte AtomicBool, the
new pos+inverse+lock ledger is about 17F+9S bytes (plus a tail position), not
64M bytes for locks alone. At F=156250, those F terms are 2,656,250 bytes; the
locks alone would be 156,250 bytes plus S bytes. Three retained F-word arrays
(directory, boundary_map, free pools) add about 3,750,000 bytes before PR terms.
These are conditional arithmetic examples, not measured allocation results or
a claim of meeting the few-times-serial cap. Rust size/alignment, actual vector
capacity/lifetimes, scratch, and all concurrent workspaces must be measured in
the proposed implementation. Compact locks do not eliminate false sharing.

With fixed P,R and balanced B, payload O(PRB) and metadata O(n/B+PR) have the
intended square-root form; explicit P,R constants and aggregate peak remain
mandatory. The Python model intentionally stores complete record identity and
state snapshots for validation. Its storage is NOT an implementation budget.

## Panic boundary, not yet solved

There is no user call, Rayon call, allocation, bounds panic, or deliberate
fallible operation in a production lock-held transaction. All sizes, indices,
pointer provenance, capacity and arithmetic must be prevalidated. The existing
Memory::ptr has an assert, so it cannot simply be called in this critical path
without establishing a non-panicking variant. Numeric physical adjacency never
proves two allocations contiguous.

Children execute only after unlock. An actual exposed safe API needs a separately
reviewed Copy recovery policy: catch child unwinds, retain the first panic,
finish all remaining root restoration, skip later children if appropriate,
join all workers, then propagate. Unexpected task/scheduler failure also needs
an owner that completes restoration before any return/unwind exposing invalid
input. This model does not simulate panics or establish that recovery. It does
not genericize owned T and makes no owned-value panic-safety promise.

## Coverage and replay

`directed` forces a 3-cycle schedule with at least one failed validation, at least
one retired metadata lock, and at least one child before all roots retire. All
three are assertions, not optional statistics. `matrix` covers P=1,2,3,8,
identity/cycles/open paths/free slots/scratch/tails/empty/tiny/shared boundaries,
four map-store orders, and fair/random schedules. `exhaustive` covers all small
F<=2 injection directories plus selected shared/tail cases, returning exit 2 if
any state cap truncates the graph. Only exact:true is an exhaustive pass.

Replay accepts frozen directory shapes in JSON. A bounded source-tree search
found no ready directory dump; aggregate evidence JSON is not a directory.
Current replay coverage is therefore a gap, not a pass. No costly regeneration
is authorized or required for this first safety gate.
