<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Result: mechanism confirmed, no accepted net win

## Correctness and actual scan coverage

The exact retained focused source passed five ordinary and five independently instrumented test groups. All Public/Full/Masked modes and u64/Row16/Row128 were covered by complete stable/payload references: random, sorted/reverse, equal, duplicates, gaps, key windows, nonzero root prefixes, widths1/8/12, empty/small slices, the1,025-key boundary, late variation and block tails. Existing unsafe root movement was not broadened; these tests are not a new Miri, Owned or panic-safety claim.

At10,000,000 records, eight workers and SplitMix64 seed1, both u64 and Row16 had256 actual children totaling10M records. All256 children reached the initial scan and downstream node in every mode. Public and helper-full examined10,000,000 initial-scan keys; helper-masked examined262,400, exactly1,025 per child, with256 early returns. Every candidate/varying mask and fixed prefix was valid. Full stable/payload references passed.

Insertion capture remained unchanged across modes:2,097,152 keys for u64 and1,048,576 for Row16. This is a scan-only mechanism control, not the native prepared entry that also bypasses insertion preparation. Key counts do not measure CPU time or memory traffic.

Acceptance input fingerprints matched: u64/key FNV1a64 `c2333e730de5dd2c`; Row16 key+original-ID FNV1a64 `42d3dd5d34d77c94`. Smaller seed19 compatibility reports matched their preserved predecessors. Independent parsers checked actual children, totals, cross-mode preparation/node evidence and corruption rejection.

## Single frozen timing study

The separately approved W8 study covered sealed Copy u64 and Row16, random seed1,10M records. Current public/native parallel, both current serial controls, and Public-child CP/Full-child CF/Masked-child CM each had own A/A and source-identical placement controls. Every timed candidate used the same current leaf, geometry and retained Instant/Stats cost. Nothing was excluded or rerun after observing results.

There were378 measured samples and42 warmup samples, comprising661 measured full sorts plus42 warmup full sorts:703 complete reference passes. All measured samples met the predeclared first-crossing50ms floor. Separately,42 full-size untimed smoke labels,84 tied/self-test sorter calls and five parser tests passed. These counts are not additional memory or generic-safety evidence.

| Paired median ratio | u64 | Row16 |
|---|---:|---:|
| Masked / Full | 1.024354 | 0.986148 |
| Full / Public-child | 1.101009 | 0.994412 |
| Masked / Public-child | 1.062152 | 0.975819 |
| Masked / current native | 1.127799 | 0.865579 |

Ratios are medians of within-block ratios across nine blocks, not ratios of pooled medians. Masked/Full was faster in4/9 u64 blocks and5/9 Row16 blocks; Masked/native was faster in1/9 and7/9. No key candidate contrast passed both its direction and own-control envelope gates. All-group envelopes were3.659241 for u64 and4.118132 for Row16; no comparison passed the all-group sensitivity envelope. Broad A/A, placement ranges and large tails remain included, with no isolated explanation or dismissal as noise.

**Decision:** archive without adoption. The scan-key saving is real, but this study establishes neither an end-to-end masked speed gain nor a slowdown magnitude outside the predeclared controls. It does not establish no effect. A reduced scan count must not be presented as a net performance gain.

`measurements.json` contains the sanitized paired contrasts and exact source-report/analysis hashes. Complete timing-report SHA256: `f90e74d43413353baefc4a5623d50980ce71aa8f8690fe61130cc310cce865f2`; complete analysis SHA256: `f7970e59be27533604ed948925f93486186f2911375ad54cefdc2359ebdad9b8`. Raw samples, binaries and harnesses remain outside this packet.

Validation used rustc1.99.1 (`58cf8f9c3`), edition2024, opt-level3 and codegen-units16 with std/rayon/portable-simd; counts used separate diagnostic builds. Stripped validation executables were not treated as timing-ready placement controls. No new heap study was performed.
