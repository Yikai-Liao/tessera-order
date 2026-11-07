# Validation scope and preserved failures

Source identity and proof are separate from performance. This packet preserves the rejected LSD code, not a claim that safety validation establishes speed or complete scheduler verification.

## Old-base scope

The initial 9ed49f0/main72c prototype had 46 passing all-target tests under Rust 1.85 with rayon + explicit32: 24 unit, 5 conformance, 12 public parallel, 3 recovery and 2 allocator tests. Focused no-default-features/default64 tests passed 16; strict Clippy passed. Four focused Miri checks covered small exhaustive cycles, eligible exclusive references plus unwind, every owned key-panic position/reuse, and exact32B owned String.

**Those old-base cargo tests used the default debug profile. No old-base release correctness suite was run.** Portable SIMD and a 32-bit target were not executed there. Explicit layout fixes were source-reviewed for portability, not proven by a 32-bit run. Earlier feature-off library tests passed 5 before the final test additions; this is not mislabeled a final full feature-off suite.

## Frozen combined integration

All heavy commands used offline -j1 with shared FD9 on the existing cpu-window.lock, device 27/inode 1578019. Optimized tests explicitly used --release and one test thread.

- Default64, rayon + indirect-tiles, release all targets: **88 passed** = 65 unit + 5 conformance + 1 main-kernel integration + 12 public parallel + 3 recovery + 2 heap.
- Explicit32, rayon + indirect-tiles-32, final release all targets: **89 passed** = 66 unit + 5 + 1 + 12 + 3 + 2.
- Strict all-feature/all-target Clippy with -D warnings: passed.
- Rust 1.85 rayon + explicit32 check: passed, retaining the inherited Unsigned unreachable_pub warning.
- no_std + default64 check: passed.
- rayon + portable-simd + explicit32 composition check: passed. Optional upstream SIMD requires Rust 1.89 separately from the default/Rayon MSRV.
- Strict-provenance Miri (-Zmiri-strict-provenance): **5 focused tests passed**, covering exhaustive small cycles, eligible exclusive references/key unwind, every owned capture-panic position/reuse, and deferred owned/exclusive bucket consumption.

Default64 and Miri preceded the final explicit32 public-fixture-only adjustment. No production/helper/unit-test bytes changed afterward. Final explicit32 all-targets and all-feature Clippy cover the final manifest. Child subprocess summaries are not double-counted. This is not a full Miri suite or validation of the real multithreaded Rayon scheduler. Exact-shape SIMD release diagnostics are listed separately.

The final explicit32 rerun printed/cleared all TESSERA_* experimental variables and recorded affinity 0-8. Earlier failing commands did not contemporaneously print/clear them; a later observation in the same shell showed none, but is not retroactive runtime evidence. The subsequent controlled geometry run also explicitly cleared them. Safety and diagnostic lock shells released FD9 and exited. Archival staging ran no further builds/tests.

## Failures retained, not erased

1. The first old-base explicit32 suite failed the moved-address panic probe because eligible indirect leaves no longer call keys on already moved scratch records. The failing log and corrected success-control log are retained externally.
2. Initial combined compilation found a copied child-only worker_started field assertion incompatible with the native combined probe's moved_worker_started field. The native public guard file was restored before testing actual behavior.
3. The initial combined explicit32 late-callback guard timed out after 20 seconds because the new completed cycle had no test movement witness. A truthful cfg(test) witness was added after complete materialization, only for nonzero record copies, never inside a hole.
4. The fully restored native moved-address public guard then failed in optimized explicit32 mode on the Owned tile-child fixture. A separate full run passed all 66 unit tests and other preceding targets but failed the same public test. These failures are not called corruption, swallowed panic, or proof that root movement must invoke the trigger.

The evidence-backed replacement is narrow: only explicit32, actual Owned size >=32 bytes, tile-only children. It requires successful stable (key,id) sorting, actual non-origin worker participation, zero moved-address callbacks, exact payload preservation and exactly one final drop. Six controlled P2/P4 and thresholds 1/7/17 cases printed actual T=48 bytes, n=131072 and zero moved calls. Block-child, Borrowed, default64 and feature-off panic expectations remain unchanged.

DIAGNOSTICS.md records actual controlled root R256/B32, all-input full sources and zero tails; those fields explain why this particular root pass did not call the key on scratch. They were captured in a later run, not the original failure. The 16-byte Borrowed fixture and combined 24-byte exclusive tuple remain literal even under explicit32; their guards are not relabeled indirect-exclusive pipeline coverage.

## Genuine public consumer capture recovery

The narrow success control is not a substitute for recovery. A separate exact32B public owned test panics only while key capture is active on a worker that already completed an indirect tile, then checks the payload, permutation and drops. Eligible padded exclusive records have separate focused Miri coverage.

Combined pipeline tests use a Consumer-scoped INDIRECT_COMPLETED flag retained across new_bucket and reset/restored on PhaseGuard transitions. The existing per-bucket MOVED reset is unchanged. The injected fault requires Consumer phase, active capture and a prior nonidentity indirect tile completed by that same consumer. It cannot be a root-scatter fault inferred from a callback count.

The producer still waits at its second post-publication callback with pending squeeze/publication work; no key is called while the queue lock is held. Assertions require publication < first consumer key < completed move < consumer snapshot panic < sender close, joined consumers, preserved identities/payloads and exact drops. Simultaneous producer/consumer panic also passed. This new pipeline fault coverage specifically uses OwnedCell56B under explicit32. It does not imply padded indirect-exclusive consumer recovery was tested end to end.

Independent read-only review found no production ownership/order/integration blocker for the frozen helper and scoped witness changes. The final evidence-backed public fixture correction is preserved with its failure and controlled replacement; final all-target validation is recorded above. Hash lists identify the exact preserved code; review does not replace the dynamic scope above.

## Evidence location

`external-evidence.json` records hashes of the original reports, manifests, commands and selected passing/failing logs outside this packet. Logs are preserved in the original prototype's validation directory, the combined integration's integration-evidence directory, and the separate diagnostic clone. The archival patch does not carry raw logs, harnesses, targets or binaries.
