<!-- SPDX-License-Identifier: BSD-2-Clause -->
# PRELIMINARY current-core owned-rank direction and space

This is a bounded direction checkpoint, not an accepted performance gain, a broad winner, a default dispatch policy or runtime adoption. No kernel implementation changed. Copy fails the consistent native-relative direction gate (2/3 wins), with no parameter rescue. Owned has a direction lead, but its native comparison is inflated by a noisy third block. Formal nine-block and genuinely independent placement gates remain pending separately.

## Exact experiment and controls

Two exact 128-byte layouts (CopyWide and String-owning OwnedWide), n=262144, SplitMix64 seed 1, K=P=8 and B=64. Prebuilt W1/W8 pools shared allowed CPUs 0–7; the controller used CPU 8. This checked affinity is not independent placement evidence. S1/S2 invoke the same current serial function on W1; N1/N2 the same current native function on W8; R8a/R8b the unchanged rank kernel on W8; R1a/R1b that kernel on W1 with the same K/B. All use the same key-function type. One randomized verified warmup block preceded exactly three randomized measured blocks per layout. Each measured label accumulated at least 25 ms using fresh individually timed calls; preparation on actual pool workers and complete stable-record oracle checks were outside timing. All 164 calls, including 16 warmups and every tail, were retained without reruns or sweeps.

Each label duration is total sort ns divided by call count. Each pair duration is the geometric mean of its two labels within a block; overall ratios are geometric means of the three block ratios. A/A below lists first-label/second-label for every pair and block; symmetric A/A is max(r, 1/r). These three-block aggregates cannot establish an effect outside controls. Raw samples, harnesses and binaries remain outside this summary archive.

### CopyWide

- N pair ms by block: 8.220986, 8.265667, 7.266059.
- R1 pair ms by block: 19.351179, 20.146095, 19.982576.
- R8 pair ms by block: 8.252085, 5.435892, 5.886883.
- S pair ms by block: 15.423182, 15.835298, 16.155213.
- N A/A first/second: 1.095396, 0.994160, 1.032977; symmetric: 1.095396, 1.005875, 1.032977.
- R1 A/A first/second: 0.965402, 0.914034, 0.935117; symmetric: 1.035838, 1.094052, 1.069384.
- R8 A/A first/second: 1.225331, 1.038384, 1.041273; symmetric: 1.225331, 1.038384, 1.041273.
- S A/A first/second: 0.939428, 0.993037, 1.013351; symmetric: 1.064477, 1.007012, 1.013351.
- N/R8: blocks 0.996231, 1.520572, 1.234280; geometric mean 1.231952.
- R1/R8: blocks 2.345005, 3.706125, 3.394424; geometric mean 3.089892.
- R1/S: blocks 1.254681, 1.272227, 1.236912; geometric mean 1.254524.
- S/R8: blocks 1.869004, 2.913101, 2.744273; geometric mean 2.463000.

Measured individual-call min–max ms (all tails kept): S 14.682047–16.817439; N 6.662004–9.564277; R1 18.606388–21.981619; R8 5.052081–17.318755.

### OwnedWide

- N pair ms by block: 7.952099, 7.359780, 19.435627.
- R1 pair ms by block: 18.677907, 18.748432, 19.400785.
- R8 pair ms by block: 6.309306, 5.319455, 5.637936.
- S pair ms by block: 15.876886, 16.755447, 15.970300.
- N A/A first/second: 0.810361, 1.092834, 0.600742; symmetric: 1.234018, 1.092834, 1.664608.
- R1 A/A first/second: 0.987058, 1.017581, 0.858379; symmetric: 1.013111, 1.017581, 1.164987.
- R8 A/A first/second: 1.001494, 1.062581, 1.024242; symmetric: 1.001494, 1.062581, 1.024242.
- S A/A first/second: 0.993755, 1.144737, 0.987606; symmetric: 1.006284, 1.144737, 1.012549.
- N/R8: blocks 1.260376, 1.383559, 3.447295; geometric mean 1.818272.
- R1/R8: blocks 2.960374, 3.524503, 3.441115; geometric mean 3.298992.
- R1/S: blocks 1.176421, 1.118946, 1.214804; geometric mean 1.169390.
- S/R8: blocks 2.516424, 3.149843, 2.832650; geometric mean 2.821122.

Measured individual-call min–max ms (all tails kept): S 14.571967–19.771712; N 6.890928–36.919510; R1 17.858791–21.457778; R8 4.848463–7.110218.

Owned native block 3 includes 36.920 ms and 25.076 ms tails, with native symmetric A/A 1.665, versus candidate maximum 1.063. The 1.818 overall native/R8 ratio is therefore not accepted gain. Copy candidate A/A reaches 1.225. R1 is slower than current serial in both layouts, so R1/R8 alone is not sufficient evidence. No phase duration substitutes for the end-to-end result.

## Separate requested-byte space observation

A separately instrumented executable reused the identical record layouts, generator, controls and cached dependencies. Each of eight labels per layout received one exact-operation/pool warmup and one recorded call. All 16 full-reference checks passed, and every retained requested-byte difference was zero. The allocator self-test passed before each layout. Ordinary timing was not rebuilt; allocator timings are not performance evidence.

