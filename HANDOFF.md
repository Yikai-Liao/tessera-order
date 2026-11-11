<!-- SPDX-License-Identifier: BSD-2-Clause -->

# Engine planning convergence: work-in-progress handoff

Temporary operator-requested handoff. Remove this file before opening the PR. The LFS evidence archive and its path-specific attribute are temporary transfer material too; preserve an external copy before removing them. Deleting them in a later commit does not remove LFS objects from branch history.

## Status and objective

**The overall simplification goal is not complete or performance-accepted.** Reduce engine concepts, coordination and production code through general mechanisms, while preserving useful fast paths, stable ordering, bounded scratch, panic recovery and inconsistent-key record ownership. Do not replace the remaining regressions with distribution-specific exceptions.

- Remote branch: `refactor/engine-planning-convergence` on `origin` (GitHub repository `Yikai-Liao/tessera-sort`).
- Original worktree: `/home/lyk/.paseo/worktrees/130vdace/flawless-catfish`.
- Pre-consolidation baseline: `b8968b694bc1c7f7172d3e13f1d07f90f8d5474b`.
- Latest engine checkpoint: `eed2f22a490a4c0449753879e3694ad5dd1dd39f`, `refactor(engine): bind classification policy to extractors`.
- The engine checkpoint was already committed and the worktree clean when this handoff session began. The earlier conversational statement that it was uncommitted is obsolete.
- This handoff session inspected code and existing evidence only; it made no additional engine changes or measurements. Archive validation and report regeneration are transfer checks, not performance acceptance.

