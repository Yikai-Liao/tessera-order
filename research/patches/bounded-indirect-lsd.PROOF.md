# Source proof and accounting

This note describes only the frozen fixed-byte LSD helper and its 83eb combined integration. It is not a proof of the entire pre-existing scheduler or a claim that measured memory satisfies an unstated numerical ceiling.

## Scope and stable ordering

The common sort_tile gate runs after existing empty/singleton, zero-variation and insertion-sized exits. It applies identically to strong serial and combined parallel leaf sorting. `indirect-tiles` is opt-in and admits records at least 64 bytes. `indirect-tiles-32` is a separate explicit control, admitting records at least 32 bytes. Feature-off retains literal sorting. Neither threshold depends on distribution.

The helper checks a real contiguous tile's payload size against 65,536 bytes and length against u16::MAX with checked size arithmetic. Maximum eligibility is therefore 1,024 records by default and 2,048 with explicit32. No whole-input record/key/index sidecar is allocated.

Each leaf captures the existing normalized unsigned Extract value once into u128. Prior parent scans may call the callback again, so this is not once per whole sort. Existing signed, float and Reverse encoders define order; no raw float comparison is introduced. Index order starts at 0..k. Stable byte-LSD passes reorder only u16 indices by cached keys; equal snapshot bytes are skipped. By induction each pass preserves lower-digit order and original order of equal full keys. MAX and all u128 bit patterns remain ordinary valid keys.

Variation is recomputed from the snapshot, rather than discarding bits with the parent's remaining mask. For consistent keys the parent-fixed bits are constant within the leaf, and the existing narrowed KeyHalf/Narrow encoders are sufficient. Including constant bits cannot change order. Changing callbacks have unspecified overall order; an entered leaf materializes its captured snapshot subject to earlier partitioning, not a new global snapshot guarantee. Inconsistent keys cannot invalidate the permutation or duplicate/drop an owner.

## Ownership and panic boundary

Before the first record transfer, all key/index capacity and one aligned record slot are ready, every user callback has returned, every index pass has finished, and an unconditional optimized-build validation has proved an in-range bijection. The validation uses the existing spare u16 vector. Test-only accounting also finishes before movement. Any allocation, callback, safe-indexing or validation panic at this stage leaves this helper's entry slice fully initialized and unchanged.

The destination-to-source map decomposes into disjoint cycles. Fixed points are skipped. For a nontrivial cycle, save its first record into one Workspace.tile slot, logically opening one hole; fill that hole from the next source and advance the hole; finally fill it from the saved temporary. The bijection guarantees a return to the same start and prevents intersection with another unfinished cycle. Marking map entries identity records visited cycles without another allocation.

All scratch pointers and lengths are obtained before a hole opens. Movement uses raw pointers and ptr::copy_nonoverlapping between disjoint T-sized slots. No whole-slice T reborrow or reference to an uninitialized T is formed in the loop. The temporary is aligned Vec<MaybeUninit<T>> storage, not a typed T stack variable or a destructor-bearing duplicate owner. No Clone, Copy or Sync bound is added; owned String/Vec, Cell and exclusive-reference records are supported.

The hole interval has no callbacks, allocation, user code, bounds checks, assertions, formatted diagnostics, typed T local or destructor. Pointer operations meet prevalidated contracts, and bounded/wrapping index increments cannot overflow. There is deliberately no claim of restoration after an arbitrary newly inserted panic inside this loop: adding a fallible operation there would invalidate the proof. Existing test hooks run before movement or after complete materialization. The completed-move witness additionally requires a nonzero cycle-copy count, so identity permutations do not claim movement.

A key panic in a later tile leaves previous tiles fully materialized. The pre-existing scheduler may allow disjoint other tasks to finish before propagating a panic; the cycle itself invokes no scheduler. Process termination, allocator abort, hardware failure or invalid unsafe callers are outside unwind recovery. Existing broader block/no_std panic semantics are unchanged.

## Full retained memory accounting

New retained storage is u128 keys plus two u16 vectors. At k=1,024 this is 20,480 requested heap payload bytes; explicit32's k=2,048 is 40,960 bytes. Byte-LSD cursors add 512 stack bytes. The validator and visitation metadata reuse the spare vector. Actual vector capacities, including allocator rounding, are what Workspace::retained_bytes reports.

The one aligned temporary reuses the first Workspace.tile slot, allocating a slot only if needed. It does not free or replace any already retained larger tile allocation. All existing tile, probe and block capacities remain counted in addition to the new metadata; shrink_to_fit releases the reconstructed workspace's allocations. Headers, allocator overhead and thread stacks are separate from these payload figures.

This adds a bounded tile constant per workspace to the inherited scratch structure, not an O(n) index buffer. It does not prove the whole combined scheduler's asymptotic space contract or a numerical peak ceiling merely from this local bound. SCREEN.md reports actual additional requested heap peaks independently, including the combined API's other scratch.

## Actual logical movement

A nontrivial length-L cycle copies L+1 full records: one save, L-1 hole fills and one restore. Fixed points copy none. For M moved records in C nontrivial cycles the total is M+C, at most 1.5M. One permutation phase is therefore not one transfer per record.

On the exact random-wide diagnostic shapes, the literal planner chose one MSD scatter and one copyback per leaf, not eight full-record passes. Its counted leaf copies were approximately 2N. The candidate roughly halved those logical copies but added key capture, index passes, validation, allocations and less sequential cycle access. No performance or physical DRAM traffic result follows from this count. The generic insertion_swaps metric also counts encoded-key probe swaps and cannot be multiplied by T size or called pure payload movement. All route/move instrumentation is cfg(test), absent from the timing/release library hot path.
