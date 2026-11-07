# Scope and preservation proof

Immediately after compact::permute_blocks returns normally, let C be the physical block count, H the free-prefix/bucket count, and S=2H the scratch block count. The order is order[i]=(i+H)%C. For each main-loop destination d in[2H,C), i=d-H gives source=d and inverse[d]=i. Thus no evacuation is needed and all four map stores are identities. For the wrapped tail i in[C-H,active_end), source=i-(C-H).

The new method uses only that established rotation. It removes inverse construction, map lookups/stores and dead evacuation/free-block bookkeeping. It retains the same ptr::copy calls and lengths, including zero-length, self and overlapping copies; partial/tail behavior; next_block_len; segmented scratch addressing; raw ancestor input pointer; and visit_due after each block and after the loops. It does not coalesce copies or substitute memcpy, add inline annotations/prefetch/runtime scans, alter shard/radix/block policy, or change native child work.

The first callback panic is retained and disables later callbacks. Remaining copies finish before input_materialized=true and payload return. The caller then resumes the panic. No new fallible production operation after movement starts was introduced, because generic Drop cannot recover a half-squeezed directory. The next_block_order allocation stays owned, without inverse-map construction. Original capacity/address retention is tested; no new measured peak-allocation claim is made.

The complete original block.rs is unchanged, including compact_with, Drop and raw-pointer helpers. The new entry is used only in the normal-return permutation branch. Serial skip, one-shard fallback, ordinary serial sorting and fault recovery remain generic.

The reference method is test-only. Removing one Evacuate, two Copy and three VisitDue recording sites, its trace parameter/reserve, and its renamed visibility restores every original compact_with byte. A reversible standalone normalization diff is included. The 12,600 native three-way geometries compare untouched generic records/callbacks/capacity/address with the traced reference, then compare that reference's complete copy/visit trace and outcomes with the actual ordered method. This is not direct tracing of unchanged generic production code.

The fixtures include empty buckets, zero/max partials, tails, segmented scratch, owned String+Cell records and non-self overlap. For example n=5, B=3, H=2, Q=1, distribution3 has bucket lengths2/3 and copies three records from input offset3 to offset2 after the first partial. Separate max-radix cases and live scoped &mut child slices cover publication and producer/child/both panic paths. Miri uses reduced geometries and serial root scheduling; it does not claim Rayon/Crossbeam Miri coverage.

The source-only independent review found no blocker. Precise final native/Miri/MSRV coverage is recorded in VALIDATION.json. No timing or performance conclusion is attached.
