<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Parallel sorting research checkpoints

These standalone crates and patches preserve working experimental stages. They
are not workspace members, are not published, and do not change Tessera's default
engine or optional public Rayon implementation. Read each checkpoint's scope,
ownership proof, validation record, and limitations before using it.

- `patches/combined-rayon.patch`: native root scatter, parallel block movement,
  pipelined child sorting and the consumer-wait fix, against the documented v3
  base. It is archived for review, not applied to the default crate.
- `owned-rank/`: stable block-recycling merge with owned `T: Send` records and
  u64 keys. It preserves records after normal key panics but is a narrower
  research API, not the full public RadixKey contract.
- `virtual-bucket-core/`: serial Copy foundation for consuming bucket directories
  without intermediate materialization. Parallel scheduling is a later stage.

The [experiment report](../docs/development/parallel-experiments.md) includes
negative results, control noise, and the practical space costs. Raw benchmarks,
profiles, and harnesses remain outside this repository. Each crate has a separate
`[workspace]` and must be tested explicitly with its own manifest path.
