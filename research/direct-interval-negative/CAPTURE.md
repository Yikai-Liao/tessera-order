<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Native capture contract

Source review: `../tessera-combined-current-main/rust/tessera-sort/src/block/parallel.rs` and `block.rs`; this frozen research tree is not asserted to be the latest published runtime. Hook after local ownership transfer, `drop(tasks)` and `drop(local_buffers)`, before the first `compact::permute_blocks` call. Export all relevant source hashes/revisions alongside the capture.

The adopted global `bucket_count` is H=P_root*R, not the semantic radix R. Scratch physical IDs are [0,2H). Each retained allocation contains 2R blocks. For a scratch physical ID s, allocation is `scratch_segments[s/(2R)]`, local record offset `(s%(2R))*B`. Never concatenate those allocations into pointer arithmetic. Input physical ID s>=2H maps to global original input record `(s-2H)*B`.

Original shard bounds are quotas of full B-record blocks; only the last shard includes the original short tail. `BlockStorage::new` initially copies that short tail to its shard scratch. The subsequent stable scatter can redistribute it, so **do not assume that original-tail records still form one distinguished prefix at the pre-permutation hook**. Export only the authoritative active directory and its initialized prefix lengths. There is no authoritative original input block at F=floor(n/B). Destination intervals may still extend into final input [FB,n), but this region contributes no original-source dependency.

Traverse logical positions `global.bucket_count..global.active_blocks_end`, using exactly `next_block_len`'s semantics or the matching sorted `partial_blocks` map. Full live blocks have length B; a partial entry overrides that length, including zero. Omit zero prefixes from the node graph. Advance destination by live length only. Bucket boundaries must use the supplied semantic `ends`, not `global.bucket_count`. At a repeated empty-bucket end, skip to the next nonempty bucket before assigning a positive-length node.

The directory was constructed bucket-major, then original shard order. This is where stable provenance is verified. Exporting `(semantic_bucket, original_shard, local_logical_position)` is recommended even though the first graph parser only consumes semantic bucket and stable node order.

JSON required fields:

- `schema`: `native-live-prefix-v1`.
- `hook`: `adopted_before_permute_blocks`.
- `n`, `record_bytes`, `root_shards`, `workers`, `radix` (bucket count), `block_records`.
- `scratch_segment_capacities_records`: actual capacities of the P_root independent Vec allocations.
- `semantic_ends`: R cumulative record ends.
- `nodes`: positive live prefixes, stable logical order, fields `node`, `bucket`, `physical`, `allocation` (`input` or `scratch`), `segment`, `offset_records`, `length`, `destination`.
- `root_and_retained_aux_capacity_bytes`: all algorithm-owned auxiliary capacities simultaneously live at this hook, including root scratch, native directories, all caller/child-pool retained allocations, and vector/struct storage. Exclude only the external input/reference arrays, warmed Rayon runtime and diagnostic capture. The probe reports unknown total-space eligibility when this is absent. Do not include the proposed new graph or spill in this field; the probe adds them explicitly.

Recommended additional manifest fields: source and leaf-core hashes, features, input generator/seed/fingerprint, root digit shift/width, original shard full-block quotas, all existing root and caller-retained allocation capacities, and original directory hashes. Those fields are provenance and heap evidence, not optional substitutes for the exact mappings above.

For an exact baseline split-cycle replay, also preserve the entire `block_order`, H, `active_blocks_end`, and complete `partial_blocks` (including zero prefixes), or the baseline's actual job/snapshot plan plus squeeze copy counts. Active live nodes alone do not determine the artificial completion of unused scratch/free blocks and therefore cannot reproduce its exact old-cycle copy schedule. Baseline replay is not yet implemented in this source-only probe.

The first simulator uses Python integers and O(V) metadata objects. That is not a production Rust byte bound. Its JSON result can be O(V) events/edges and must never be included in sorter allocation/timing measurements. It does not execute records, allocate an n-record or n-bit oracle, or validate Rust provenance. All files here are source-only until a shared CPU window is authorized.
