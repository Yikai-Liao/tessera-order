# 6. Ownership and panics

**The core idea:** Tessera moves arbitrary Rust types (`String`, `Box<T>`, `&mut T`, types with destructors) using raw bitwise copies — never requiring `Copy`, `Clone`, or `Default`. Memory safety rests on one strict invariant: **at every instant, every input record has exactly one live owner**, and every panic guard knows how to put all live owners back into the input slice without calling user code again.

## One live owner per record

Records move by bitwise copy between the input slice and `MaybeUninit<T>` scratch. The moment a record is copied to a new slot, the old bytes become a **stale copy**: they may look like valid bytes, but Tessera never reads, borrows, drops, or returns them (the faded grey cells in [4. The block partition](block-partition.md#step-by-step)).

Each stage tracks the exact set of live copies with a single source of truth:

- **Block partition:** the bucket chains (during scatter) and the completion ledger (during write-back) name every live block prefix ([invariants](block-partition.md#invariants)).
- **Tile pass:** `RestoreTile` tracks whether the complete live set currently sits in the tile or in scratch.
- **Insertion pass:** `Hole` holds the single lifted record and records the sole vacated slot while predecessors shift right ([engine reference](../design/engine.md#panic-safety)).

The user's key closure is only ever given a shared borrow to a live record.

## Inconsistent key functions

A textbook counting pass reads every key twice: once to build the histogram (deciding bucket sizes) and once to scatter records into those buckets. If a buggy or non-deterministic key closure returns a different digit on the second call, two records can collide on the same output slot while another slot stays uninitialized — instant undefined behavior for owned types.

Tessera closes that hole completely:

- **User key functions (`sort_by_key`):** during the histogram pass, Tessera caches each tile record's `u16` bucket ID in scratch and reads that cached ID during scatter instead of calling the key closure again. Bucket ranges are disjoint and match the cached counts identically, so every output slot is written **exactly once** no matter what the closure returns. (The block partition calls the key function only once per record per pass, so it needs no cache.)
- **Direct keys (`sort`):** Tessera's own sealed encoders are deterministic and panic-free, so scatter recomputes digits directly with zero bucket-ID storage ([engine reference](../design/engine.md#record-handling)).

An inconsistent key closure may therefore produce an unsorted slice or panic, just like Rust's standard `slice::sort_by_key`, but can never leak, duplicate, or corrupt a record.

## Panicking key functions

If a key closure unwinds, Tessera restores every original record into the input slice (in unspecified order) before the panic propagates:

| Stage where panic occurs | How ownership is restored (zero key calls during unwind) |
|---|---|
| **Block scatter** | The partition guard inspects bucket cursors and consumed input blocks, appends the unread suffix directly to bucket 0 without calling the classifier, and runs the [completion ledger](block-partition.md#completion) to move every record back into the input slice. |
| **Tile counting pass** | `RestoreTile` copies the last complete record set back from scratch into the tile; the interrupted pass wrote only stale bytes into the opposite buffer. |
| **Insertion pass** | `Hole`'s destructor writes the lifted moving record back into the sole vacated slot. |
| **Parallel completion callback** | Rayon jobs join first; the parent completion guard finishes every unpublished band without running callbacks and never touches bands already lent to callbacks. |

Every build requires `std`, so unwinding recovery is always active; there is no abort-only mode.

## Where this is tested

The test suite in [`rust/tessera-order/tests`](../../rust/tessera-order/tests) exercises drop-tracked owned records, `&mut` references, zero-sized and over-aligned types, non-deterministic key functions, and panics injected at every stage of serial and parallel sorts, verified continuously under Miri and AddressSanitizer in CI.

Next: [7. Beyond sorting](operations.md) shows how `argsort`, `select_nth`, and `sort_by_cached_key` reuse this engine and skip work their answer does not need.
