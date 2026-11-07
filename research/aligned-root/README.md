# Native aligned root partition research

This is a runnable algorithm checkpoint, not a production replacement. It
stably partitions sealed `Copy` records (`u64` and `Row16`) using an intrinsic
pure u64 key, then sorts real contiguous child slices with the frozen Tessera
serial kernel. It does not implement virtual buckets or an interior-hole squeeze.

## Run

From this directory, run `cargo test --all-targets` and `cargo test --doc`.
The package supports Rust 1.85 and later. Its only registry dependency is Rayon
and its transitive dependencies. The measured Tessera leaf source is vendored;
there is no dependency on another checkout or a machine-specific path.

The public research API provides:

- `partition`: one stable arbitrary-digit partition, returning cumulative bucket
  ends and stage/copy statistics.
- `sort`: root partition followed by existing serial Tessera on real disjoint
  child slices. This requires a most-significant root digit, enforced by
  `shift == 64 - radix_bits`, so concatenation is globally sorted.
- `Config::parallel_restore`: select the serial swapping control or packed
  parallel block-cycle restoration. The default remains the serial control.

This package intentionally contains no benchmark runner, raw measurement data,
profile, executable artifact, or generated build directory.

## Alignment and ownership

1. Count every original input shard by bucket. Prefix sums in bucket order,
   then original shard order, assign each segment an exact final rank interval.
2. Reserve one scratch block for each distinct block containing an unaligned
   segment boundary, plus the final partial output block. Arbitrarily many
   segments can share one such block. Writers own disjoint record intervals,
   not the entire shared block. There are at most P*R boundary blocks.
3. Seed each shard with R scratch body blocks. Scan only that shard's original
   input in order. Write boundary fragments directly to their planned cells.
   Full body blocks come from the seed or that shard's consumed-input pool. An
   original block is released only after its last record is read and written;
   the original short tail is never recycled as a full-capacity block.
4. Per-bucket cursors cache the current write pointer and endpoint. Resolve the
   physical block only on a cursor transition. Cap each endpoint to its own
   segment boundary, including a fragment sharing its physical block with other
   shards, and check complete segment consumption after scattering.
5. The logical output directory has no interior holes. Each full block is fully
   initialized. Restore these packed blocks, then copy the final initialized
   tail from its reserved scratch block. Parallel restoration assigns each
   physical destination one writer and snapshots long-cycle boundaries before
   any parallel writes. The final partial block remains fixed during cycles.
6. Form disjoint mutable slices only after restoration has joined. Fixed-P
   groups reuse serial Tessera workspaces. This public child API repeats child
   preparation; the frozen combined engine's internal path does not.

Stability follows from original shard order in the prefix and original input
order within each shard. Neither job completion order nor cycle order affects
record order.

At the point a body starts, completed bodies account for enough previously
consumed records to pay for their input blocks. At most R active bodies remain,
so R seed blocks cover the not-yet-retired current source block. Boundary writes
only reduce the number of required body blocks.

## Space and panic limits

Record scratch is at most 2*P*R*B plus O(P*B) restoration temporaries. Directories
and free pools are O(n/B + P*R). B balances these terms, with explicit floors of
1024 bytes and 32 records. There is no n-record output allocation in the kernel.

Only sealed, fixed-key Copy records are supported. This is not an arbitrary
callback-key API. A generic two-pass key function can disagree with its earlier
histogram and would require bounded mismatch recovery. Owned records and a
panic-preserving permutation guarantee are unsupported. A panic after movement
may leave initialized but duplicated or unsorted Copy values. Allocation failures
before movement leave input unchanged; later failures do not promise recovery.

All record pointers derive from one input ancestor created after the read-only
histogram, and one scratch ancestor. Input is not reborrowed while raw movement
jobs run. Scratch cells use `MaybeUninit`; uninitialized spare/padding cells are
never interpreted as records or written into a live full input block.

## Validation scope

