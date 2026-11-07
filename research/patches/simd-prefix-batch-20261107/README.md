# Four-group SIMD prefix batch: archived experiment

**Decision: no demonstrated gain; no adoption.** This is a source-only archive
of a completed, bounded experiment, not a production change or recommendation
to enable the candidate. Every candidate median remained within the declared
A/A and independent-placement controls. No end-to-end sort timing followed.

## Candidate and baseline

The patch applies to main commit `0bc80b49d5efd5bd8b62165b170015e5803226b7`.
It changes only `prefix_sum_with` in `portable_simd.rs` and adds internal tests.
The exact original stays the control: it was not rewritten to resemble the
candidate. Four independent **u16x8** local prefix scans replace one per loop
iteration. The scalar carry chain, original eight-lane remainder and scalar
tail remain. Runtime dispatch, routing, thresholds, keys, histogram/scatter,
allocation and public API stay unchanged. No new unsafe code is introduced.

Exact-main assembly was inspected first and did not already batch four groups.
Candidate AVX2/AVX-512 register use grows from three to five XMM registers;
SSE4.2 grows from two to seven and inline SSE2 from two to nine. No new x86-64
vector spills were observed. Dispatch stack setup is unchanged. This is not
a spill guarantee for other targets or compiler contexts.

## Maturity and validation scope

Release all-feature conformance/recovery/smoke tests, optimized Clippy, default
and no_std tests, MSRV 1.85 default/no_std checks, and rustdoc passed. New native
prefix tests cover every length 0–16384, u16 total exactly 65535, zeros,
one-hot, alternating/dense inputs, guarded offsets, lane/batch boundaries and
remainders on safely available SSE2, SSE4.2, AVX2 and AVX-512 backends.
Focused strict-provenance Miri passed all three scalar-fallback prefix tests.

**Optional broad Miri was interrupted and is incomplete.** Ten unchanged
library tests and the float conformance test passed before interruption during
integer conformance; exit status was 130. No full Miri pass is claimed.
Portable SIMD was tested with Rust 1.99.1, not separately with 1.89. Non-x86
backends were unavailable. See `validation-scope.json` for precise scope.

## Bounded result

The same four real-count corpora come from the existing n=1,048,576 seed-1
sort inputs: u32/u64 random and sorted99. All 896 actual tables are retained
in original call order, with lengths 4096/8192/16384 and totals <=65535.
Each block contains duplicate original, duplicate candidate, and independently
compiled original-placement labels. One warmup plus nine randomized blocks
per corpus, at least 25 ms accumulated per sample. Resets and full scalar
reference equality checks are outside the timer. Runtime dispatch is included.

| Corpus | Candidate/original median | Blocks below 1 | Control envelope |
|---|---:|---:|---|
| u32-random | 0.971755 | 6/9 | [0.873669, 1.144598] |
| u32-sorted99 | 0.944019 | 8/9 | [0.874744, 1.143192] |
| u64-random | 0.955539 | 9/9 | [0.887235, 1.127097] |
| u64-sorted99 | 0.946371 | 8/9 | [0.531041, 1.883093] |

All four results are **within controls**. Directional medians are observations,
not accepted speedups. This candidate was stopped without retuning, further
variants, or whole-sort experiments. Kernel timing does not establish the
prefix share of sort CPU time, and whole-corpus cache residency differs from
a freshly constructed histogram. No speculative hardware cause is assigned.

All 180 measured and 20 warmup samples verified. **Every raw batch duration,
including long tails, is preserved outside this packet.** No filtering or
censoring was used. `results-summary.json` includes control ranges, minimum
and maximum batch tails, and raw-file hashes. `artifact-index.json` locates
and hashes the preserved complete evidence, including assembly, corpus,
source freeze, driver, binary and raw samples. The metadata-only failed first
launch is also retained; it produced no timing sample. The successful run
used the unchanged sources/binary and released its exclusive CPU lock.

## Packet contents and use

- `candidate.patch`: exact-main source patch including its test module.
- `validation-scope.json`: passed gates and explicitly incomplete broad Miri.
- `results-summary.json`: result/control/corpus/tail summary, not raw samples.
- `artifact-index.json`: preserved external evidence identities and sizes.
- `apply-check.json`: independent apply/hash/reverse verification.
- `manifest.json` and `SHA256SUMS`: packet identities.

For review only, from a clean export of the exact baseline:

```sh
git apply --check /path/to/candidate.patch
git apply /path/to/candidate.patch
git apply --reverse --check /path/to/candidate.patch
git apply --reverse /path/to/candidate.patch
```

The archive stage ran no compilation, test suite, benchmark or new candidate.
An independent temporary tree verified exact applied source hashes, unchanged
unrelated files and byte-for-byte round-trip restoration. No shared repository
was modified. Raw samples, corpora, target directories, binaries and vendor
copies are deliberately excluded from this packet, consistent with the
repository's benchmark-evidence policy.
