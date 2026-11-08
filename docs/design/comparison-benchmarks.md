# Comparison benchmark design

The comparison is a local measurement pipeline: adapters produce verified block samples, a separate process measures allocations, and an offline TypeScript report computes every statistic and figure. Commands, inputs and measurement contracts are in [Benchmarking](../development/benchmarking.md). Release sessions run on one maintainer-controlled machine: hosted job limits must not determine groups or absolute-time comparisons.

## Boundaries

| Module | Owns |
|---|---|
| `case.rs` | Key generation shared with the continuous suite |
| `suite.rs` | Exact case matrix, case identity and resumable groups |
| `variant.rs` | Supported tasks, record representations and sort adapters |
| `check.rs` | Ordering, exact record preservation and stable tie order |
| `run.rs` | Clock boundaries, calibration, block order and sample fields |
| `host.rs` | Read-only host fingerprints and changing observations |
| `comparison/mod.rs` | CLI, provenance checks and result persistence |
| `native/` | Foreign signatures, layouts, pinned sources and build flags |
| `bench/report/` | Raw-data validation, paired statistics, figures and BMF |

`Variant::prepare` returns a kernel or `None`; capability listings and the manifest case plan use that operation. There is no second support table in the runner. A prepared input owns the original records and stable reference once per case; kernels share them immutably and own their restorable batches. Sorting is monomorphized for its record type; dynamic dispatch occurs once per batch. Only one variant's prepared batch is resident at a time.

The checker uses exact equality for scalar bits and stable outputs. Unstable records are checked against the original record indexed by their payload, with a seen-index set. This permits reordering ties while detecting lost, duplicated or damaged records.

## Persistence and measurement

Groups are one type and distribution across lengths. A group is committed by its final observation. Resume keeps committed groups and rewrites the journal without incomplete groups or an unfinished final line, then measures missing groups. The host fingerprint and build manifest must match before any append. Separate process invocations write the same schema; report merging additionally rejects overlapping samples.

Memory runs before timing because its measured Tessera peak sets the `glidesort-sqrt` buffer. Its manifest is separate: both executables share source and configuration while their binary hashes differ. The timing executable checks that provenance before using the memory results.

The report owns derived statistics. Both SVGs and Bencher Metric Format are computed from the same summaries, so the uploaded result and published figure have one definition of ratios, bounds and A/A noise. Raw samples remain sufficient to recompute them.

## Native code

The fetch command verifies pinned archives and records extracted-tree hashes. The build verifies those trees and performs no downloads. Native source trees and compiler outputs are ignored; repository-owned shims include upstream sources rather than copying them. Each pin records the applicable license files.

C and C++ see complete native buffers with C layouts. Comparisons execute on that side of the boundary. Clausecker's original 32-bit pairs have a supplementary task; SIMD key-value interfaces with different layouts remain unsupported. Float interfaces participate only when they preserve `total_cmp` order and all payload bits.

The memory executable wraps C allocation symbols and provides C++ new/delete definitions. This closes the shared-library allocation boundary without adding allocator instrumentation to the timed executable. Native peaks include allocator usable-size rounding; Rust-only peaks use requested sizes. Stack space is reported as unmeasured.

## Verification

Generator hashes retain the continuous suite's input identity. Harness tests cover input restoration after destructive sorts, stable and unstable payload checking, floating-point edge cases, partial journal recovery and allocation accounting. Report tests cover paired timing, the 8-of-9 decision, A/A noise, the SIMD anchor, raw-data round trips and invalid or incompatible directories. Synthetic SVG fixtures include losses, close competitors and long names.
