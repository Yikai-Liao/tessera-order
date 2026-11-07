# Negative same-core LSD screen

**Decision: not adopted.** The fixed-byte LSD index candidate is rejected as the default from this screen. Serial indirect calls were slower in all three blocks for every shape. Parallel wide-case gains were not established outside controls, and explicit32 owned records regressed in all three blocks. The causes of the slowdown and large tails have not been diagnosed. Lower logical leaf-copy counts are not a speedup.

This summary is derived from the final external report SHA-256 `6642aba2de5b6114e9329e99080cd6472c190c467dd3ae5f1867bb5fac152015`, whose original samples, full ratios, build graph and runner remain outside the repository. The report is referenced, not copied here because it also lists every measured sample.

## Controls and scope

A is literal serial, S same-source indirect serial, C literal strongest research combined parallel, I same-source indirect parallel. Each has two repeat labels. A separate C placement copy is byte-identical. P has two repeats but is a distinct, pointer-corrected public scheduler, not another C placement.

The four research packages share all 22 Rust files with validated source identity 83eb25a383f33b62b50ac00a80b87cb6fe0d7788925ab07ef91e9dac2d24e9ff, with isolated off/default64/explicit32 features. Fixed public P is local commit `fdd3c9cc169fe2f1b918bba7bfdae20c16f5b3e3`, manifest identity `7ee0369e596090bd84f587c5e3cf12b76877435d8a2c7c506ea012576c08ae6c`, later published as `0e50926e873b014de1d37a67a6da66da368010d0` with identical tree `5da97e75c400b0921140fb1c5361207bd1fbfeba`. Historical 6c is excluded. Research and public share 5b13 lineage, but not all serial package bytes: research block.rs has additional scratch-segment machinery. P/C is contextual scheduler performance, not the causal mechanism comparison.

All packages enable Rayon and portable SIMD. AMD EPYC 9V74; process mask 0-7; precreated W8 pool; clone and timer on an actual worker; stable rustc 1.99.1 default release, offline -j1, no native/LTO/RUSTFLAGS override. SplitMix64 seed 1, exact repr(C) sizes Row64=64, Row128=128, U128Row128=128 and Owned32=32 bytes. N=1,048,576 for Row64 and 262,144 otherwise. Every payload byte and stable order were checked; input preparation/reference is outside timing and API scratch allocation inside.

One randomized warmup of all eleven labels and three randomized measurement blocks per shape produced 44 warmups plus 132 measured samples; none were discarded. This is a directional screen, not the nine-block/8-of-9 acceptance gate. Some Owned32 parallel calls are below the provisional 5 ms target. The screen's twelve release selftests made 396 verified API calls; four preflights made 44. Correctness-only low-cardinality cases are not speed evidence. Exclusive timing ran 2026-11-07 09:33:39-09:33:53 UTC; a separate shared allocator run followed 09:34:03-09:34:08.

## Timing summary

Times are independent pooled medians in milliseconds. Ratios are medians of three matched within-block pair-mean ratios, not ratios of the displayed times. Smaller S/A and I/C are better.

| Shape | A ms | S ms | C ms | I ms | P ms | S/A | I/C | P/C |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Row64 | 45.756 | 70.758 | 17.604 | 17.560 | 34.659 | 1.5579 | 0.9703 | 2.0462 |
| Row128 | 19.443 | 25.851 | 11.132 | 9.112 | 14.134 | 1.2705 | 0.8559 | 1.3157 |
| U128Row128 | 19.040 | 31.005 | 8.909 | 9.714 | 12.465 | 1.7239 | 1.0187 | 1.3967 |
| Owned32 | 3.277 | 8.852 | 1.876 | 3.869 | 2.458 | 3.0168 | 2.4788 | 1.2508 |

Reciprocal control envelopes are [1/E,E]. Literal E includes A/A, C/C and C placement; full E additionally includes all S/I/P repeats.

| Shape | Literal repeat + C placement E | Full E |
| --- | ---: | ---: |
| Row64 | 1.050866 | 1.118153 |
| Row128 | 1.469948 | 1.469948 |
| U128Row128 | 1.588005 | 6.927308 |
| Owned32 | 1.260584 | 3.329878 |

Row64's parallel result is within controls. Row128's apparent parallel improvement is not separable from its C placement control. Full-u128 has a large retained candidate repeat tail and no established gain. Owned32's serial and parallel regressions occur in every block; wide candidate tails do not erase that directional result. No profiler or CPU-steal telemetry was collected, so none of these observations is assigned to a specific cause or an external process.

## Actual measured peak additional heap

These are warmed, separate-process counting-allocator observations, not RSS or sums of capacity maxima. Inputs/references/deep String clones precede tracking. Values are requested allocation bytes. Each entry has one observation, not a distribution or asymptotic scaling proof. Stack memory is excluded.

| Shape | A peak bytes | S peak bytes | C peak bytes | I peak bytes | P peak bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Row64 | 1,432,224 | 1,405,888 | 9,361,848 | 9,361,848 | 1,755,156 |
| Row128 | 2,219,040 | 2,200,268 | 17,553,822 | 17,360,780 | 2,461,296 |
| U128Row128 | 2,219,040 | 2,200,268 | 17,551,904 | 17,432,784 | 2,549,380 |
| Owned32 | 659,520 | 646,124 | 4,598,256 | 4,561,064 | 731,264 |

Allocation request counts, respectively A/S/C/I/P: Row64 20/39/606/735/435; Row128 24/51/494/564/280; U128Row128 18/33/522/636/288; Owned32 21/44/313/381/131. Indirect raises request counts on every measured API/shape while reducing total requested bytes and generally only modestly reducing peak. All twenty net retained balances were zero. These observations neither establish asymptotic bounds nor certify a numerical memory budget.

Separate cfg(test) diagnostics prove the indirect leaves actually ran and roughly halved literal leaf record-copy counts. They exclude root movement, encoded-key probe swaps and DRAM traffic. Their source was never a timing dependency. See DIAGNOSTICS.md for exact counts. No new parameters, successor mechanism, additional timing runs or production adoption are authorized by this archive.
