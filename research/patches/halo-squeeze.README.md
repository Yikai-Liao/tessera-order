<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Bounded parallel halo squeeze checkpoint

## Status and exact application base

This source-only research checkpoint preserves a correctness-tested mechanism,
not an accepted performance improvement or a production default. Neither the
initial pilot nor the corrected exact-entry pilot establishes an end-to-end gain.
The negative/inconclusive [screen record](halo-squeeze.SCREEN.md) retains both
pilots, all outliers, exact control envelopes and the validation limitations.

Apply `halo-squeeze.patch` **after** the archived `combined-rayon.patch` on its
historical base commit `60873c86558456aca496ca8c4e084007415b58b1`. Do not apply it
to current SIMD main, or combine it with the prefetch or root-budget patches
without a separate rebase and validation.

- Prerequisite combined patch SHA-256:
  `ac27669d2009d1b0fdadf10535beb9edb3c29d984304f0181a1512bbd09ae751`.
- This follow-on patch SHA-256:
  `260ce0be15152fe615ced3242f631f67964fd758888d04e0f030cbb4a98b69b3`.
- `halo-squeeze.base-hashes.txt` identifies the four existing files before
  application; `block/parallel/halo.rs` must not yet exist.
- `halo-squeeze.source-hashes.txt` identifies all five resulting source files.

An independent archive of the historical commit was patched with the combined
patch and then this follow-on. Apply-check, actual application, all source-hash
comparisons, and reverse apply-check passed. The resulting Rust bytes exactly
match the frozen halo prototype. This packaging operation did not compile,
format, benchmark, edit the shared checkout, register a worktree, commit, or
push. See `halo-squeeze.apply-check.txt`.

The source prototype package was `tessera-consumer-wait`, with library name
`tessera_sort`. The benchmark copy changed only its package identity for linking.
The historical repository package already exposes `tessera_sort` and the same
optional Rayon feature. No Cargo manifest, lockfile, crate alias, dependency,
algorithm parameter, or Rust source normalization is exported here.

## Mechanism and controls

The native root scatter and whole-block permutation remain. A new opt-in path
replaces final serial squeezing with independent semantic-bucket groups. Each
group stages threatened input bytes into a bounded raw halo, compacts forward,
then runs ordinary contiguous child Tessera sorts on its completed buckets.

`TESSERA_HALO_SQUEEZE=1` selects this schedule only at the existing multi-digit
parallel-root route. Leaving it unset retains the archived combined pipeline.
Small, single-worker, equal/outlier, and single-digit routes are unchanged.
Canonical whole-block permutation is mandatory on the halo path, regardless of
the old `TESSERA_COMPACT_SERIAL` diagnostic setting.

The old shared non-Send `FnMut` publication callback is not called concurrently.
Instead, each group owns a preallocated Send visitor containing its extractor
clone and ordinary child workspace. All extractor clones and visitor allocations
precede movement. There is no waiting-consumer queue on this path, so a nested
Rayon join cannot steal a consumer waiting for its own producer to finish.

When no Q >= 2 plan fits the budget/balance rules, this path uses the existing
serial squeeze and one local child visitor. It does not reactivate the old
consumer queue. This distinction matters to the two legacy injection probes
listed below and to any performance comparison.

The public signatures and adapters (`Direct`, `ByKey<&F>`, `Narrow`) retain
their existing bounds: records require Send, not Sync, Copy, or Clone. This is
not a new public promise about arbitrary private extractor Clone/Drop behavior.

`TESSERA_HALO_STATS=1` prints root dimensions, each candidate Q/raw-cell budget
and balance decision, explicit fallback, accepted snapshot capacities, partial
metadata capacities, and movement-worker count. It is a diagnostic, not a
public Stats API or complete heap counter. Leave it unset for timing.

## Ownership and recovery

The complete movement and record-space argument is in
`halo-squeeze.SPACE-AND-OWNERSHIP.md`. The implementation never forms a typed
reference to padding. Authoritative threatened records transfer into raw
`MaybeUninit` halo storage before any group writes; their old bytes are stale.

A dedicated recovery guard replaces the old directory's recovery responsibility
once squeezing starts. Every group has a preallocated committed-prefix cursor
and panic slot. No allocation, callback, or panicking operation occurs between
a raw prefix copy and cursor commit. The whole group is materialized before
any child callback runs. Published intervals are never replayed from old sources.

Child and injected movement panics are caught separately per group, so Rayon's
first-panic selection cannot drop another group's payload prematurely. Workers
and child calls join, remaining copies finish without callbacks, and only then
may saved payloads or visitors drop or a panic propagate. Cancellation is
cooperative: callbacks already admitted elsewhere may finish. Scratch is freed
normally; no forgotten allocation is used to avoid ownership recovery.

## Validation, with build profiles kept distinct

These results apply to the frozen originating source, not to a new build of
this metadata-only packaging:

- **Debug, default schedule:** 51 unit plus 21 integration tests passed,
  72 top-level tests total. The unit set includes ten explicit halo tests.
- **Debug, opt-in public suite:** 10 of 12 tests passed. Two preserved
  queue-specific mandatory-worker injection tests fail as described below.
  A rerun of the ten applicable tests passed. This is not an all-pass opt-in
  suite.
