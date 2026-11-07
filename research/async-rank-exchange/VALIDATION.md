<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Native and focused Miri gates

The validated root source freeze is `5e4f07c90a93463ad9c4f16f4e50755d675e5fc1ca210e9a56d8309c74dd9d02`. The earlier source-only manifest is superseded. Neither frozen producer/child sources nor linked module targets were changed during execution.

## Native results

Direct-rustc builds used the existing ordinary child-mask leaf and cached Rayon. Rustc was 1.99.1 (58cf8f9c3 2026-10-13), x86_64-unknown-linux-gnu. Both builds denied warnings and used edition 2024, debuginfo=0, codegen-units=16 and stripped symbols.

The checked test binary used opt-level=1, debug-assertions=on and overflow-checks=on. All twelve async tests and then thirteen preserved regressions passed. A separate release-flags test binary used opt-level=3, debug-assertions=off and overflow-checks=off; all twelve async tests passed again. Actual Rayon and dedicated scoped-thread workers were exercised natively. These are correctness runs, not speed measurements.

The release rlib remains `native-output/libasync_rank_exchange.rlib`, SHA256 `e7fa6af14c4be6b9c09e48b14ca1c705420c013c4c9b1d224fb2c939db6d6dc3`. Release test binary SHA256 is `dd7008d704f1b84f1d850074272d996e57bcd080018bc7cc348602e42f91ae8b`. Full compiler commands and results are in `native-evidence/result.json` and `miri-evidence/result.json`.

## Focused Miri results

All ten exact filters listed in MIRI-PLAN.md each reported one passing test. Miri version was 0.1.0 (c980177d81 2026-11-06). Flags were exactly `-Zmiri-strict-provenance -Zmiri-symbolic-alignment-check`, with default Stacked Borrows and default leak checking. No alias-model switch or ignored-leak flag was used.

Coverage includes fixed-ledger restoration, a stale source attempt while the retired child's exact mutable borrow is live, tiny/empty/shared-boundary/tail buckets, scoped concurrent workers, child and worker panic recovery, finish-guard recovery, ordinary secondary destructor cleanup, and small u64/Row16 stable public-child oracles. The same mover, retirement and recovery source ran through std scoped threads under cfg(miri), with the unchanged frozen leaf compiled in std/scalar mode. This does NOT establish actual-Rayon's Crossbeam or SIMD execution as Miri-clean. Row128 async behavior was not validated by these gates.

The intentionally malicious secondary destructor case passed both native configurations but is excluded from Miri: a newly generated panic payload is deliberately forgotten to prevent recursive destructor panics. Ordinary secondary payloads were tested as reclaimed after full restoration, including under default Miri leak checks.

## Resources and evidence

Each execution window held the existing shared FD9 lock at device 27/inode 1578019 and reported actual acquisition/release. No performance timing was authorized or run. The cumulative sampled artifact peak across native, release and Miri outputs/evidence was 13,791,232 bytes. Minimum sampled free space was 194,732,032 bytes, above the 154 MiB stop and 150 MiB reserve. The 24 MiB conservative stop/30 MiB budget and per-command/whole-stage time bounds were respected. These disk guards are sampled controls, not filesystem quotas.

The source, earlier native outputs and existing Miri sysroot fingerprints matched after all tests. `miri-evidence/result.json` SHA256 is `c227f8b547ff3fc2e4953d6261eabb5676432cb19811ce8d9dfd6de437002853`; `miri-evidence/input-freeze.json` SHA256 is `35bfd0e0a9f0c8afe7395a5e0df616b94fe8595192fe03ed5c7ff85a4b4f296e`.

Remaining gates: full-call allocator peak against the frozen serial reference/aggregate memory cap, then separately authorized controlled performance screening. Correctness and focused Miri success imply neither speed improvement nor memory-cap acceptance.

## Space and panic limits

The mover retains the original producer directory, boundary map, shard cursors/free pools and count/start arrays, plus F atomic positions, M inverse entries and compact locks, O(PR) retirement counters, P block temporaries and all P child Workspaces. The Stats byte ledger counts simultaneous retained capacities, not allocator transients or peak RSS. No full-n record buffer was introduced; balanced fixed-P/R geometry has the stated square-root storage scaling, but the concrete aggregate cap remains unmeasured.

Root completion recovery applies after the completed directory is validated and seeded. The inherited sealed-Copy scatter panic contract remains initialized-but-not-necessarily-permutation; this work does not claim generic owned records, arbitrary/changing key callbacks, or process-abort recovery. After seeded restoration begins, caught child/worker/dispatcher failures complete pending root targets before propagation, without revisiting published child payload. Ordinary secondary panic payloads are dropped after join/restoration; only a new payload produced by a secondary destructor panic is intentionally leaked.
