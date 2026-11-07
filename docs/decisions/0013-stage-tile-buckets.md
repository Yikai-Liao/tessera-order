# 0013: Stage tile-sized key buckets instead of compacting them twice

- Status: proposed
- Date: 2026-11-07

## Context

A block partition scatters records into a block pool and then compacts each
bucket into the input slice, because the recursion continues on contiguous
slices. For a bucket that fits one tile, that copy is immediately followed by
the tile sort, which reads the bucket again, scatters it into its own scratch
buffer and copies the result back. Three of those four moves touch records that
are already in the cache of the core that wrote them.

## Decision

A tile-sized bucket of directly sorted keys is copied into a staging buffer
instead of the input slice, and the visitor receives both the staged records and
their destination range, so the leaf scatter writes each record once, straight
to its final position. The block directory is updated exactly as for the
compacted path, and the evacuation of live input blocks is unchanged.

## Measured

Interleaved A/B with two identical copies as control, nine blocks, median of
`candidate / mean(A1, A2)`, faster is positive:

| Case | Speedup |
|---|---|
| `u64/random/1000003` | 1.9% |
| `u64/random/10000003` | 1.6% |
| `rec16/random/10000003` | 6.6% |
| `u64/runs/1000003` | 1.4% |

The staging buffer costs up to 64 KiB of workspace. Records sorted by a key
function keep the compacted path, because a key function can panic after the
destination range was written.

## Alternatives measured and rejected

| Candidate | Result |
|---|---|
| First digit of 12 bits, one block pass at 10M | 19% slower: 4,096 write cursors and a 10 MB pool of open blocks |
| Tile cursor table at 4 or 1 records per bucket | 13-184% slower: insertion pays more than the table saves |
| Finish only the buckets that hold two or more records | 33% slower: the boundary walk costs more than the scan it avoids |
| Log multi-record buckets in the histogram and finish those | 16-36% slower: two 32 KiB tables per tile plus a store in the hot loop |

## Consequences

Direct keys gain a second materialization protocol in the block engine. The
compaction copy itself stays: the records still have to leave the block pool for
the input slice, and only the duplicate move disappears.
