<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Parallel sorting research checkpoints

These standalone crates and patches preserve working experimental stages. They
are not workspace members, are not published, and do not change Tessera's default
engine or optional public Rayon implementation. Read each checkpoint's scope,
ownership proof, validation record, and limitations before using it.

- `patches/combined-rayon.patch`: native root scatter, parallel block movement,
  pipelined child sorting and the consumer-wait fix, against the documented v3
  base. It is archived for review, not applied to the default crate.
- [Current-main combined port](patches/combined-main-5b13.README.md): the
  stronger native-root pipeline rebased onto upstream `5b13ac1`, with explicit
  SIMD-off/on controls and an inconclusive four-case pilot. This patch targets
  the documented experimental commit and is not applied to production.
- [Ordered post-permutation map control](patches/ordered-map.README.md):
  a validated single-variable research patch removing redundant directory work
  while preserving every copy, publication and allocation. It remains disabled;
  [screening paused after a non-target serial-path regression](patches/ordered-map.SCREEN.md),
  with all completed samples retained and the exact cause still unproved.
- [Causal root and scheduling controls](patches/causal-controls.README.md):
  independent fixed-root, sleep50 and deferred-consumer experiments. Their
  source/guard patches and precise negative results distinguish waiting CPU,
  hidden child work and unverified hardware causes. None is adopted.
- [Bounded indirect LSD leaves](patches/bounded-indirect-lsd.README.md): a
  validated tile-only key/index snapshot and record-cycle experiment. It halved
  measured logical leaf copies but regressed serial throughput and explicit
  32-byte owned parallel sorting. Not adopted; follow-on work is paused pending
  bottleneck diagnosis. Its exact historical apply chain is documented.
- [Bounded halo squeeze](patches/halo-squeeze.README.md): generic parallel
  materialization with bounded threatened-source snapshots. Neither screen
  establishes a win; the corrected four-worker case regresses and unexplained
  candidate tails remain. The source is archived without adoption.
- `aligned-root/`: sealed Copy histogram-aligned root scatter with packed block
  restoration and real contiguous child sorts. The prototype removes squeeze,
  but its verified screens do not establish an end-to-end win.
- `owned-rank/`: stable block-recycling merge with owned `T: Send` records and
  u64 keys. It preserves records after normal key panics but is a narrower
  research API, not the full public RadixKey contract.
- `virtual-bucket-core/`: serial Copy foundation for consuming bucket directories
  without intermediate materialization. This research line is stopped; the
  completed negative stage-control reports are retained.

The [experiment report](../docs/development/parallel-experiments.md) includes
negative results, control noise, and the practical space costs. Raw benchmarks,
profiles, and harnesses remain outside this repository. Each crate has a separate
`[workspace]` and must be tested explicitly with its own manifest path.
