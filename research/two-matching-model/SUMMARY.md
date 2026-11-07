<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Two-matching restoration: negative equal-copy-cost model gate

A bounded metadata experiment evaluated the standard decomposition of a permutation into two swap matchings. This is not a novel sorting algorithm. For destination-to-source cycle c[i] → c[i+1], execute reflection A(i)=−i mod L and then B(i)=−i−1 mod L, with each B swap depending only on A swaps incident to its endpoints. Completion of an endpoint's final operation permanently retires it from root access. The model retained the existing stable-rank packed directory, ghost completion, and bucket geometry; no count, scatter, leaf, or serial baseline changed.

## Decision and interpretation

Stop this candidate before a native mover under the predeclared equal-copy-cost readiness/work gate. This is a prioritization decision based on a negative ideal finite-worker model, **not proof that no implementation could ever win**. Every swap was charged three equal-cost logical whole-block copies. The temporary block may remain cache-resident; DRAM traffic, cache behavior, scheduling/locality effects, and actual bandwidth were not measured. Approximately three times the logical copies must not be described as three times DRAM traffic.

For M physical slots and C cycles (including identities), the two matchings use exactly M−C swaps and 3(M−C) logical block-copy operations. Dependency depth is at most two swaps, six modeled copy steps. P finite workers still have to execute the aggregate work. Root temporaries require at most PB record cells, with O(n/B+PR) metadata; native capacities and simultaneous child workspace were not measured.

## Four actual directories

The table reports bucket-priority scheduling. Ratios expressed as integer fractions are exact. All completion values are **ideal logical block-copy steps, not milliseconds**. Weighted readiness means the record-count-weighted mean bucket-ready step. The old control includes its serial pre-snapshot copies; its recorded maximum job clock already includes any charged final tail, which was not added twice. These four inputs have no partial tail.

| Input | Logical copies, new / old (exact ratio) | Completion, new / old (exact ratio) | Records ready by old completion | Weighted readiness / old completion |
|---|---:|---:|---:|---:|
| u64 random 10M | 246606 / 82246 | 30828 / 11204 | 0.1485467 | 1.9833666451981435 |
| Row16 random 10M | 480900 / 160353 | 60114 / 22141 | 0.1485467 | 1.976906670570435 |
| Row128 random 1M | 105813 / 35322 | 13227 / 4467 | 0.148109 | 2.0699515990597717 |
| Row16 spread d16 10M | 475227 / 158466 | 59406 / 20556 | 0.1248635 | 2.194518900043783 |

| Input | Weighted readiness / new completion | Median nonempty-bucket readiness / new completion | Records ready by 90% of old completion |
|---|---:|---:|---:|
| u64 random 10M | 0.7208265178668742 | 0.826391592059167 | 0.1328677 |
| Row16 random 10M | 0.7281280665585388 | 0.8364607246232159 | 0.1289685 |
| Row128 random 1M | 0.6990605422998413 | 0.7972329326377864 | 0.128539 |
| Row16 spread d16 10M | 0.7593598375467124 | 0.8805676194323806 | 0.0624735 |

Early retirement exists for a small prefix, but the weighted benefit is unsupported against the absolute old-completion reference. Fractions of the new, longer makespan alone would overstate the result. A layer-first queue was also modeled: it released zero records by old completion for every input, and its weighted readiness divided by its own completion was respectively 0.9983190346632931, 0.9992173519912166, 0.9962102383760489, and 0.9999337148267852. It had the same logical work and makespan as bucket-priority scheduling.

## Validation scope

Safe-token checks passed 5,825 schedule cases with 64,272 counted transition assertions. This count includes two conjunctive endpoint checks per issued swap and one exact-token check per operation-retired target; other validation/final-state assertions are excluded. Cases cover all permutations through six physical tokens at P=1,2,4 under two deterministic policies, plus injective maps through five slots, ghost slots, empty buckets, shared boundary blocks, and reserved partial tails. These are not all possible topological schedules. Four actual-directory simulations passed both policies. Python assertions were mandatory and optimized mode explicitly rejected.

No native record mover, sort, benchmark, elapsed performance measurement, pointer/provenance validation, child contention, arbitrary user callback, panic recovery, or Rayon reentrant-progress test was performed. Generic safety and progress remain unproved. The ordinary native acceptance work was not modified. No further candidate execution was performed for this publication packet.

`ORIGINAL_HASHES.sha256` identifies the original predecessor sources, four CSV/JSON inputs, and executed model source by SHA-256. Those artifacts are references only and are deliberately absent from this aggregate-only packet. The packet contains no raw directory, per-bucket trace, model harness, source-record identifiers, or absolute filesystem path. `PRIVACY_AUDIT.md` records its narrow publication scope; `MANIFEST.sha256` covers the included files.
