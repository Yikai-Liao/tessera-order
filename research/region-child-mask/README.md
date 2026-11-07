<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Fixed-root child-mask research archive

This BSD-2-Clause source-only checkpoint records a real reduction in initial variation scanning without an accepted end-to-end masked performance win. It adds no production API or adopted optimization. It contains minimal replayable patches, the exact validated focused test source, diagnostic-source preparation, source hashes and sanitized summaries. It contains no core vendor tree, executable, raw sample, benchmark/count driver or machine-specific execution wrapper.

## Source chain

1. Start from public research API commit `55b299b3716cd1af9e997f30b1a3c1813455d8be`.
2. Apply only `research/draining-consumer-negative/patches/01-public-to-combined.patch` from published commit `5d06e2cc25704901b1a6838ef53416c0da39fe98`. Do not apply that packet's draining patch02.
3. Apply `research/native-entry-validation/patches/validation-tests.patch` from published commit `7428f82491d27607e7d94b641badf2fa63a7414e`. The published source map identifies the exact current leaf. Its original combined-profile identity is `bf3b2ea730f98fca2dfaf75f3baba732f7f5d67eaea8bafdbae17c684fe11bae`.
4. Apply this packet's `patches/01-child-entry.patch` to that leaf. Original public engine/library bytes remain unchanged prefixes. Unchanged leaf files remain exact current-core bytes.
5. Obtain the five root source files from `research/region-blocks/src` at published commit `36781cb78238a54bc2558b2cf1834b12b2a32a72`, then apply `patches/02-root-dispatch.patch`. The root's historical leaf dependency is not reused. Both helper modes and the original public child control use the current leaf from steps1–4.

`provenance.json` pins full public commit IDs, paths and SHA256 hashes. `source-map.json` pins the complete final replay. Upstream main `b973f09baa4fad9b6140fd54748a98ece25d6742` was already merged in the research branch; this packet does not represent a pending main merge or silently rebase its frozen experiment onto a newer source tree.

## Verify or export without building

Run `python3 verify.py REPOSITORY --output verification.json` using a repository that already contains the pinned public commits. Verification reads Git objects, applies exact unified hunks in memory, rejects offsets/fuzz, checks all25 combined Rust hashes, all37 native-validation source hashes and all34 final control-source hashes, and checks public-entry, scanner and root-transport equivalence. It does not modify the repository, create a source tree, build, execute tests, sort records or access the network. Run without Python -O.

Optional `--export NEW_DIRECTORY` writes the verified source-only layout to a new directory outside the supplied repository and this packet. The export contains `leaf/src`, `src`, `checks` and `make_diagnostic_source.py`, but no replacement Cargo manifests or lockfile. Dependency resolution and any build/run are separate deliberate steps. The original public metadata is verified before constructing this research layout.

The retained focused tests were compiled with the leaf as extern `tessera`, root as extern `control`, and the same Rayon dependency. Run focused tests with one test thread because diagnostic report collection is global. Diagnostic builds require a successfully completed, fresh overlay from `make_diagnostic_source.py DESTINATION`, run beside the exported `leaf/src`; use the generated overlay as the leaf source and enable child-scan-diagnostics. Merely enabling that feature on the original leaf would not instrument the untouched public control. Failed overlay creation must be retained and inspected rather than used for compilation.

## Exact control and scope

Public calls the original Workspace child entry. Full and Masked call the same duplicate preparation and the same duplicate public scanner. Only its saturation-mask value changes: full u64 width versus the high root digit excluded. Both use the public1,025-key prefix and uninterrupted remainder loop. A64-key-check scanner was an earlier proposal and is not this experiment.

Order probing, insertion probing, flat-order confirmation, Direct/CACHED=false for u64, ByKey/CACHED=true for rows, tile policy and downstream sorter are retained. Root histogram, stable ranks, scatter, path restoration, barrier, fixed child groups and workspace lifetime are preserved. Only typed dispatch and relocation/re-export of identical Row16/Row128 record definitions change. No new unsafe operation was introduced.

The root validates width1..=12 and shift=64-width. Each child's root digit is fixed, so all genuinely varying bits lie within the root-excluded candidate. Once the prefix has varied every candidate bit, the complete varying mask is known. Diagnostics check that premise with every input key, separately from non-instrumented timing.

This remains sealed, pure intrinsic-key Copy research for u64/Row16/Row128. It does not establish a generic owned-record API, key inconsistency recovery, callback panic safety or strict per-physical-thread memory bounds. See `RESULT.md` and `SPACE.md` for evidence and limits.
