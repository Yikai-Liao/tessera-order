# Exact-rank region/block research prototype

This isolated BSD-2-Clause experiment has two deliverables:

1. A read-only dependency probe over **actual aligned stable-rank root output**.
2. A working bounded-space exact-target exchange rearranger. It is not a
   production API or an accepted performance optimization.

The original aligned histogram/rank/scatter and whole-cycle restore control are
reused from the repository's original BSD-2-Clause research implementation.
`probe.rs` and `broker.rs` are original code. No PBBS, Parlay, or Regions source
was copied or transliterated.

## Scope and stable ordering

Only sealed `u64`, `Row16` and `Row128` Copy records with an intrinsic pure key
are supported. Row16 and Row128 carry an original ID; Row128 also has 112 bytes
of payload. Their exact stable partition/full-sort outputs are checked against
stable reference arrays by full equality. Owned values, Send-but-not-Sync
records, inconsistent extractors and panicking callbacks are **not supported**.
There is no public callback API. An allocation/scheduler panic after movement
is not promised to retain a permutation, although Copy values remain valid.

Stable rank is assigned by bucket, then original contiguous shard, then original
record order within that shard. Record ordinals are test payloads, never a
per-record sorting allocation. The rearranger only moves exact already-stable
whole-block tokens; equal bucket labels are not enough to authorize a swap.

## Real dependency probe

At the scatter/restore boundary, `directory[target]` names the unique physical
source of the target's exact final records. The probe records source and target
intervals, fixed blocks, exact reciprocal pairs, open dependency paths, cycles,
component sizes, one independent exchange wave, and the original cycle
scheduler's job assignment, snapshots and bucket retirement curve.

Source coordinates in interval CSVs are **physical-block-ID coordinates**:
IDs below `full` refer to input full blocks, higher IDs refer to scratch. They
are not a claim that scratch occupies input addresses. The short tail is a
separate initialized fragment in reserved scratch.

The equal-speed model is not a CPU timing. It preserves the control's greedy
node-count scheduling and counts per-job block copies, including short-cycle
temporary copies. Long-cycle boundary snapshots are a separate prerequisite
before time zero. The tail costs one conservative block-copy step after all
jobs join. A bucket is ready only after every full physical target intersecting
it has retired, including shared bucket boundary blocks and the tail.

The diagnostic interval/component/job arrays are O(n/B + PR) machine words.
They are not needed by the candidate and are disabled for timing and memory
runs. No diagnostic full record array is part of either rearranger. Input and
stable-reference arrays exist only in the external research harness.

## New exact-target broker schedule

For each bucket's increasing target-block prefix:

1. Scan unfinished target blocks and choose endpoint-disjoint target/source
   exchanges. A physical endpoint appears at most once in the wave.
2. Update the target-to-source directory and inverse live-token map for the
   planned exact-token exchanges. When a target has no live displaced token,
   use one direct copy instead of a three-copy exchange.
3. Run the independent jobs in parallel, each with a private B-record temporary.
4. Join the wave and repeat until all targets intersecting the bucket are fixed.

Every successful exchange permanently fixes at least one target. There are at
most F=floor(n/B) exchanges, at most 3FB+tail copied records, and no snapshots.
Job vectors have total capacity at most F+P. Their capacities and all guards are
allocated before restore movement; later waves reuse them.

A finalized block has directory[i]=inverse[i]=i. Since the map is injective,
no unfinished target can read that block. Future jobs only write unfinished
targets and their sources, so finalized blocks cannot be written again either.
There is no deferred endpoint read. This proves root retirement after a joined
wave. This version measures retirement but **does not run children concurrently
with remaining root work**. It must not be advertised as a pipeline speedup.

The price of earlier retirement may be more copies, serial planning and many
wave joins. Actual timings, wave counts, batch sizes, tails and negative cases
must be reported, not only the retirement graph.

## Space, including constants

Let P be actual shards, R=2^radix_bits, B be block records, F=floor(n/B), and
Q<=PR the distinct shared segment-boundary/tail scratch blocks.

- Stable rank/scatter: (PR+Q)B <= 2PRB record slots.
- Broker restore: PB additional record slots and no snapshots.
- Original cycle control: PB temporaries plus at most 8PB snapshot slots.
- Directory, inverse, free pools, histograms, cursors, job descriptors and
  touched generations: O(n/B + PR + P + R) machine words.
- Report readiness arrays: O(R) counters. Probe-only arrays are separate.

Thus broker payload scratch is at most (2PR+P)B; cycle control is at most
(2PR+9P)B. These bounds count all allocated scratch blocks, even dead or unused
ones. For fixed radix, B balances record storage against n/B metadata, subject
to floors of 32 records and 1024/sizeof(T) records. The floors and R dependence
matter at small inputs; this is not a universal small-constant memory promise.
With this explicit B policy the asymptotic total is O(P sqrt(n)) for fixed R,
not O(n). There is no n-record Tmp, output, linear ordinal array or sqrt(n) by
sqrt(n) bucket matrix. Allocation high water is measured separately, including
Vec capacities and root metadata; timing is never inferred from that run.

## Validation and reproduction

Run Cargo offline with one build job and hold shared FD9 for all build/test
processes. Timings hold exclusive FD9. Never replace the common CPU lock.
Focused Miri uses default Stacked Borrows and strict provenance. It substitutes
scoped std threads for the Rayon executor only; planning and the movement
closure are identical. Miri's explored schedules are not an exhaustive race
proof and do not validate the Rayon dependency implementation.

The external harness/script/data stay outside Git. `VALIDATION.md` records
exact commands and source identities. This crate is research only; changing
its child dependency from a historical core to current main is recorded and
is not claimed to be a measured improvement by itself.

## Follow-on: direct open-path restoration policy

`Config::path_restore` selects true open paths plus whole-cycle restoration.
This is a new controlled use of established pull-path machinery on the aligned
root, not a claim to invent path/cycle permutation. Earlier original research
controls include `tessera-rank-merge-prototype`'s `restore_pull` and
`parallel_restore.rs`; no foreign source was used.

A source-unique directory gives each full target one outgoing dependency and
each physical block at most one incoming dependency. A full target with no
incoming edge contains no remaining live block token. Its chain must end in a
scratch block. Different paths and cycles have no shared physical source or
target, including scratch endpoints. A worker reads each next source before
overwriting that source on its next step. A closed cycle protects its initial
source in the worker's one reusable temporary. Fixed points need no copies.
The partial tail is explicitly excluded from every full source and copied only
after all workers join.

This **whole restoration-policy change** removes artificial spare completion,
omits normal-path splitting, and assigns complete components by descending
copy length to the least-loaded worker. LPT sorting, allocations and planning
remain inside root timings. If any component exceeds 2*ceil(F/P), the plan is
dropped before any record copy and the original bounded-snapshot split-cycle
control runs instead. The report flags that fallback. Its 9PB temporary field
is an upper bound, and its zero `max_job_copies` means unavailable, not zero
work. The four measured plans use the direct route with no fallback.

The direct route allocates PB temporary record slots and no snapshots. Node
and component lists, source-uniqueness/visited flags and worker lists require
O(F+PR+P) metadata words. No record-sized allocation has F entries. The fallback
retains the original bounded snapshot allowance. Current benchmark inputs have
many independent paths, but a single giant path/cycle is explicitly tested both
on direct P=1 and fallback P=4 routes.

Both `sort` and `partition` accept the policy. `sort` still uses a full root
barrier followed by the same current-main serial children. There is no new
lifecycle/overlap machinery and no adaptive production routing added to make
particular distributions look better.
