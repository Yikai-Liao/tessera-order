# 0014: Use scalar extraction for default insertion finishing

Date: 2026-11-07. Status: accepted.

Amends the default finishing scan in [decision 0007](0007-optional-simd-backend.md).
The shared insertion loop and separate finishing entry point from
[decision 0008](0008-insertion-finishing-entry-point.md) remain in use.

## Context

Commit `72c2984` introduced a blocked adjacent-pair scan and switched continuous
measurements to the optional SIMD build. Default measurements resumed at
`c245c18`, which changed the measurement workflow without changing the Rust
sources. Its Default result for `u64/random/65536/s1/api` was 762.36 microseconds;
the preceding Default result at `8bab76a` was 721.51 microseconds. The latest
measured main commit, `26b340c`, still took 747.43 microseconds. These are
continuous regression signals, without paired A/A or placement controls.

The default scan checks four pairs before forming a mask over 32 pairs. At
the baseline x86-64 target, its `u64` mask expands into scalar comparisons,
shifts and ORs. Insertion finishing after a radix pass repeatedly encounters
short ordered runs separated by inversions, including when it finishes the
whole tile at once. Repeating the blocked scan there costs work and expands
the finishing loop. Budgeted probes also encounter long ordered stretches,
where replacing the blocked scan outright can lose performance.

## Decision

Have the sealed default `finish_insertion` implementation pass an identity
`ByKey` extractor to the existing engine loop. That extractor scans scalar
and retains the previous encoded key. Primitive SIMD overrides retain their
existing vector finishing path; fallback types continue to use the sealed
default. Budgeted insertion retains its existing scan.

Reuse the extractor and loop rather than introducing another sorting loop,
backend, size threshold or distribution probe. The choice follows the
operation: a budgeted probe and an unbudgeted finish already have distinct
entry points.

## Validation

A temporary harness outside the repository compares `26b340c` with the change
in one release binary, on a Ryzen 7 8845H pinned to CPU 4, using Rust 1.99.1,
the baseline x86-64 target, and no native CPU flags or LTO. It reproduces the
continuous suite's 40 cases, including API and prepared-workspace modes, with
SplitMix64 seed 1. Each case runs nine shuffled blocks, with the main baseline
timed twice per block, an independently named copy of main, and two
independently named copies of the fix. Batches target 60 ms with a 256 MiB
prepared-buffer cap. Input restoration and exact output verification against
a stable standard-library reference are outside the timed region; row
verification includes original indices and float verification includes bits.

Ratios below are medians of `variant / mean(main A1, main A2)`. A/A is the
per-block range of `A2 / A1`; placement is the range of the second main copy.
Both copies of the fix agree on the direction in all nine blocks for these
random `u64` cases:

| Length | Mode | Fix | Second fix copy | A/A range | Main placement range |
|---:|---|---:|---:|---|---|
| 8,193 | API | 0.9400 | 0.9158 | 0.9882–1.0115 | 0.9854–1.0006 |
| 8,193 | workspace | 0.9500 | 0.9416 | 0.9948–1.0076 | 0.9942–1.0033 |
| 65,536 | API | 0.9487 | 0.9160 | 0.9971–1.0084 | 0.9951–1.0036 |
| 65,536 | workspace | 0.9442 | 0.9181 | 0.9944–1.0020 | 0.9951–0.9991 |
| 1,000,000 | API | 0.9670 | 0.9447 | 0.9756–1.0282 | 0.9834–1.0176 |

The 99%-sorted cases retain their budgeted scan. Replacing that scan with a
streaming scalar scan was rejected: in an earlier paired experiment,
`u64/sorted99/65536` had a median ratio of 1.2879, outside its A/A range
0.9888–1.0105 and placement range 0.9938–1.0180.

The broad run also found a small cost on `u64/sorted_tail/65536`: fix ratios
1.0219 and 1.0108, with A/A 0.9923–1.0038 and placement 0.9763–1.0056.
This is investigated separately rather than dismissed as noise. Long ordered
stretches can still occur during a finish; scalar extraction trades their
blocked scan for cheaper repeated short scans. No extra distribution or
size gate is added to hide that trade-off. Two targeted reruns retained
positive median changes, but their wider controls did not establish a
regression independently. The small cost remains a point for Intel runner
validation.

Ordered-input timing also moves with code placement. The root `u64` sort
bodies of main and the fix have the same instruction sequence after address
relocations are normalized, and sorted inputs return before the changed
finishing operation. Their apparent speedup is not attributed to the scan
change. Raw samples, manifests, executable snapshots and disassembly stay
outside the repository. These local results do not establish the size of
an improvement on Bencher's Intel runner. A paired Portable SIMD check of
`u64/random/65536/s1/api` remained within its controls; the changed default
finishing body is overridden for that primitive SIMD path.

Default, no-std and Portable SIMD tests, doctests, Clippy with warnings denied,
and rustdoc with warnings denied pass. Miri passes the library tests,
integration tests and doctests. The local MSRV toolchain is unavailable;
compatibility with Rust 1.85 still needs its CI job.

## Consequences

Routing, digit planning, tile sizes, scratch allocation, and the public
ordering, stability and panic contracts are unchanged. User key functions
keep their existing scalar scan and evaluation order. The default primitive
finishing path uses the same sealed encoding through an identity key
function; it does not materialize a parallel array of keys.
