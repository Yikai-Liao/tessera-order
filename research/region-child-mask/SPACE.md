<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Space and semantic limits

The child-mask change adds a scalar mask and reuses existing public preparation/workspace storage. It introduces no record buffer, root mover, early overlap scheduler or new unsafe operation. Source equivalence, rather than a new allocation measurement, establishes that its root geometry and movement code are unchanged.

For the existing root, let P be source shards, R root buckets, B records per block and Q<=PR shared boundary/tail blocks. Scatter reserves `(PR+Q)B<=2PRB` record slots. Direct path restoration uses at mostPB temporary records; the pre-existing split-cycle fallback retains its conservative9PB bound. Root metadata is `O(n/B+PR)`. With fixed radix and the existing balancing rule/floors for B, the inherited asymptotic bound is sublinear in n for fixed P. This is not a newly measured peak or a universal small-constant promise.

Root restoration joins before fixed child groups begin. Each executing group reuses one current serial Workspace. Its retained capacity and downstream tile/block behavior are unchanged by the mask. Do not add independent stage peaks without respecting lifetimes, and do not infer strict per-physical-thread or reentrant bounds from a group-count argument.

Diagnostic report vectors, input clones, full stable-reference arrays and full-key mask-validation traversals are external correctness/count overhead. They are absent from non-diagnostic timing configurations and must not be included as production auxiliary space or used as timing evidence.

Only sealed intrinsic-key Copy u64, Row16 and Row128 are supported. Stable IDs are oracle payloads, not a new per-record ordinal scratch array. Owned and Send-but-not-Sync records, arbitrary/inconsistent/panicking extractors and permutation recovery after allocation/scheduler panic are not established by this transport. Existing Copy values remain initialized under the original root's limitations.

The archive itself contains only small patches/tests/support and uses in-memory source verification by default. No vendor tree, raw sample, binary or external benchmark harness is included. Optional source export is explicit, targets a new directory and is not performed by the archive verification recorded here. No build, correctness sort, diagnostic count, timing or heap experiment was repeated while assembling this packet.
