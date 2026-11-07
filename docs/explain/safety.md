# 6. Ownership and panics

Tessera sorts records of any type: `String`, `Box<T>`, `&mut T`, structs with destructors. It never requires `Copy`, `Clone` or `Default`. A sort that moves records with raw copies has to make sure that no record is dropped twice, leaked, or read while half-moved. That has to hold even when the user's key function returns inconsistent keys or panics. This page explains the rules that make this work.

## One live owner per record

Records move by bitwise copies between the input slice and scratch memory of type `MaybeUninit<T>`. After a copy, the old bytes are a **stale copy**: they look like a record but are never dropped, borrowed or returned. In the [block partition figure](block-partition.md#step-by-step), stale copies are the grey cells.

At every point, each record has exactly one **live** copy, and the engine always knows which copy it is:

- in the block partition, the block table plus the partial-block lengths name every live record ([invariants](block-partition.md#invariants));
- in a tile, the `RestoreTile` guard remembers whether the complete set is in the tile or in scratch.

The key function only ever borrows a live record.

## Inconsistent key functions

A counting pass reads each key twice: once for the histogram and once for the scatter. If the key function returns a different digit the second time, the histogram's bucket sizes no longer match the records sent to each bucket. Two records could then be written to the same slot, and one slot would never be written.

`stable_sort_by_key` therefore **caches bucket IDs**: during the histogram it stores each record's bucket as a `u16`, and the scatter reads that value instead of calling the key function again. The bucket ranges are disjoint and their sizes match the stored IDs, so every output slot is written exactly once, whatever the key function does. The block partition calls the key function once per record and pass, so it has nothing to cache.

`stable_sort` sorts the keys themselves with Tessera's own encoders, which are deterministic and cannot panic, so it recomputes keys and stores nothing ([decision 0001b](../decisions/0001b-cache-bucket-ids-for-every-record.md)).

An inconsistent key function can therefore produce an unsorted result, but never a lost, duplicated or uninitialized record.

## Panicking key functions

With the default `std` feature, a panic in the key function is caught, every record is moved back into the input slice, and the panic is resumed:

- **During a block scatter**, the engine works out how far the pass got from the bucket cursors and the number of input blocks consumed. It then finishes the pass **without calling the key function again**, sending every unread record to bucket 0. The result is a valid, unsorted pass. Compaction then copies every record back into the input.
- **During a tile pass**, `RestoreTile` copies the last complete record set back into the tile if it was in scratch. The interrupted pass only wrote stale copies into the other buffer.
- **During a compaction callback** (a tile finished while cache-hot), compaction continues without further callbacks. The panic is resumed once the input holds every record again.

Afterwards the slice holds every record exactly once, in an unspecified order. The [workspace](../guide/workspace.md) stays usable.

Without `std`, the crate cannot catch a panic. If the key function panics while records are spread over blocks, unwinding would run destructors over an incomplete block table, which could free a record twice. An abort-on-unwind guard turns such a panic into an abort instead. Panics elsewhere unwind normally with every record in place ([decision 0005b](../decisions/0005b-no-std-unwind-guard.md)).

## Where this is tested

The tests in [`rust/tessera-sort/tests`](../../rust/tessera-sort/tests) and the engine's unit tests cover owned records with drop tracking, `&mut` records under panicking and changing key functions, workspace reuse after a panic, over-aligned and zero-sized records, and key-function panics at many points of a sort. Miri and AddressSanitizer run them in CI.
