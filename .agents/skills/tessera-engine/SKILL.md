---
name: tessera-engine
description: Implement and validate Tessera Rust engine, key-encoding or block-partition changes, including panic safety and algorithm updates. Not needed for a read-only contract lookup.
---

<!-- SPDX-License-Identifier: BSD-2-Clause -->

# Change the engine

Run commands from the repository root. Read [the engine reference](../../../docs/design/engine.md) and [shared semantics](../../../conformance/semantics.md) together with this skill unless already loaded. For routing, thresholds, digit planning or inlining, include [the measurement protocol](../../../docs/development/benchmarking.md) and the relevant section of [engine measurements](../../../docs/design/port.md) in that same batch.

## Implement

1. Identify the affected stage and its invariant in the reference, source and tests. For a bug, reproduce the failure before changing the implementation.
2. Make the smallest complete change. Follow record ownership through moves, panic recovery and inconsistent keys; preserve exactly one live owner. Explain every new unsafe operation with its safety invariant.
3. Add or extend a focused regression test. Exercise the changed boundary as well as panic, changing-key, owned-record and `&mut`-record behavior where affected. Do not weaken those tests to accept the change.
4. Update semantics only for an intentional contract change. Algorithm changes also update the relevant explanation and simulation in `docs/.vitepress/theme/sim/`. Keep rationale with the current design, not a parallel decision record.

## Validate

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo test -p tessera-sort --no-default-features
cargo test -p tessera-sort --features portable-simd
```

Run affected tests under Miri; [contributing](../../../CONTRIBUTING.md#development) gives the commands. Run `pnpm docs:test` when simulation models change and the site checks when their pages change. CI owns the remaining platform, MSRV and sanitizer lanes; [its workflow](../../../.github/workflows/rust-ci.yml) is the command source for reproducing one.

For a performance-sensitive change, use the [measurement workflow](../tessera-measure/SKILL.md) before accepting it. A local development screen or continuous alert alone is not placement-controlled evidence.

## Finish

Report the invariant preserved or contract changed, the regression coverage, commands actually run and anything left to CI. Include paired evidence for performance choices and investigate results outside the controls. Do not report an unavailable check as passed.