Native debug/release tests check full stable references across tiny roots,
empty buckets, multiple segments sharing a block, source tails, block sizes
1 through 256, serial and parallel restoration, and Rayon pools of 1/2/4/8
threads. Invalid and overflowing digit configurations are rejected before writes.

Targeted default Stacked-Borrows Miri checks cover the actual movement closures
with concurrent standard scoped threads, including forced short/split cycles
and uninitialized spare scratch. Under Miri only, histogram collection is serial
and movement job execution uses `std::thread::scope`; production uses Rayon.
The Rayon/Crossbeam scheduler has a separately observed Miri retag failure.
These checks do not establish generic-key, owned-record, or Rayon-Miri safety.

## Measured checkpoint, not a performance win

The bounded API-allocation pilot used random 10M-record inputs, 8 workers, root
radix 256, 9 randomized blocks, repeated identical-code combined controls, an
identical-source placement control, and full stable output comparisons outside
each timed region. The checkpoint is the cursor version, with explicit Direct-u64 child routing.
The first two pilot bullets below preserve the earlier cached-key-u64 control;
they do not describe a Direct-u64 baseline. Row16 consistently uses by-key APIs.

- u64: parallel restoration median 33.86 ms; median paired ratio to the frozen
  combined baseline 1.433, winning 1/9 blocks. A/A range 0.826–1.263; placement
  range 0.928–1.217. This case is slower than the combined baseline.
- Row16: median 45.32 ms; paired ratio 0.961, winning 7/9 blocks. A/A range
  0.713–1.107; placement range 0.817–1.660. This is within noise, not a gain.
- Old/current-main by-key serial medians: 93.19/91.36 ms for u64 and 151.14/147.83 ms
  for Row16. Serial controls are reported separately from the parallel baseline.
- Parallel restore copies 10,527,488 record-equivalent cells for u64 and
  10,262,592 for Row16, versus 28,083,200/28,927,616 in the serial-swap control.
  Counts include raw spare-cell transfers; they are not bytes or elapsed times.

The final isolated same-binary u64 control explicitly uses Direct APIs for the
combined and strong serial baselines, and compares the frozen cached-child
version with the new Direct-child version without changing the root layout:

- Direct-child median 34.96 ms; frozen cached-child median 37.41 ms. Their paired
  ratio was 0.823, but only 5/9 blocks favored Direct children. Child-stage medians
  were 13.21/13.70 ms. These do not establish an improvement.
- Direct-child / Direct-combined median paired ratio was 1.495, with 1/9 wins.
  A/A range was 0.872–1.873 and placement range 0.688–5.798, including a severe
  scheduling outlier. No performance win or precise slowdown claim is accepted.
- True Direct old/current-main serial medians were 96.70/100.55 ms in that run.

Direct child routing is retained for API consistency: u64 uses
`Workspace::stable_sort`, while Row16 uses `Workspace::stable_sort_by_key`.
The measured library source is preserved exactly. A separate integration test
adds direct-u64 full-reference coverage without modifying the measured library.

A separate counting allocator measured actual peak additional heap, excluding
input/reference buffers allocated before sorting and including internal metadata:

- u64: aligned parallel restore 8,069,820 bytes; combined 6,168,395 bytes;
  either serial control 1,318,748 bytes.
- Row16: aligned parallel restore 11,258,012 bytes; combined 7,971,658 bytes;
  either serial control 1,849,780 bytes.

The Direct-u64 heap control repeated exactly the same listed peak figures.
The aligned peak is about 6.1 times serial peak at 8 workers. These are measured
heap peaks, not RSS or estimates. The final API retained no additional heap in
that run. Future performance work must account for the extra histogram scan and
loss of root/child overlap; eliminating squeeze alone did not establish a win.

## Source provenance

`SOURCE_SHA256SUMS` identifies the library and exact frozen Tessera dependency
sources. The vendored dependency retains its original licenses, source notices,
and optional parallel modules so the measured dependency configuration is
preserved; only its manifest is made standalone. This does not enable or claim
a new generic parallel public API.
