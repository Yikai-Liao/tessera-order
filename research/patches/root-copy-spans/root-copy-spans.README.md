# Ordered root copy-span research checkpoint

This source-only patch adds private ordered-one and coalesced-full-run controls to the published combined-current-main research archive. It does not alter the public scheduler, establish a performance benefit, or authorize adoption. The frozen source is unchanged by packaging.

## Exact base and candidate

- Pure upstream archive: `0bc80b49d5efd5bd8b62165b170015e5803226b7`.
- First apply published `research/patches/combined-current-main.patch`, SHA256 `0d375cfb6f962fb6032c2a962833d9d003d15951403e220b69a84e0b1e0f6b7d`.
- Required combined base identity: `305068d9d6f4b1d0621c18426eee4945c8b45514f4b89037a276ed60fef37d70`.
- Candidate identity: `3903f8d1dfc75231eeddcb2cee4e2d72e72c495bb37f179e92bf6adc1495a41b`.

The base manifest uses SHA256 of sorted `hash + two spaces + path + newline` lines. The candidate identity deliberately retains the validated freeze convention: SHA256 of the sorted compact JSON path-to-hash mapping, with separators `,` and `:`. These two identity conventions are explicit; per-file hashes are authoritative for reconstruction.

## Reproduce source without building

Run from a repository containing the exact upstream commit and published combined archive. Set PACKET to the extracted packet directory.

```sh
UPSTREAM_PATCH="$(pwd)/research/patches/combined-current-main.patch"
PACKET="$(cd "$PACKET" && pwd)"
WORK="$(mktemp -d)"
git archive 0bc80b49d5efd5bd8b62165b170015e5803226b7 | tar -x -C "$WORK"
(cd "$WORK" && git apply --check "$UPSTREAM_PATCH" && git apply "$UPSTREAM_PATCH")
python3 "$PACKET/verify-source.py" "$WORK" "$PACKET/root-copy-spans.BASE-MANIFEST.json"
(cd "$WORK" && git apply --check "$PACKET/root-copy-spans.patch" && git apply "$PACKET/root-copy-spans.patch")
python3 "$PACKET/verify-source.py" "$WORK" "$PACKET/root-copy-spans.SOURCE-MANIFEST.json"
```

Do not apply onto public overlays or another experimental tree. Packaging independently recreated the exact upstream archive, applied the published base, verified all 25 base files, applied this patch and verified all 29 candidate files, then reversed this patch and checked every file byte, mode and symlink target in the complete combined-base tree. `APPLY-CHECK.json` records that source-only verification; no build/test/timing was repeated during packaging.

## Controls and scope

The package remains `tessera-combined-current-main`, library `tessera_sort`. Use feature `ordered-one` or `ordered-coalesced`, each implying Rayon. Neither is default; enabling both selects coalesced. For future causal comparisons select exactly one, and compare practical results against the actual untouched combined base. Match compiler, features and target flags across controls and clear all `TESSERA_*` overrides.

Only existing Cargo feature declarations and native root dispatch change. Four private modules implement the ordered mover and its source-level reference/metadata/safety tests. The native metadata fixtures embedded in the test source are needed to reproduce assertions; no separate raw-data files are included. Original generic `block.rs`, whole-block permutation, scatter, root geometry, key extraction, child scheduling/batching, production scratch and allocation capacities stay byte-identical.

Both ordered controls include the same O(C) pre-move validation pass. Its cost must be included in future timing; comparisons against untouched generic alone cannot isolate copy granularity. Runs use typed overlapping ptr::copy, stopping at partial prefixes, allocations and original semantic callback boundaries. Exact safety/progress/panic reasoning is in `PROOF.md`.

This packet contains only patch, proof, manifests, concise historical validation scope and reproduction checks. It excludes raw logs, timing data, benchmark harnesses, binaries, dependency/build trees, machine-specific paths and private user information. Required public copyright/license notices are retained in source. Performance results, if accepted later, belong in a separate documentation follow-up.
