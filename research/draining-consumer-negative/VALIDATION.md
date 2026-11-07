<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Validation scope and limits

## Observed passes before archival normalization

Production library check and strict Clippy with Rayon and `-D warnings` passed. No-default-features check passed. Rust 1.85.0 check passed with the inherited `unreachable_pub` warning for `key.rs::Unsigned`.

The initial private-wrapper debug run passed six new scheduler tests. A subsequent independently bounded attempt of every compiled native unit entry passed 69 tests; the pre-existing 10M root geometry diagnostic was explicitly ignored. All nine legacy progress/rendezvous tests passed unchanged. No timeout was hidden or relabeled as an expected mismatch.

Final exact-source optimized validation passed 32 unique tests: all eight new scheduler tests, ten inherited root/scatter/compact controls, twelve inherited ownership/safety-gap tests, and two delayed-bucket helpers. Tests cover P2/P3/P4 empty/refill and no-stranded-work behavior, fixed resource slots across real nested public sorts and Rayon joins, stable duplicate-key owned String+Cell and exclusive mutable Cell records, actual native moved-key child panic, simultaneous producer/consumer custom-payload Drop ordering, subthreshold no-launch/no-child-clone behavior, Clone reentry, and first-consumer Clone failure. P3/P4 also fail the second consumer Clone after one successful clone, require zero launched consumers and exactly one retained child-key Drop only after root completion/join. Root/shard key clones are distinguished from these consumer clones.

Default Stacked Borrows Miri with strict provenance passed seven tests. Two use scoped standard threads around the actual new drain/state kernel: exact exclusive Cell bucket conversion with empty-return/reuse, and retained owned-record panic payload. Five inherited serial/raw helpers cover late bucket consumption, raw adoption, and delayed callback recovery. Actual Rayon execution is not claimed Miri-clean. The scoped Miri panic occurs before record movement; native moved-key recovery and inherited raw-boundary Miri checks are separate evidence.

The new metadata mutex has a test witness whose lifetime includes actual mutex ownership; new key/Clone/payload tests assert user-code exclusion. The nested fixture bounds fixed slots across real nested work but does not prove a specific same-call stealing interleaving occurred. Source review found no remaining lifecycle defect in the tested candidate; that is not a general proof against arbitrary user behavior.

## What was not validated

Private-wrapper integration tests and doctests were not run because their retained `tessera_sort` imports differed from private package identity `generic-combined`. This archive restores the public package identity without changing Rust bytes, but no new integration suite, build, Clippy, Miri, or performance run was performed after that packaging normalization. The archival replay validates only application and exact source hashes.

Undeclared public Job/Pool and sparse-root test modules remain outside the inherited compilation graph. Strict Clippy covered the production library, not all test targets. No workspace allocator multiplier or heap-scaling measurement was performed. User-owned key state and explicit recursive sorts are not bounded by the scheduler's scratch count. There is no production-readiness, actual-Rayon-Miri, full-workspace-test, or end-to-end performance-gain claim.

## Failure and retention disclosure

One initial Clippy command failed before compilation because the harness put Cargo options after rustc's argument separator. The corrected production command passed. No compiler/test failure or process timeout was observed in the reported safety gates.

The worker incorrectly removed its own already-tested initial release unit executable while its replacement compiled, to satisfy an initial artifact-size cap. Its SHA256 was `d833df1d61874e52daa1988a044790cd3397c0c6d7d4a317d151d80c681844bb`, size 10,655,560 bytes. The executable was referenced by the initial eight-test success log. Its hash/stat and log remain in external evidence, but its bytes were not retained and were not reconstructed. It was not an earlier/shared target or a final frozen artifact. This scope mistake was disclosed to the coordinator; no further artifact deletion occurred.

The final replacement executable was independently rerun through the 32 optimized guards and retained separately from its Cargo output location. Its SHA256 is `03c3231621fce2afc7f3305de72fe560e018689066bc68b142cec2cf3fda16f2`, size 10,657,280 bytes. This packet contains no executable. It does not claim that every historically executed binary remains preserved.

## Packaging checks

`verify-result.json` records the independent public-commit export, sequential patch checks/application, exact 18/25/26-file Rust maps, and unchanged public manifests. `privacy-check.json` records the packet's text-only inspection. These checks use no builds, test suite, or timing workload. Original source/evidence stays outside the packet and is not changed.
