<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Three causal controls: negative-result checkpoints

**Not adopted.** These are independent attribution experiments on the historical strongest combined pipeline, not new production sort implementations. They do not stack, and are not drop-in changes to the public scheduler at `0e50926e873b014de1d37a67a6da66da368010d0`.

All main screens used exact-source original and independent controls, fresh pool-worker input clones, full stable references, randomized blocks and retained tails. Source, scripts, raw timing events and full logs remain external. No raw sample table is reproduced here.

| Control | Aggregate result | What remains unresolved |
|---|---|---|
| Fixed root P4, actual pool W4/W8 | W8/P4 did not establish a win over original W8/P8. Paired ratios were 1.085 Row16 and 1.109 u64, inside A/A noise. Extra pool/child capacity helped at fixed root geometry descriptively in all nine blocks; the u64 ratio .753 cleared the relevant controls, Row16 .811 did not. | Root geometry alone does not explain waiting CPU. Pool/child capacity changes are not whole-algorithm pure scaling because root scatter/permutation retain four jobs. |
| Fixed 50µs Empty-path sleep | Empty-wait CPU fell 83–92% in paired diagnostic traces, but no end-to-end improvement cleared A/A and placement controls. Main sleep/original ratios were .948 Row16 and .975 u64. Typical pickup delay increased. | Waiting is real CPU cost, but removing most of it is not sufficient to remove the wall critical path. This changes the whole waiting strategy, not pure yield instruction cost. |
| Deferred consumers until actual channel closure | No established win: deferred/original ratios were 1.120 Row16 and 1.124 u64, within wide controls. Children still used all eight workers, but mandatory post-root tails became about 12–16ms Row16 and 9–10ms u64. | Root-side changes were inconsistent; pre-spawn scatter/permutation negative controls also varied substantially. No isolated DRAM or bandwidth attribution is supported. |

The fixed-root screen used nine randomized blocks with original W4/W8 and fixed W4/W8 A/A labels. Sleep and deferred screens used nine blocks with original/candidate/independent-placement A/A labels. Each had a full verified warmup. Separate three-block traces were never mixed into main timing. A/A and placement tails were wide enough that direction alone did not establish every apparent difference. Full ratios, ranges and failures remain in the external reports identified by hashes.

Geometry and logical copy counts were explicitly checked. Logical copied records are not measured DRAM traffic. Hardware CPU-cycle events were unavailable; software thread CPU clocks worked. No cache-miss, memory-controller bandwidth, global CPU-utilization fraction or quota cause was established.

No further implementation, benchmark, parameter sweep or production promotion is part of this archive. These controls preserve useful falsification results: fixing geometry, reducing wait CPU, or removing overlap alone did not yield a demonstrated replacement for the original combined pipeline.
