# e1 count-control source follow-on

This small checkpoint extends the published e8 source checkpoint at experiment tip `93ce70b332405ca9e704ad025163716491000e07`. It contains only the next shared runtime bridge, test-only boundary coverage, Cargo-name normalization, and necessary replay/validation metadata. It asserts no speedup, runtime adoption or benchmark acceptance; measurement reports are separate.

## Replay both controls independently

Use separate isolated checkouts of public source base `0bc80b49d5efd5bd8b62165b170015e5803226b7`. Retrieve the prior patches from the fixed experiment tip, under `research/patches/parallel-count-window/source/patches/`. Their exact checksums are in `replay-proof.json`; no live working-tree file is the reference.

Original control:

1. Apply published `01-combined-current-main.patch` and `02-e8-default-insertion.patch`.
2. Apply local `patches/00-baseline-cargo-identity.patch`.
3. Apply local `patches/01-e8-to-e1-runtime.patch` and `patches/02-wide-tile-boundary-tests.patch`.

Count candidate:

1. Apply all three published patches, including `03-parallel-count-window.patch`.
2. Apply local `patches/01-e8-to-e1-runtime.patch` and `patches/02-wide-tile-boundary-tests.patch`. Do not apply the baseline-only Cargo patch.

Run `git apply --check` before each application. The inherited combined patch is a research workspace change, not a production adoption step. Source remains under the upstream BSD-2-Clause license; the existing repository and prior checkpoint retain its license notices.

Independent forward/hash/reverse/second-forward checks passed for both chains. Reversing each full chain reproduced the exported original base exactly. Every final Rust byte matches the corresponding frozen source. Only private package names in Cargo.lock and the crate Cargo.toml are normalized to `tessera-sort`; the lib name remains `tessera_sort`.

## Exact shared delta

The runtime patch is the sole Rust change across upstream `e8e590616f0d3cfb5e8a30b84e4b3e43042036a4` to `e1d25297685cecb205d18114e898a39878e3fd83`: engine.rs tile_digit_bits uses slack2 rather than3 for record sizes greater than8 bytes. Both controls absorb the same change. The exact upstream comment is retained for provenance, but this checkpoint does not endorse its statement that128-byte records are unchanged: the actual formula can change their cap too. At64 records, size128 now yields8 bits instead of9.

The tests add only a cfg(test) module and its two checks. No producer/pipeline/generic guard assertion, count kernel, scheduling, allocation layout, padding, threshold tuning or forced-worker control was changed. The count optimization's direct u32/u64 domain is <=8 bytes and does not enter the imported cap branch.

## Identities and feature boundary

Frozen original identity: `ab73401745af53e114015e4ba6cc6a87ef91a5cfe3a1daa73036049189050e2a`.

Frozen candidate identity: `8f300a35a875c8949d80fc306ed2ec24ddc9fbe6747663d4768369610b641b35`.

`replay-proof.json` supplies normalized identities, relative file hashes and ordered chains. Identity hashing uses SHA256 of UTF-8 canonical JSON for the file map, with sorted keys and separators `(',', ':')`.

Production uses `rayon,portable-simd` with `count-window-research` OFF. The optional research-ON sibling exists only for controlled codegen/scheduling comparisons. Its TLS scope must be entered on the actual pool worker outside a timer; it changes count/fill scheduling, not P, geometry, scans, prefix, budget or fallback. It is not a new production policy. See `SAFETY.md` and `validation.json` for exact completed gates and limits.
