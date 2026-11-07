<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Async rank-exchange directional screen: not adopted

A sealed Copy u64/Row16 prototype replaced natural-path restoration with globally ordered exact-target exchanges. Barrier joins restoration before sorting public children; RetireInline starts a public child when its final contributing owner retires. Both schedules use the same mover and retained producer state. No generic owned-record or arbitrary-key-callback guarantee is claimed.

The test used 10,000,000 SplitMix64 seed 1 records per shape, eight workers, P8/R256/shift56 and default B128/B64, release rustc 1.99.1 and observed AVX-512 dispatch. Existing strongest native, public parallel, matched native serial and natural-path/public-child controls were preserved. Public is experimental commit 55b299b3716cd1af9e997f30b1a3c1813455d8be, not current main. Native is the previously established combined/native-root pipeline derived from that public revision, with shard scatter, parallel permutation, serial squeeze and unchanged fixed consumers.

One shuffled warmup and three shuffled measured blocks, twelve duplicate labels per block, produced 96 samples and 150 fully verified sorting calls. Measured samples used fresh inputs until their first complete-call crossing of 50 ms. Every result passed a full typed stable oracle and output fingerprint. All warmups and crossing tails were retained; there were no reruns or excluded observations. Duplicate labels used exactly the same concrete function body.

## Absolute median times

These are medians of three within-block duplicate-label means, in milliseconds.

| Group | u64, ms | Row16, ms |
|---|---:|---:|
| P: Frozen public parallel | 33.039932 | 61.629919 |
| N: Strong native parallel | 23.434840 | 45.734541 |
| SN: Matched strong native serial | 78.723601 | 122.480603 |
| C: Natural-path/public-child root | 28.996949 | 42.933737 |
| B: Async Barrier | 34.563158 | 49.666407 |
| I: Async RetireInline | 30.949345 | 45.009132 |

## Candidate/native paired ratios

Ratios were calculated within each block before taking their median, not from the absolute medians above.

| Shape | Schedule | Block 0 | Block 1 | Block 2 | Median |
|---|---|---:|---:|---:|---:|
| u64 | Async Barrier | 1.474862 | 1.304791 | 1.098868 | 1.304791 |
| u64 | Async RetireInline | 1.320655 | 0.993094 | 1.932091 | 1.320655 |
| row16 | Async Barrier | 1.043929 | 1.090327 | 1.024090 | 1.043929 |
| row16 | Async RetireInline | 1.415528 | 0.988086 | 0.871613 | 0.988086 |

A/A variation was substantial: native/Barrier/Inline reciprocal envelopes were 1.199/1.344/2.832 for u64 and 1.292/1.147/1.404 for Row16. Neither schedule showed a consistent native-relative benefit beyond its controls. Barrier's Row16 improvement over the older public control was favorable in all three blocks beyond their A/A envelope, but that control was not the strongest native baseline. This was only a three-block direction screen without independent code placement, so it is not formal performance acceptance.

A separate twelve-call heap gate passed. Candidate conservative requested-live peaks were 8,748,750/8,748,902 bytes for u64 Barrier/Inline, and 11,974,556/11,974,572 bytes for Row16. These were 3.1581× and 3.0688× the corresponding strong native serial normal requested-live peak, below the experiment's predeclared 4× cap. Both normal and conservative retained balances returned to zero. Conservative accounting reserves the possible entire realloc replacement while the old allocation is still counted; neither it nor normal requested-byte accounting measures physical allocator peak, RSS, rounding or metadata. Capacity ledgers were reported separately, never added to the peak. This fixed-size gate is not an asymptotic memory proof.

Measured restoration performed approximately 2.87–2.96N logical record copies. Each invocation's zero/one/three-copy block counts was retained; counts changed with scheduling. These counts and the absence of a demonstrated net native-relative benefit do not prove that extra movement alone caused the result.

Decision: do not adopt, and do not expand to acceptance testing or tune this candidate. The existing strongest native pipeline remains the performance control. Source, protocol and raw supporting evidence are retained separately for audit; machine-specific/private logs are not part of this publication summary.
