<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Ordered-map validation scope

All final checks below ran against the frozen private source whose21 runtime
and test files are byte-identical to the independently applied historical target.
The standard-name staging archive was not rebuilt. Source equality transfers
this evidence; it does not replace a future integration or promotion decision.

- Current stable release workspace/all-targets/all-features:56 unit plus22
  integration tests passed. Existing native ownership, inconsistent extraction,
  producer/consumer panic and bounded nested Rayon P2/P3/P4 tests are retained.
- Exact generic/ordered comparison passed12,600 paired native geometry cases,
  comparing every evacuation/squeeze copy, visit position, callback contents and
  order, plus retained directory capacity/address. It covers zero/self copies,
  partial lengths, empty buckets, tails and segmented scratch. Maximum-radix
  tiny-input cases also passed.
- Eight root tests passed with `TESSERA_COMPACT_SERIAL=1` on the final binary.
- Six all-feature doctests, Clippy and rustdoc with warnings denied, and explicit
  three-file formatting checks passed. No repository-wide format or path
  dependency rewrite was used.
- Stable no_std check and Rust1.85 default/Rayon tests typechecks plus no_std
  check passed on final source. Rust1.85 default full tests passed before the
  final unsafe-contract/test-trace-only revision, which was then typechecked.
  portable-simd is not part of the Rust1.85 claim; it requires Rust1.89 or newer.
- Twelve focused Miri tests passed after the final revision with only
  `-Zmiri-strict-provenance`, using default Stacked Borrows. They exercise the
  same ordered kernel with serial root scheduling, owned String+Cell records,
  exact copy traces, deferred exclusive records, inconsistent keys, permutation
  faults, and a standard scoped-thread fixture keeping a borrowed child live
  while the producer progresses. Producer, child and simultaneous panics pass.
  Actual Rayon/Crossbeam scheduling is not claimed as Miri coverage.
- Independent read-only source review approved the final three changed-file
  hashes. This is separate from runtime test execution and performance evidence.

The initial Clippy pass found two missing rustdoc backticks; both were fixed
before source freeze and the original failure log is retained externally.
Rust1.85 emits the unchanged `key.rs` visibility warning; Miri emits existing
native-test-witness dead-code warnings. No warning suppression was introduced.
No native correctness or Miri failure occurred in this implementation stage.

The private fixed-source manifest and gate summary distinguish source freeze
from later successful dynamic gates. External evidence hashes are listed in
`ordered-map.external-evidence.json`; raw logs, binaries and harnesses are not
included. No new allocation-byte measurement, timing result or performance
acceptance conclusion is asserted here.