Read [AGENTS.md](AGENTS.md) first. Its task index selects the applicable contracts and procedures. Current decisions belong in [the engine reference](docs/design/engine.md), measured evidence in [the port record](docs/design/port.md), and unfinished acceptance in [ROADMAP.md](ROADMAP.md#measured-engine-refactors), especially E2–E7. This temporary file does not replace those authorities.

## What has converged

The branch includes more than the final cache-policy change. Important checkpoints, in development order:

| Checkpoint | Change |
|---|---|
| `b8a33b5` | Reuse clustered key samples across planning decisions |
| `d2d5741` | Plan dense, shifted and sparse key windows with one objective: minimum passes, then minimum table slots |
| `00559dd` | Separate planning decisions from tile ownership and restoration |
| `0c0f71d` | Require one SIMD backend instead of maintaining the previous optional backend split |
| `cb6be7f` | Share ordered finishing across child jobs |
| `b6d50a6`, `e4fea65`, `6fac229` | Replace the low-fanout tile override with conditional sample-weighted work, retain recursive partition opportunities, and reduce group-scan work |
| `a208d15`, `a239c33` | Give block storage active-range and shard-adoption authority |
| `2a53f30` | Remove the obsolete float scan policy |
| `7ebb249`, `bad108a` | Share exact order confirmation and reduce its overhead |
| `eed2f22` | Make the extractor own tile classification caching |

Benchmark tooling also gained resolved historical-feature provenance, parallel revision screens, linked revisions with independent copies, and three-block local screens. These are maintained tools, not experiment-only harnesses to duplicate.

### Latest checkpoint: extractor-owned caching

The independent `CACHED` parameter no longer crosses the public entry, serial engine, Rayon scheduler and tile operations. `Extract::CACHE_BUCKETS` owns the rule:

- User callbacks retain cached classifications, so panics or changing keys cannot invalidate scatter ownership.
- Sealed, deterministic, panic-free direct encoders retain the uncached fast path.
- Half-width adapters inherit the extractor policy rather than accepting another caller-selected mode.

The preceding session's independent `yume/gpt-6.1-sol` review accepted this structural change as a positive but limited improvement. This is not whole-branch performance acceptance or a new review of the eventual PR. Before merge, follow the independent Agentic Review requirements in `AGENTS.md` and re-review material fixes.

## Open regressions and evidence limits

The completed 280-case linked comparison uses **`b8968b6` versus clean `bad108a`**, not `eed2f22`. The cache-policy checkpoint has separate bounded evidence. Do not relabel the broad comparison as a direct measurement of the latest checkpoint.

Broad-run conditions: Intel Core i7-10875H, Linux x86-64, CPU 0 affinity, Rust 1.99.1 / LLVM 23.1.3, three shuffled blocks, 5 ms sample target, 256 MiB prepared-buffer cap, shared 64-byte loop/non-fallthrough alignment. Recorded governor is powersave, turbo on and SMT enabled; these were not changed. Historical baseline builds explicitly enable Portable SIMD. See the retained host, session and manifest files for exact provenance.

Linked report ratios are **placement time / paired current time**. A baseline ratio below one means the current revision is slower. The table shows both baseline-copy median ratios, and converts them to current-time increases with `1 / ratio - 1`; it is not an aggregate verdict.

| Case | Baseline A / current | Baseline B / current | Current-time increase | Current A/A range |
|---|---:|---:|---:|---|
| `record128/runs/65536/s1` | 0.47312 | 0.47130 | +111.4% / +112.2% | 0.98982–1.01018 |
| `u64/bimodal/1000/s1` | 0.78773 | 0.78764 | +26.95% / +26.96% | 0.99870–1.00130 |
| `u32/random/1000/s1` | 0.86069 | 0.86288 | +16.19% / +15.89% | 0.99580–1.00420 |
| `u64/random/1000/s1` | 0.88457 | 0.88633 | +13.05% / +12.82% | 0.99122–1.00878 |
| `record128/random/1000/s1` | 0.84772 | 0.84702 | +17.96% / +18.06% | 0.99605–1.00395 |

Both baseline placements expose these losses. Current-copy medians for the listed cases are approximately 0.99812, 1.00009, 0.99589, 0.99320 and 0.99510. Inspect the full per-block ranges and retained executable before making source-attribution claims; distinct adapter entries alone do not prove distinct internal bodies.

Fixed-work evidence against `b8968b6` confirms additional small-input work, including sampling, sample sorting and estimation. For example, `u64/bimodal/1000` adds 29.33% instructions and `rec128_u64/random/1000` adds 21.33%. Large-record losses also involve insertion finishing and memory access: `rec128_u64/runs/65536` has +4.22% instructions but fewer data references and sharply different simulated cache misses. Instruction totals alone do not explain its +112% latency loss. Counter IDs use `rec128_u64` where linked reports use `record128`.

The last reported production-code count before `eed2f22` was **3,349 → 3,391 lines (+42; +1.25%)**, excluding tests, comments and blank lines. This handoff has not recomputed that metric at `eed2f22`. Neither whole-diff line counts nor file splitting demonstrate the intended reduction.

## Validation already recorded

For the cache-policy checkpoint, the preceding session recorded successful workspace tests and doctests, Rayon tests, both Clippy configurations, site checks, and seven focused Miri tests. See `cache-owner-workspace.log`, `cache-owner-rayon.log`, `cache-owner-clippy.log`, `cache-owner-site.log` and `cache-owner-focused-miri.log` in the evidence snapshot.

The focused Miri tests cover the narrowed callback tile path (one), half-key encoding/window behavior (two), and parallel bucket-borrow ownership/stability (four). **The broader tile Miri run timed out and is not a pass.** Earlier experiments also include interrupted runs; a directory's presence in the archive is not a completion or acceptance verdict.

The latest cache-policy change has an 18-case, three-block linked comparison against `bad108a` and six fixed-work counter cases. Instructions and data-read/write counts match exactly in those six cases; simulated cache events and linked timings can differ. Placement differences remain, so these checks do not establish universal performance equivalence.

No engine tests or new timing experiments were rerun merely to package this handoff. Transfer checks passed: XZ integrity, archive comparison with the original files, SHA-256 verification, extraction to a disk-backed external directory, and regeneration of all four reports named below. `git diff --check` and `pnpm -s lint` passed; lint emitted five existing informational suggestions in benchmark tooling, with no fixes applied.

## Evidence transfer

The operator explicitly authorized this temporary Git LFS exception to the usual rule that raw measurements stay outside Git.

- Archive: `artifacts/handoff/engine-planning-evidence.tar.xz` (about 40 MiB compressed; 344 MiB original directory).
- Checksum: `artifacts/handoff/engine-planning-evidence.tar.xz.sha256`.
- Contents: the complete existing `/tmp/tessera-goal-evidence/` snapshot, including logs, plans, raw samples, reports, provenance, Callgrind outputs and retained executables where present. Original local paths in manifests are intentionally preserved.
- Other local review worktrees and toolchain installations are not included. The archive is evidence, not a portable build environment; use the repository history, locked dependencies and documented toolchain setup to rebuild.

From the repository root, fetch and unpack **outside the checkout**; choose a disk-backed location with enough free space. The original machine's `/tmp` was nearly full during packaging.

```sh
git lfs pull --include="artifacts/handoff/engine-planning-evidence.tar.xz"
sha256sum -c artifacts/handoff/engine-planning-evidence.tar.xz.sha256
export EVIDENCE_ROOT="$HOME/tessera-engine-handoff"
mkdir -p "$EVIDENCE_ROOT"
tar -xJf artifacts/handoff/engine-planning-evidence.tar.xz -C "$EVIDENCE_ROOT"

# Regenerate retained reports without taking new measurements.
pnpm tools report "$EVIDENCE_ROOT/tessera-goal-evidence/core-convergence-linked"
pnpm tools report "$EVIDENCE_ROOT/tessera-goal-evidence/cache-owner-linked"
pnpm tools report "$EVIDENCE_ROOT/tessera-goal-evidence/core-convergence-counters-v1"
pnpm tools report "$EVIDENCE_ROOT/tessera-goal-evidence/cache-owner-counters"
```

Start with these directories under `tessera-goal-evidence/`:

| Directory | Purpose |
|---|---|
| `core-convergence-linked` | Completed 280-case broad baseline comparison; `report/index.md` and `report/summaries.json` are the entry points |
| `core-convergence-counters-v1` | Fixed-work baseline comparison and raw Callgrind attribution |
| `cache-owner-linked` | Latest 18-case linked screen; also contains retained symbol/disassembly inspection |
| `cache-owner-counters` | Latest six-case fixed-work comparison |
| `conditional-work-*`, `conditional-recursive-counters-v3`, `conditional-fixed-group-*` | Planner candidate history, including rejected or performance-unaccepted variants |
| `shared-order-*` | Confirmation candidates and their code-generation/placement limitations |

## Suggested continuation

1. Read the planner and tile routes together: `rust/tessera-sort/src/engine/plan.rs` owns sampling, exact digit planning and advisory recursive work; `engine/tile.rs` owns counting passes, insertion completion, scratch and restoration. Trace serial and parallel consumers before changing a shared rule.
2. Use the existing Callgrind attribution and retained linked results to distinguish fixed planning work from finishing/record-movement costs. The large-record regression needs its own causal explanation; do not assume cheaper planning resolves it.
3. Seek a cheaper, simpler **general planning mechanism**, preserving constant-child finishing, recursive MSD opportunities, sparse/shifted key handling, effective bucket limits and known candidate variation. No new implementation candidate was selected in this handoff session.
4. Compare representative maintenance tasks—changing sampling, tile capacity, digit selection, block layout and task size—to judge net understanding and coordinated-change cost. Do not optimize line count by discarding useful information or old fast paths.
5. Keep structural changes and algorithm/performance changes separately identifiable, with local rollback commits. Use the maintained `pnpm tools` commands, preview the intended coverage, and follow [the measurement protocol](docs/development/benchmarking.md#run-an-experiment), including linked copies, A/A ranges and placement investigation. Reuse the retained plans where helpful; do not silently change benchmark inputs or sampling.
6. Update engine explanations and simulation models for algorithm changes; retain contract, ownership, panic and inconsistent-key tests. Complete relevant serial/Rayon, Clippy, doctest, site and Miri checks, recompute production-code size under a stated scope, and obtain independent `yume/gpt-6.1-sol` review before declaring success.

Before opening the PR, copy evidence to durable external storage and strip the temporary handoff/data commits if the PR must not carry LFS history. The branch is a resumable checkpoint, not a ready-to-merge simplification result.
