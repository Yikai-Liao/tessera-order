# 0002: Cache bucket IDs for every record type

Date: 2026-11-06. Status: accepted.

## Context

A counting pass reads each key twice: once for the histogram and once for the scatter. A user key function may return a different key the second time. The research prototype handled this in two ways. Non-`Copy` records stored a two-byte bucket ID per record during the histogram, so the scatter never called the key function again. `Copy` records recomputed the key and filled scratch with valid records on the first pass. Choosing between the two needed Nightly specialization.

## Decision

The crate has one `stable_sort_by_key` for every record type, and it always stores bucket IDs. `stable_sort`, whose keys are encoded by the library and are deterministic, recomputes keys and stores nothing. A private const generic chooses between the two inside the engine, with no per-record runtime check. There are no `_copy`, `_owned` or `_unchecked` variants.

## Consequences

- The crate builds on Stable Rust, and a key function that returns inconsistent keys can reorder records but never duplicate, drop or expose them, for every record type.
- Plain-data records pay for the stored IDs. Against the prototype, the cost is a few percent in most cases, and up to 17% on 128-byte records at n = 128 ([measurements](../design/port.md#performance)).
- `{u32 key, u32 row}` records are not measured yet. If their cost is large, an `unsafe` entry point that skips the cache can be reconsidered ([roadmap](../../ROADMAP.md)).