- **Release, halo-only filter:** the benchmark owner ran all ten halo tests
  successfully on the exact frozen Rust source. Other integration tests were
  filtered out. This is not a full release regression suite.
- **Miri:** all eight selected halo tests passed with default Stacked Borrows
  and strict provenance. They execute the same movement/run kernel using
  serial and `std::thread::scope` schedules. Actual Rayon execution is excluded;
  this does not claim that the Rayon/Crossbeam dependency path is Miri-clean.
- **Clippy:** all features/all targets with warnings denied passed.
- **Documentation:** six doctests and rustdoc with warnings denied passed.
- **No-default-features:** check passed.
- **Rust 1.85:** all-feature check passed, with one inherited unreachable-pub
  warning in `key.rs`. MSRV tests were not run for this halo checkpoint.

The focused tests cover stable references for random, sorted, reverse, equal,
low-cardinality and skewed data; partial/block/group boundaries and scratch
segment crossings; cap fallback and checked overflow; owned String/Cell records
with exact drops; exclusive mutable references; prefix/scheduler/armed-guard
panics; simultaneous payload destructors; nested Rayon and a full-size nested
public sort with an operation-thread accepted-Q witness.

The distinct-group key-panic test asserts Q = 2, starts ordinary child Tessera
sorts, and injects only after a child key observes a record displaced from that
group's initial position. Both payload destructors inspect the restored input.
Its native children fit tiles; it is not fresh child block-route coverage.
Independent static review found no concrete remaining production safety or
progress defect, but is not a complete proof of all unsafe Rust behavior.

### Two retained opt-in injection mismatches

`delayed_worker_movement_panics_preserve_owned_records` and
`delayed_worker_movement_panics_preserve_exclusive_borrows` mandate enough
moved-record key calls on a non-origin worker. Actual route diagnostics show:

- Their uniform cases accept Q = 2 and all three thresholds 1/7/17 inject and
  recover. Owned records use B = 32, R = 256 and 4,100 halo cells; borrowed
  records use B = 64, R = 64 and 2,052 cells.
- Their next concentrated case has n = 131,072, P_root = 2, radix = 12,
  shift = 52 and R = 4,096. Its Q = 2 halo requires 65,233 cells, above the
  fixed cap of 16,384, for both record layouts. Balance passes; space fails.
- The documented serial fallback has no obligation to run child keys on a
  different worker. The mandatory injection-count assertion therefore fails
  before the test reaches its subsequent record-validation assertion.

An independent reconstruction matched those runtime dimensions and counts.
This is recorded as an injection-assumption mismatch, not a passing test and
not evidence that those skipped assertions were executed. The separate explicit
Q > 1 phase-specific recovery tests retain their own narrower evidence.

## Verified screening outcome

Both pilots cover only 10M random seed-1 u64 and Row16 at W4/W8, three measured
blocks per case. Each verifies 84 measured and 28 warmup full stable references.
The initial Row16 H1/H2 entries had equivalent but separate closure
monomorphizations, so their ratio is not exact-entry A/A. The corrected pilot
uses one entry arm; the algorithm source and allocator binary are unchanged.
No candidate source tuning or broader matrix was performed.

No case supports adoption. Corrected u64 W4 is slower in all three blocks:
paired median H/C = 1.453, beyond the C-repeat-plus-placement envelope 1.229.
Unexplained long candidate samples broaden the candidate-repeat envelope to
3.151; that does not explain or excuse the slowdown. Corrected Row16 W4 is
also slower in all three blocks. W8 does not establish a gain. The full report
preserves separate independent medians and within-block ratios rather than
reconstructing one from the other, and never discards the long samples.

Separate counting-allocator measurements found these additional-live-requested
heap peaks for eight workers:

- u64: combined 6,168,395 bytes; halo 9,191,622 bytes; increase 3,023,227 bytes.
- Row16: combined 7,971,658 bytes; halo 10,351,952 bytes; increase 2,380,294 bytes.

At four workers the increase was 600 bytes for each type. All 16 allocator
case/variant observations ended with zero retained-byte delta. These are
operation peak requested heap, not snapshot-only bytes, RSS, thread stacks or
allocator bookkeeping. The screen is a mechanism rejection/retention record,
not a claim that every possible halo schedule fails.

The verbatim report in `halo-squeeze.SCREEN.md` has SHA-256
`66bbe5a578f09c4a15299523a7adea2b8a85975eccf462841c10735d971114b9`.

## Source and evidence identity

Frozen source-manifest SHA-256:
`c66e83aeee558b31312895b51ba284f3a4f1a7df370b9147cef3b44cf21a287e`.
The original source archive SHA-256 is
`195ad328a4da3c10e25b1f19b2a18b267e9e6495d038c61f6406bf1ceeedfd2c`.
The optimized halo-only test log SHA-256 is
`6ba35cb015de805c86f3cbe2cbe60fc5bba92a80da23a6341daa99acaa0f2d4f`.

Raw logs, measured samples, allocator records, harnesses, targets, archives and
binaries remain outside this repository packet. Only source changes, concise
review notes, proof, identifiers and application metadata are exported.
