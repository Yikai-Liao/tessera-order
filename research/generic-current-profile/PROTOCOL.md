# Current generic root profile, four fixed cells

## Source and scope

Public source is immutable commit55b299b3716cd1af9e997f30b1a3c1813455d8be. Parent verified later tip0ff67d3 has identical Rust/Cargo bytes. The native combined origin is count-e1 baseline manifestab73401745af53e114015e4ba6cc6a87ef91a5cfe3a1daa73036049189050e2a. Initial exact origins remain under origins, with independent manifests. Runtime source uses the public current serial engine, key, scan, SIMD, workspace and unwind code; native combined adds segmented scratch ownership and mechanically renamed old native scatter/permutation/fixed-consumer code. Current public narrow Direct parallel-count gate and Narrow forwarding are aligned in combined. Full-random u64/ByKey Row16 diagnostics must produce a generic root plus all256 children, proving that gate did not replace the call. No algorithm or scheduling candidate is being screened.

Public uses serial block::partition_digit_with_ends and materialized Job/Pool tasks. Combined uses native parallel root scatter, parallel block permutation and serial squeeze overlapped with fixed std::thread::yield_now consumers. These are not the same implementation. Independent source-placement clones differ only in package manifests. Diagnostics are additional distinct source copies, never the acceptance controls. Timing source has no metadata dump. The adopted-directory captures use their own clone and are non-timing correctness diagnostics.

## Frozen run

Only Direct u64 and repr(C) Row16{key:u64,index:u64}, n10,000,000, SplitMix64seed1, W4/W8. Key fingerprint must be c2333e730de5dd2c. Reference is a complete stable sort, including original Row16 index, not just key-order checks. Inputs/clones/reference comparison and pool initialization are outside the timed full public API call. Pool broadcast initializes all workers. One full randomized warmup block is excluded. No environment tuning variables may be present.

Control mode: nine measured blocks plus one warmup, twelve labels per block. P1/P2 are same-code public A/A; PL is independent public source placement. C1/C2 and CL are the equivalent combined controls. SP1/SP2/SPL call the current public serial API; SC1/SC2/SCL call the native package's same-current-core serial API, retaining the segmented-storage layout's code-placement effects. These are distinct strong-serial policy references, not fixed-kernel W1 scaling. Total120sorts per cell,480 across four cells,432 measured.

Trace mode: three measured blocks plus one warmup, ten labels per block. P1/P2/PL/C1/C2/CL retained, plus Pt/Poff and Ct/Coff for tracer-on/off from each instrumented copy. Total40sorts per cell,160 across four cells,120 measured. No new acceptance conclusion from the trace timings; tracer and placement effects are reported independently per block without median subtraction.

Total screen640full sorts,552measured. A fixed SplitMix shuffle seed9123 randomizes each complete block. Four cells run u64W4,Row16W4,u64W8,Row16W8 in each phase. No adaptive tuning or dropping slow samples. Estimated execution under90seconds on prior per-call scales, plus output verification; timeout is not a license to drop cells.

## Trace meanings

Common-origin Instant and Linux CLOCK_THREAD_CPUTIME_ID are captured before the event mutex. Root preparation and variation scan, setup, classify/scatter, native adoption, permutation planning/jobs, materialization, first/last child, worker child CPU and overlap are recorded. Timings are per-run intervals, never reconstructed by adding medians or summed worker time. Empty-wait CPU includes try_recv, queue mutex acquisition, polling, yield and instrumentation; it is not pure yield or mutex CPU. bucket_publish precedes enqueue, so its delay is materialized-to-start. Public task_enqueue is a separate event. No PMU or physical bandwidth claims. Copy lengths are logical copies including self copies and padding, not DRAM traffic.

Record-loop atomics/clocks are absent. Only coarse phase/job events and scalar counters per block-copy call exist in tracer copies. Tracing perturbation is measured using trace-on/off and independent exact controls. Trace addresses are normalized to record offsets before output, never exported as pointers.

## Resource and acceptance gates

Existing lock device27 inode1578019 is never replaced. Entire shell FD9 is shared for builds/tests/diagnostics, exclusive for timing. ACQUIRED/RELEASE messages are retained. Buildoffline/-j1/INC0 uses the existing scaling target and distinct package/bin identities. Prior targets and binaries are retained. Final source/driver/parser/binary hashes are frozen before timing; no timing until parent START. Final manifest and every input/cell are independently checked by analyze.py. Full tests are not claimed: exact focused native safety and Miri filters are reported, with public Job/Pool and sparse-root coverage exclusions documented in VALIDATION-SCOPE.md. Strict Miri executes native movement/adoption without actual Rayon scheduling.