The tracker measures aggregate process requested bytes linearized after successful allocation/reallocation and before deallocation forwarding. Extra peak is maximum live requested bytes minus the pre-sort baseline. Inputs, Strings, reference, warmed pools and fresh worker-local clone precede baseline; kernel cleanup and participating-worker runtime allocations are included. Oracle checking and input destruction follow the snapshot. This is not RSS, malloc-internal peak, allocator rounding, stack usage or a universal upper bound. No runtime allocation was subtracted.

### CopyWide heap

Recorded peaks in bytes: S1=2219040, S2=2219040, N1=17554848, N2=17555248, R8a=8753320, R8b=8759208, R1a=3203120, R1b=3203120. All retained differences: 0.

Conservative max-R8/min-serial: 3.947296; geometric pair R8/serial: 3.945969. Peak A/A first/second: S=1.000000, N=0.999977, R8=0.999328, R1=1.000000.

### OwnedWide heap

Recorded peaks in bytes: S1=2219040, S2=2219040, N1=17553376, N2=17551004, R8a=6477352, R8b=6633128, R1a=3203120, R1b=3203120. All retained differences: 0.

Conservative max-R8/min-serial: 2.989188; geometric pair R8/serial: 2.953880. Peak A/A first/second: S=1.000000, N=1.000135, R8=0.976515, R1=1.000000.

Both observed layouts are below the provisional 4× serial extra-peak gate, narrowly for Copy. This clears only the measured space gate. One recorded call per label is not a universal guarantee. The Copy/Owned peak difference is not causally attributed to scheduling without a trace.

## Binding, source and safety scope

The three files in [the existing owned-rank checkpoint](../README.md) were reused byte-for-byte, including the frozen multi-panic tests. All six rank features were enabled: padded, cursor-cache, inline-step, pull-restore, parallel-restore and loser-tree. Comparison/allocation-counting features were off; existing clocks, worker masks and structural counters remained. Native compilation used rustc 1.99.1 (58cf8f9c3 2026-10-13), edition 2021, opt-level=3, codegen-units=16, panic=unwind, debuginfo=0 and stripped debug information, with no target-cpu override.

The three rank local-sort calls were explicitly bound through the tessera_sort extern alias to the frozen generic_combined rlib. Serial and native controls used that same rlib and the same Rayon identity. Its cached fingerprint has default, portable-simd, rayon and std enabled and no rustflags. The historical relative Cargo dependency does NOT automatically reproduce this cached research baseline. Binary hashes in provenance identify actual inputs rather than asserting Cargo rebuild equivalence.

The native source provenance is the [existing pinned native-entry-validation reconstruction](../../native-entry-validation/README.md): public 55b299b3716cd1af9e997f30b1a3c1813455d8be plus only the combined patch from published 5d06e2cc25704901b1a6838ef53416c0da39fe98, then validation-tests.patch. Do not apply the draining patch. Its original combined source freeze is bf3b2ea730f98fca2dfaf75f3baba732f7f5d67eaea8bafdbae17c684fe11bae. Native validation-test additions do not change the production algorithm. Source and binary identities are separate: Miri rebuilt current scalar-std metadata from that reconstruction, while native timing reused the verified cached combined library.

All 12 optimized kernel test groups and both exact-layout oracles passed, including payload corruption and equal-key-order negative checks. Four strict default-Stacked-Borrows Miri scoped filters passed: tests::owned_miri_smoke, tests::owned_miri_multiple_job_panics, parallel_restore::tests::owned_miri_scoped_restore and parallel_restore::tests::miri_scoped_restore_kernel. Miri used 1.101.0-nightly (c980177d8 2026-11-06), strict provenance and symbolic alignment, with newly built current scalar std metadata. This does not certify actual Rayon execution or native portable-SIMD feature parity. The first metadata command failed before compilation on an obsolete flag; the installed Miri compiler-mode amendment then passed. That failed attempt remains part of the retained evidence. No new MSRV claim is made.

## Space reasoning and remaining gates

See [the ownership proof](../OWNERSHIP.md) for move/restore invariants. For p=K=P, M=ceil(n/B), D<=min(M,p(p-1)+tail), the structural bound is E=3p²+11p+D+2 extra record blocks. Here D<=56 and E<=338, giving at most 21632 scratch records (2768896 bytes); observed structural counters were 21504 records and 63083 metadata slots. Neither counter is the allocator peak.

Current local-sort workspaces die before later merge scratch is allocated: their lifetimes do not overlap. The peak uses the maximum of these phases, not their sum, while charging global metadata and all concurrent local workspaces. Choosing B approximately sqrt(n)/p yields O(p sqrt(n)) aggregate and O(sqrt(n)) local only with an explicit p<=sqrt(n) policy. The historical API itself only caps p by M, and callback allocations/panic payload bytes remain unbounded. No universal API space claim follows from this screen.

Later acceptance would require at least 8/9 paired wins, at least 10% margin outside candidate-specific controls, independent placement evidence and the measured space gate. None of those timing/placement gates is completed by this checkpoint. No runtime, Cargo, main branch or user pull request is changed.
