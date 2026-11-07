# Decision records

Each file records one decision that shapes the project: the context, the decision and its consequences. Records are not rewritten when a decision changes; a new record supersedes the old one, and the old one says so at the top.

| No. | Decision | Date |
|---|---|---|
| [0001](0001-single-bsd-2-clause-license.md) | One BSD-2-Clause license for the whole repository | 2026-11-06 |
| [0002](0002-cache-bucket-ids-for-every-record.md) | Cache bucket IDs for every record type | 2026-11-06 |
| [0003](0003-independent-package-versions.md) | Version and release each language package independently | 2026-11-06 |
| [0004](0004-continuous-benchmarks-on-bencher.md) | Run continuous benchmarks on Bencher Bare Metal | 2026-11-06 |
| [0005](0005-no-std-with-alloc.md) | Support no-std with alloc behind a default-on std feature | 2026-11-06 |
| [0006](0006-no-std-unwind-guard.md) | Guard record movement against unwinding without std | 2026-11-07 |
| [0007](0007-optional-simd-backend.md) | Add an optional fearless_simd backend for the inversion scan | 2026-11-07 |
| [0008](0008-insertion-finishing-entry-point.md) | Give insertion finishing a unit-returning entry point | 2026-11-07 |
| [0009](0009-benchmark-report-images-and-history.md) | Publish immutable benchmark images and compare PR history with main | 2026-11-07 |
| [0010](0010-vector-ascending-confirmation.md) | Reuse the vector scan for ascending-order confirmation | 2026-11-07 |
| [0011](0011-benchmark-feature-configurations.md) | Track default and Portable SIMD benchmarks independently | 2026-11-07 |
| [0012](0012-count-narrow-direct-windows.md) | Count narrow windows of direct keys instead of partitioning them | 2026-11-15 |
| [0013](0013-compare-with-the-previous-main-sample.md) | Alert on a fixed percentage from the previous main sample | 2026-11-07 |
| [0014](0014-optional-bounded-rayon.md) | Add optional parallel completion of materialized buckets | 2026-11-07 |
| [0015](0015-materialized-bucket-overlap.md) | Overlap retired bucket batches with serial root compaction | 2026-11-07 |
