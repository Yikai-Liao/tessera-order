<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Ordered-map screening: paused after a non-target regression

The validated source checkpoint is not adopted. Performance testing stopped
with **139 of 264 planned full-matrix cells complete**, plus two primary cases.
The remaining 125 cells were not run. Every started case finished; no measured
sample was trimmed, interrupted, discarded, or rerun to obtain a win. All fresh
outputs passed complete stable reference and payload checks. No completed
sample was censored for insufficient duration. Heap accounting is not complete.

## Controls and conditions

O is the frozen strongest combined implementation, N the map-elision candidate,
and R the same candidate with its ordered wrapper instantiated as false, retaining
map work. R controls for extracting the common body behind new wrappers.
An independently packaged original provides placement control. Strong serial A
and corrected public entry P use the same enabled Rayon/portable-SIMD features.
Duplicate labels share a match arm and closure. Each case has nine randomized
blocks after verified warmup; comparisons are within the same executable.

Inputs use the preserved SplitMix64 seed-1 generators. The process and its eight
workers inherit affinity mask 0–7; this is not individual worker pinning. Cloning
and complete output verification are outside each API timer, on the actual
worker; sorting scratch allocation is timed. Matrix samples accumulate fresh
single-sort calls to at least 25 ms. The two primary 10M cases use one API call
per sample. Per-type executables are not compared through absolute times.

## Primary random cases

For u64 at 10,000,000 records, independent label medians are N 23.509/23.934 ms,
O 25.190/25.269 ms and R 25.215/24.864 ms. Paired N/R has median 0.8824 with
9/9 blocks lower, and N/O 0.9477 with 8/9 lower. This does not pass the complete
control rule: retained R1 block 4 takes 80.282 ms versus R2 24.266 ms, producing
a reciprocal control envelope of [0.3023, 3.3084]. No cause is assigned to that
tail. Strong serial labels are 91.343/91.726 ms; public labels 45.647/44.190 ms.

For Row16 at 10,000,000 records, N labels are 42.987/43.488 ms, O 47.700/45.015 ms
and R 45.467/46.838 ms. Paired N/R is 0.9370 with only 7/9 lower; N/O is 0.9430
with 8/9 lower. The complete control envelope is [0.7384, 1.3543]. Strong serial
labels are 143.720/140.732 ms; public labels 78.259/74.825 ms. This case also does
not establish a speedup. Ratios of independent medians are not paired ratios.

Final-binary disassembly confirms inverse construction and four u32 directory
stores disappear in N while the per-block memmove/visit loop remains. This proves
that the intended machine-code intervention occurred, not its time savings.

## Observed regression and pause decision

For reverse u32 at 65,536 records, original O labels take 89.621/89.334 microseconds
per sort; N takes 126.386/125.034 and R 125.963/129.135. Placement takes 88.395.
Paired N/O is 1.4133 and R/O 1.4469, both 9/9 slower and outside the full
repeat/placement envelope [0.7677, 1.3027]. N/R is 0.9880 and inconclusive.
This end-to-end regression is shared by N and refactor-only R; it cannot be
attributed to eliminating map work or dismissed as noise.

The input occupies 256 KiB, below the frozen 1 MiB parallel threshold. A separate
untimed entry-counter probe confirms zero ordered-root entries, with a complete
reference pass. It does not alter any timed source or executable. Further
read-only inspection follows the actual O/N/R call-table entries: this strictly
descending fixture returns through the descending-order probe and inline stable
reversal, without calling even general compaction. The three reversal hot loops
are byte-identical (77 bytes). Normalized general-compaction bodies also match
at 602 non-padding instructions, with no retained extra wrapper call. An extra
compact call therefore does not explain this result. Top-level specialization
and code addresses/layout differ; the cause of the measured latency remains
unproved. Identical instruction bytes are not proof of identical runtime costs.

The next narrowly controlled implementation preserves the entire original
block.rs, including the generic compact_with function, byte for byte and adds
a separate ordered implementation. Its evaluation must retain the old N/R
controls, the observed regression, and the two random primary cases. Source
preservation is not a prediction that the regression will disappear.

## Exact coverage and retained evidence

All 33 cells completed for each of u64, Row16, u32 and full-width u128: three
scales times eleven distributions (random, d16, d256, skew25/50/87.5/99, sorted,
reverse, equal, sorted99). Seven Row64 cells completed: skew25/d16/random/sorted
at n=1,024, and d16/skew25/skew50 at n=1,048,576. Other Row64 cells and all
Row128/full-u128-Row128/Owned32 cells are unstarted. None of the completed N/R
comparisons passes the predeclared all-control acceptance rule. A planned matrix
is not treated as a completed one, and fallback cases are not called coverage
of the new ordered entry. Subsequent non-timed entry probes checked all 139
completed matrix cases and both primary cases: 25 matrix cases and both primary
cases reach the ordered entry. Every probe matched the timed input fingerprint
and full output reference. The remaining measured cases exercise fallback paths,
not the new root specialization.

The old frozen packages, 137-file input manifest, binaries, raw samples, exact
cell list, pause boundary, strict parser results and disassembly are preserved
outside Git under the ordered-map screen evidence. The primary binary SHA-256
is bce3bf6ea457da40c1121b62896397817de7bdd667457bcb60c9fb20cbae9871.
This report summarizes results; raw benchmark data and harnesses are excluded.
