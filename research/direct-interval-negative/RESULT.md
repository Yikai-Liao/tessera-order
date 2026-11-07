<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Rejected on the first actual native graph

## Decision

Do not implement the direct live-prefix record-moving kernel. The single frozen greedy policy fails its extra-spill cap on the first real full-random Row16 native graph. Its concrete full-graph storage plan independently exceeds the frozen total auxiliary-byte cap. Neither result proves every possible interval mover impossible. No larger cap, alternate victim order, different layout, u64 rescue case, sorting benchmark, or unsafe implementation was attempted.

This is useful negative structural evidence: the apparent opportunity to replace permutation+squeeze with one direct copy does not survive the specified bounded implementation gates.

## Actual result

Input: 10,000,000 stable-ID Row16 records, P=W=8, R=256, B=64. The capture owner continued the native call through permutation, squeeze and ordinary children, then checked full stable reference equality. This probe read only that call's pre-permutation numeric directory.

- 157,253 positive live prefixes and 307,769 exact external interval edges.
- Self-overlap: 14 records, excluded from external dependencies and treated as memmove.
- One nontrivial SCC has 154,030 vertices; 3,223 other SCCs are singletons. This is a graph statistic, not a proof of a linear-spill lower bound.
- Initial ready work: 1,057 nodes / 67,048 records.
- Frozen extra cap: P*ceil(sqrt(n))=25,304 records, hence 395 B-capacity slots / 25,280 allocated records.
- The policy reaches 395 simultaneously live spill slots and cannot proceed within the cap.
- At rejection, only 1,848 nodes / 99,923 records have received their final copy. 155,405 nodes remain.
- 715 ready frontiers have been drained. Total evacuation copy requests are 25,344 records; a released/reused slot explains why total evacuation work can exceed live capacity.
- Partial copy work is 125,267 records (final writes plus evacuations). This is an incomplete schedule, **not** a completed 0.0125n materialization result.
- No semantic bucket has a complete readiness quantile. No end-to-end time or hardware-traffic inference is made.

## Independent space rejection

The frozen total auxiliary cap is 32*P*ceil(sqrt(n))*sizeof(Row16)=12,955,648 bytes. The predeclared 64-bit structure-of-arrays plan uses 11,548,146 bytes of node/edge/queue/state/construction metadata for this graph. Its formula is in README.md and the probe; diagnostic Python objects and exported events are not the proposed Rust representation.

Even a deliberately incomplete lower bound already fails:

- Existing native scratch capacity: 4,194,304 bytes.
- Proposed metadata: 11,548,146 bytes.
- Frozen spill capacity: 404,480 bytes.
- Sum: 16,146,930 bytes, exceeding the total cap before counting native directories and other retained state.

The later unmodified capacity ledger `../tessera-generic-current-profile/evidence/adopted-v2-row16-n10m-w8-ledger.tsv` accounts for 5,832,772 bytes live at export, after local tasks/directories are dropped. Adding proposed metadata and spill gives at least 17,785,398 bytes. Its accounting still excludes channel/Arc allocations, Rayon runtime and diagnostic storage and is not an allocator peak. These omissions cannot rescue an already-exceeded cap. Before-transfer accounting is 7,521,672 bytes and is a different lifetime, not an additional simultaneous charge at export.

There is no total-memory pass and no comparison against same-core measured strong-serial heap. Those acceptance stages were unnecessary once these lower bounds failed.

## Validation and exact evidence

The metadata parser checks the real scratch-segment mapping, original B-aligned input mapping, full physical-directory permutation, live-prefix lengths, final contiguous rank coverage, and semantic bucket intervals. All external edges preserve exact source-record intersections, excluding padding. Scratch allocations are never treated as one flat array. The original short tail is not an extra authoritative input block.

Ten small metadata tests passed twice: identity; one-spill cycle; zero-cap rejection; overlapping self-move; padding exclusion; two exact dependencies; separate scratch segments; duplicate source rejection; unaligned input-source rejection; wrong hook rejection. This is not exhaustive schedule testing, a record mover, a Rust provenance proof, owned-panic recovery, or actual parallel moving-kernel testing.

- First test window: shared FD9 acquired/released 2026-11-07 17:38:07 UTC. The auxiliary `/proc/$$/fd/9` stat failed because the shell PID path was unavailable; its complete log is retained. The flock and tests returned success.
- Verified full-graph window: shared FD9 acquired 17:39:30 UTC, `os.fstat(9)` confirmed device 27 / inode 1578019, released 17:39:34 UTC. Timeout limit was 60 seconds. No sort benchmark ran.
- Test/full-graph log: `evidence/native-row16-n10m-w8-run.log`.
- Complete graph statistics, exact edges and partial execution events: `evidence/native-row16-n10m-w8-result.json`.
- Native v1 capture: `../tessera-generic-current-profile/evidence/adopted-row16-n10m-w8.tsv`.
- Capture SHA256: `13766d5c1e52266dea18a93624eae49d1a6d40cb48b62b9499c111b3345b3dfe`.
- Native capture source ID: `4f2f2a14cbc4a1c6c449d856f87409dda43050903e8bb7568ad2e671f365acd7`.
- Executed probe SHA256: `bf8a2f81e294b77dd731bf268302ad4195e4af308691879b051d32ad41b5aeeb`.
- Executed test source SHA256: `16a81fbeed6d26cc6b2a3e0c8771024c8df76d254ee2454370a79cf258995d80`.
- Executed TSV adapter SHA256: `2a49470c4209de7eaae4981fae51fa73fa1f2211b95eee5bf0d17781b9249b1e`.

## Small root-radix duplication audit

No matched full-random, same-core, same-P root-R16 versus root-R256 end-to-end control was found in the reviewed pipeline/scatter/compact/head-to-head/head-matrix/root-budget/combined reports, harnesses and run scripts. This is a bounded evidence search, not a claim about every historical invocation.

Three superficially similar experiments do not answer it:

- `../tessera-scatter-evidence/REPORT.md`, lines 136 onward: root2/root4 means root **shards**, retaining eight-way children. `../tessera-root-budget-evidence/README.md` explicitly leaves the root digit width unchanged.
- `../tessera-best-combined-profile/report.md`: random uses width eight; the width-four d16 path is a single-digit case with no child sorting.
- `../tessera-virtual-adaptive-child/CONTROL.md` and associated adaptive notes change virtual child fan-out, not a full-random native root.

The historical combined and current metadata source do contain a `TESSERA_PIPELINE_ROOT_BITS` cap. An existing knob is not evidence of a measured matched R16 control. The head-matrix replays explicitly clear that override. No new radix experiment was started here.

One useful correction to the hypothesis: root-R16 does not necessarily add a physical partition level at 10M. Current `engine.rs::choose_split_digit` chooses ceil(log2(child_records/(tile_records/2))) clamped to 3..8 bits before its concentration check. With the 64KiB tile and approximately uniform occupancy:

- u64: root8 leaves ~39k-record children selecting ~4 bits; root4 leaves ~625k children selecting ~8 bits. Both can reach tiles after two block-partition levels and resolve ~12 high bits.
- Row16: root8 children select ~5 bits; root4 children select ~8 bits. Both can reach tiles after two levels, resolving ~13 or ~12 high bits respectively.

This arithmetic is an expectation from the source, not observed route counters. Real concentration checks, skew, child sizes, staging floors, remaining tile digits and materialization overlap must be counted. Fewer root prefixes can lower root metadata and holes but delay first child publication and alter child work. Therefore R16 remains an unmeasured radix/decomposition control, not a demonstrated new algorithm or automatic permission for a parameter sweep.
