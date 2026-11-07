# Existing native entry validation checkpoint

This source-only packet adds test adapters to the existing combined native scheduler. It contains no new sorting algorithm, no failed draining-queue changes, no R16 geometry override, no binaries, raw logs, benchmark harness or performance claim. Runtime adoption remains pending the remaining gates in VALIDATION.md.

## Apply and verify

Use public research API commit `55b299b3716cd1af9e997f30b1a3c1813455d8be`, then apply only `research/draining-consumer-negative/patches/01-public-to-combined.patch` from published commit `5d06e2c` (full identity and dependency hashes in provenance.json). Do not apply that packet's draining patch02. Then apply this packet's `patches/validation-tests.patch`. Preserve public Cargo manifests/lock and the benchmark workspace member so package resolution works; explicitly select package tessera-sort when checking tests, rather than building workspace benches.

For a source-only replay in a temporary export, run `python3 verify.py REPOSITORY --output verification.json`. It exports the exact public commit, retrieves the dependency patch from its pinned published commit, verifies all 25 combined Rust hashes, creates a disposable Git index and checks/applies the test patch using actual three-way application. It compares the complete final source map and public metadata, and verifies production equivalence after removing only the new test module declarations. It does not build or execute tests, touch the supplied repository, or need credentials/network.

The patch contains three new test files and two cfg(test) module declarations. Adjacent Materialized borrow adapters run in scoped serial paths and under Miri; sparse tests assert actual root512/Q>=2 execution and post-move recovery; duplicate-width integration tests stable payload ordering through the public API. Existing public Job/Pool-specific modules remain unchanged and undeclared in native, not falsely counted as executed native tests.

source-map.json gives exact public metadata, combined base Rust map, final full source map, production-bearing base/final maps and new test files. Production-bearing source files may embed old tests; their hashes are reported as whole files rather than pretending test code is absent. Only engine/parallel.rs differs among those files, by the two cfg(test) declarations. binary-freeze.json records the identities of six retained local optimized executables without distributing them or their machine paths.

Main `a0794cf712e6010948713d13a88af76cfdfa71a5` was already merged by experimental commit `0ff67d3`. Main and 55b share the common serial algorithm; parallel API/count additions are research-branch additions, not an outstanding upstream merge. Native scratch_segments and forwarding changes make native serial a separate control from strongest public serial.
