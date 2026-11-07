# Parallel count-window research checkpoint

This is source and validation evidence, not runtime adoption or performance acceptance. No benchmark samples, timing harnesses, compiled artifacts or machine-specific paths are included. Verified measurement results may be appended separately; this packet makes no speedup claim.

## Reproduce in an isolated checkout

Start from public commit `0bc80b49d5efd5bd8b62165b170015e5803226b7`. Apply the following patches in order, using `git apply --check` before each application:

1. `patches/01-combined-current-main.patch`: an unchanged copy of the already published `research/patches/combined-current-main.patch`.
2. `patches/02-e8-default-insertion.patch`: the exact Rust delta from upstream `e8e590616f0d3cfb5e8a30b84e4b3e43042036a4`. It changes only default sealed insertion finishing to identity `ByKey`, with its comment.
3. `patches/03-parallel-count-window.patch`: the new count-window control and tests; final package identity is normalized to the repository name `tessera-sort`.

The inherited combined patch selects a research-only Cargo workspace without the public benchmark member. Do not apply this lineage to a production branch as an adoption step.

Forward application, reverse application of all three patches, and a second forward replay were checked without compiling or running tests during packaging. Reverse replay reproduced the original exported base source exactly. The final replay matches frozen candidate `add21ec01b2982c32dbc430e98444a866743cd0de03801f7d9968213e1b82d71` across every Rust file and recorded build input. Only its private package name is normalized in `Cargo.lock` and `rust/tessera-sort/Cargo.toml`. `replay-proof.json` contains relative file hashes and patch checksums.

## Feature boundaries

- Original control: apply patches 1 and 2 only. Its parallel public API still uses the original serial count-and-fill route for eligible narrow direct keys. This is the correct original denominator, not an obsolete partition/scatter path.
- Candidate: apply all three. With `rayon`, eligible direct u32/u64 windows use the new parallel count/fill hook. Without `rayon`, serial behavior is unchanged. Public generic ByKey records decline the hook.
- Optional `count-window-research`: provides `count_window_research::with_serial_schedule`. Enter its panic-safe thread-local scope on the actual pool worker, outside the timer, around the same public sort. It changes only count/fill iterator scheduling; P, table/chunk geometry, probes, prefix computation, budget and fallbacks stay identical. Normal builds have no TLS read. P1 retains the original public serial fallback.

See `DESIGN.md` for scope and storage, `SAFETY.md` for completed checks and precise limits, and `LICENSE` for redistribution terms.
