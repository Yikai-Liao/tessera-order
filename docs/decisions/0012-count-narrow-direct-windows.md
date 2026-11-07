# 0012: Count narrow windows of direct keys instead of partitioning them

Date: 2026-11-07. Status: accepted.

## Context

A node whose remaining variation spans at most eight bits needs one pass to become sorted, and the engine gave it a block partition: bucket tables, a block order, and a compaction pass that moves every record by exchanging blocks. The table work per record is what makes that route expensive at a small fan-out. Most of it is avoidable when nothing has to be preserved but the order of the keys themselves.

The engine has two extractors with different guarantees. `Direct` sorts a record by its own encoded key, and every sealed encoding is a bijection on bit patterns, so equal keys are equal records. `ByKey` calls a user key function: two records with the same key may hold different payloads and an order between them that the sort must keep.

## Decision

`Extract::try_count_window` finishes a node by counting its keys inside one window and filling each run from one of its records. `Direct` implements it for windows of at most eight bits; `KeyHalf` moves the window into the half it narrows to. `ByKey` keeps the default, which declines and leaves the node to the block partition.

The fill keeps the auxiliary-memory bound: one `usize` count and one remembered record per bucket, so 256 counts and 256 records on the stack, whatever the node size. Nothing is allocated and the block buffers stay untouched.

## Consequences

- Nodes of a narrow-window distribution no longer build block tables or move records through them; they take one counting pass and one fill.
- Only `stable_sort` takes the route. `stable_sort_by_key` sorts a distribution of 16 or 256 values exactly as before, which keeps it as the reference path for the block partition in tests and benchmarks.
- The window is capped at eight bits because the fill needs one remembered record per bucket. A wider window would need a table in the workspace, which this route is meant to avoid.
- If the caller's window ever failed to cover every key difference, the fill would silently merge distinct keys. The engine derives the window from the varying bits of the node itself, and the tests cover windows at the bottom, in the middle and at both ends of 64- and 128-bit keys.
