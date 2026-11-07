# Ordered-input controls: non-default research patches

Apply `grouped-order-confirmation.patch` after `combined-rayon.patch`.
The separate `singleton-reversal-control.patch` applies after the grouped patch.
Neither is enabled in the production crate or selected as a universal winner.
No raw timings, assembly diagnostics, harnesses, or binaries are included.

## Changes

Grouped confirmation retains the existing strided order gate, then compares adjacent
keys in groups of 16 before branching on an inversion. Keys are still extracted in
input order, but an inversion can be followed by the rest of its bounded group.
No additional full scan or allocation is introduced for random input. The measured
u64 specialization is scalar unrolled code, not SIMD. Three targeted tests are included.

The reversal control skips reversing a run of length one. It preserves extractor
call order and adds three tests for stable duplicate runs, owned values with extraction
panics, and exclusive mutable borrows. It is not recommended as a proven repair.
Neither patch changes unsafe code, feature defaults, allocation policy, or API bounds.

## Measurements and adverse cases

The grouped full-sort experiment covers 15 cases: u64/u32/row16 at 1,048,576 records,
with sorted/equal/reverse/random/d16, 15 variants, nine randomized blocks, 2,025 samples.
Every sample accumulates at least 25 ms of sorting; clones and stable-reference checks
are outside timing. No outliers were removed.

Grouped sorted parallel8 paired ratios versus unchanged consumer-wait are 0.680 u64,
0.467 u32, and 0.736 row16. For u64, medians are 0.228 versus 0.326 ms. Serial u64
is 0.222 versus 0.334 ms, paired ratio 0.659. Old serial controls are [0.848,1.180],
grouped serial [0.800,1.250], and grouped parallel/placement [0.801,1.248]. Including
all original baseline placements expands the envelope to [0.425,2.354]: identical
original kernels have a separately reproduced code-address sensitivity on this host.
Do not call these measurements a portable performance guarantee.

Random parallel paired ratios 0.983/0.934/1.002 are within broad controls. The adverse
u64 reverse case is 1.569 versus 1.395 ms, ratio 1.140, slower in all nine blocks.
Thus the grouped candidate is not approved for default promotion.

A separate 21-case reversal-control experiment contains 2,835 verified samples, adding
reverse pairs and descending 16-key runs. Relinking alone changes grouped-only reverse
time to about 1.31 ms; the guarded version is about 1.29 ms (ratio 0.99). This is not
credited to the guard. Guarded u64 duplicate-pair parallel sorting is worse, paired
ratio 1.144. The guard remains a negative/control candidate.

The forced-alignment reversal experiment completed only unique keys. Its duplicate-pair
case is partial, and remaining cases were not run. No more attribution experiments are
scheduled for this checkpoint; ongoing work prioritizes structural parallel acceleration.

## Verification and limitations

Original grouped source passes 65 native all-target Rayon tests, the three new tests
under Miri, default-feature warnings-denied Clippy, and no-default-feature check.
Grouped plus guard passes the existing suite and three final reversal tests (68 total),
six targeted Miri tests, and stable/Rust 1.85 no-default-feature checks. Strict Rayon
Clippy still has the inherited compact.rs `for_each(&run)` warning. Rust 1.85 reports an
inherited unreachable-public-item warning. Doctests were not added for these private
changes. These are source-origin results; packaging itself only validates application.

The two patches independently apply in sequence and reconstruct the exact tested engine
and added test files. Source/hash manifests for the two measured full-sort freezes were
reverified without mismatches. An earlier unmeasured driver source was later expanded;
that mismatch does not affect frozen alignment or prototype sources, but is not hidden.
Historical affinity/frequency/environment traces are incomplete. No unspecified host
contention is claimed as the cause of unexplained cross-run variation.

Patch SHA-256 values:

- grouped: `715259764797a017a762bf5f2c9f17b2c10eaf30ecc1da7c4328bc4f5aa80cc8`
- reversal control: `a2dd37133cb0a14baec9f0f196cde31db9b56e1156fc2240951fa53827c6b4bb`

The full external research report is `tessera-fastpath-evidence/stage-package/README.md`.
