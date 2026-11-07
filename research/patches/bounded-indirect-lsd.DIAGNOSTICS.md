# Controlled geometry and exact-shape diagnostics

Immutable production base: `83eb25a383f33b62b50ac00a80b87cb6fe0d7788925ab07ef91e9dac2d24e9ff`. Raw diagnostic evidence identity: `379365f0cf36d5246417607ccd5552a0a3287a722c32cef2354063e716d0b886`.

A separate clone changed only lib.rs to load an extra cfg(test) module, added exact_shape_diagnostics.rs, and printed actual initialized root/shard fields in block/parallel.rs. The frozen production directory was never overlaid and its complete manifest was reverified afterward. No diagnostic hook or counter entered the timing dependencies. This diagnostic overlay is not in the archival patch.

All diagnostic runs were optimized release tests, offline -j1, under shared FD9 on device 27/inode 1578019. They collected no timings. TESSERA_* variables were explicitly cleared and affinity 0-8 was recorded. The lock was released and its shell exited. Original source/command/environment/log hashes remain in the external evidence manifest.

## Actual root geometry

The exact 48-byte Owned fixture, n=131072, original 24-bit movement key and P2/P4 were executed without SIMD, matching the failing public test's feature configuration. The printed geometry reads the initialized root and shard Sorter fields; it is not merely a formula evaluated by the test.

- Both runs: actual root digit width 8, R=256 and B=32.
- P2: two shards of 65536 records, each with 2048 full input source blocks.
- P4: four shards of 32768 records, each with 1024 full input source blocks.
- Every shard: stored partial_tail=0, partial_total=0 and scratch_full_sources=0.
- Both public sorts observed non-origin worker activity and zero callbacks on outside-input record addresses. Each used 256 indirect leaves, 512 index passes and 132298 logical cycle copies.
- Stable order, every identity/payload and exactly one final drop were checked.

This controlled observation confirms the default-configuration explanation of the original moved-address probe failure. Initial root sources are whole input blocks with no scratch tail, and the indirect children have no post-movement key calls. It does not retroactively assert that the original failed command captured its own geometry or environment.

## Exact benchmark shapes

Types and payloads match the owner's existing four-case harness: repr(C) Row64 {u64 key, u64 original_id, [u8;48]}, Row128 with [u8;112], U128Row128 {u128 key, u64 original_id, [u8;104]}, and Owned32 {u64 key, String payload}. Payload byte at offset j is id.rotate_left(j % 64).wrapping_add(j * 31) as u8; the owned string is the same fixed-width hexadecimal id plus tessera-owned-payload suffix. SplitMix64 starts at seed 1; u64 uses one draw, u128 uses the first draw as high half and second as low half.

Public serial and parallel by-key calls execute inside an eight-worker Rayon pool with portable SIMD enabled. Wide shapes use default64; only Owned32 uses explicit32. Every worker's counters and force-literal TLS are reset before each API/control call. Literal forcing is a cfg(test) diagnostic override; actual timing uses the compile-time feature-off package.

Every output is compared in full against a separately cloned stable standard-sort oracle, including payload and tie order. Those O(n) oracle allocations are outside Tessera's auxiliary-memory claim. All sixteen shape/API/control combinations passed. Aggregate serial and parallel counts match:

| Shape | N | Literal MSD leaves | Literal scatter + copyback copies | Indirect leaves | Index passes | Cycle record copies |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Row64 | 1048576 | 2048 | 2097152 | 2048 | 14336 | 1058395 |
| Row128 | 262144 | 2048 | 524288 | 2048 | 14336 | 269240 |
| U128Row128 | 262144 | 2048 | 524288 | 2048 | 30720 | 269222 |
| Owned32 | 262144 | 256 | 524288 | 256 | 1792 | 263568 |

Serial used one active leaf worker and parallel used seven active leaf workers in the W8 pool, consistent with a separate producer. This does not claim eight leaf workers.

## Limits

The literal planner selected one MSD scatter plus copyback per leaf, not eight full-record passes. Cycle copies include the saved temporary: L+1 per nontrivial length-L cycle, zero for fixed points. The counts exclude root partition/permutation/compaction and physical DRAM traffic. The generic insertion_swaps metric includes encoded-key order-probe swaps, so it is not a pure record-movement counter and must not be multiplied by payload size.

Index passes, validation, extra allocations and random cycle addresses can outweigh fewer leaf payload copies. No speed result follows from these diagnostics; the separate end-to-end screen was negative. Raw geometry/route logs and source overlays remain external, with hashes referenced in external-evidence.json.
