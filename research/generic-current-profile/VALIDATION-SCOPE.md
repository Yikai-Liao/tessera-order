# Validation scope

The immutable origins are the initial draft port. Runtime sources add only the current public narrow Direct parallel-count gate and its Narrow forwarding to the combined scheduler, plus the old test-only counted helper module required by the native tests. This is current API alignment, not a candidate optimization. The diagnostic full-random u64 and ByKey Row16 roots must show an actual generic root scatter and 256 published children; the narrow count route cannot produce that trace.

Current public Job/Pool-specific bucket_borrow_tests are not declared by the native scheduler. They test a different batching representation and are not represented as passed. The native Materialized delayed owned/exclusive-borrow tests and native panic/restoration tests remain wired. Current public sparse_root_tests are also not declared in the native module; their coverage is a known validation gap pending a targeted adapter, not an implied full test pass. Current key/parallel_count tests remain wired and their assertions are unchanged. The old native counted helper assertions are unchanged.

Initial cargo check is a build/type check, not a runtime safety result. Focused tests and Miri results will be reported by exact filter; actual Rayon scheduling is never described as Miri tested.
