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

- [Parallel Direct count-window](patches/parallel-count-window/README.md): frozen e8 source and initial four-case screen. Three narrow research signals, one inconclusive case; no default adoption or full-matrix acceptance.

- [e1 count-window follow-on](patches/parallel-count-window-e1/README.md): shared runtime bridge, wide-tile tests and feature-OFF Tier 1 evidence with retained inconclusive cells; no runtime adoption.

- [e1 count-window Tier 2 bounded screen](patches/parallel-count-window-e1/tier2-results/REPORT.md):
  504 samples and 20,385 sorts at W8/n1M. Three of four narrow-window cells
  clear controls; u32/d16 and all four wide fallback cells remain inconclusive.
  No default adoption, runtime changes, or later shifted/ordered results.

- [Frozen e1 shifted and early-exit boundaries](patches/parallel-count-window-e1/tier2-boundary-results/REPORT.md):
  630 samples and 139,113 sorts across ten cells, all inconclusive under full
  controls. The u32/equal37 slower primary tendency remains unresolved; this
  does not establish regression-free behavior. Historical e1 source, not the
  subsequently merged main refactor; no count-window adoption.

- [Public-entry count-window source checkpoint](patches/public-count-window-source/README.md):
  Rust-only patch against public `987fbdfe`, with complete source hashes,
  safety scope and replay proof. Sealed Direct u32/u64 only; no performance
  result in that historical packet. The experimental runtime now includes this
  candidate with public memory documentation clarified; the archived bytes
  remain unchanged. This does not change the default sequential API.

- [Public count-window 17-cell result](patches/public-count-window-results/REPORT.md):
  1,377 samples and 56,715 reference-checked sorts for the
  [archived source candidate](patches/public-count-window-source/README.md).
  W8 clears controls in three of four narrow inputs against both the public
  baseline and original serial API. u32/d256 and equal37 remain inconclusive;
  no general regression-free claim or runtime adoption.

- [Public count-window fallback gate](patches/public-count-window-fallback-results/REPORT.md):
  810 samples and 12,580 reference-checked sorts across ten declined-route
  cases. All C/A cells remain inconclusive; u128/d256 is slower than original
  serial, not established as count-hook regression. Zero retained heap in all
  30 observations. The frozen report precedes the separate runtime integration.

- [Rejected direct-interval metadata probe](direct-interval-negative/README.md):
  the frozen greedy policy exhausted 395 spill slots after 99,923/10M final
  writes. Its complete auxiliary lower bound exceeds the frozen byte cap.
  This rejects the tested policy/representation, not all low-space interval
  algorithms; no record mover, sorting benchmark or runtime change.

- [Current generic-root diagnostic profile](generic-current-profile/README.md):
  640 reference-checked sorts; current public and native combined controls,
  with broad tails and tracer perturbation. Root end fractions are not serial
  fractions. Focused library/Miri scope only; no new optimization accepted.
