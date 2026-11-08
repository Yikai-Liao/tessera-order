# Benchmarks

Use Node.js 24 and the pnpm version pinned in `package.json`. Install the tools once:

```sh
pnpm install --frozen-lockfile
pnpm bench --help
```

| Task | Command |
|---|---|
| Compare uncommitted code with a commit | `pnpm bench compare HEAD~1` |
| Select cases and pin measurement to one CPU | `pnpm bench compare main --types u64,row_u64 --distributions random,zipf2 --cpu 2` |
| Validate and measure Rust competitors | `pnpm bench run` |
| Produce the full native and SIMD comparison | `pnpm bench run --plan release --simd --cpu 2` |
| Measure one complete release group | `pnpm bench run --plan release --group u64/random` |
| Preview or save a custom selection | `pnpm --silent bench plan local --types u32,u64` |
| Run a saved algorithm selection | `pnpm bench run --plan my-plan.toml` |
| Regenerate figures and tables | `pnpm bench report bench/results/SESSION` |
| Read the latest continuous feedback for a PR | `pnpm bench pr 123` |
| Read that feedback as JSON | `pnpm bench pr 123 --json` |

The [benchmark guide](../docs/development/benchmarking.md) explains measurement protocols, requirements and result formats. Local comparisons are development screens with process-level A/A controls, not in-process placement-controlled performance claims. Published algorithm comparisons use the full release protocol.

`compare` defaults to `HEAD` and the `local` plan: one 65,536-element random `u64` API case. It takes nine blocks, each containing two baseline processes and one working-tree process. `run` defaults to the small Rust comparison plan; the full native plan must be requested explicitly. Both use the [same JSON/TOML selection rules](../docs/development/benchmarking.md#experiment-plans), with CLI overrides for algorithms, types, distributions, lengths and API modes. Continuous coverage is configured in `bench/plans/continuous.toml`. `--plan` accepts TOML and JSON files; `plan` prints TOML by default or JSON with `--format json`. Each result directory retains the normalized `plan.json`.

Both commands accept `--out DIR` and require a new output directory. The default is `bench/results/<timestamp>`. Source snapshots and builds live under `~/code/tessera-bench/` (override with `TESSERA_BENCH_WORK`) and are removed on completion or a handled failure; results remain. A hard kill can leave a worktree: list it with `git worktree list`, remove that session's `tree` with `git worktree remove --force`, and delete that session's remaining build files.

## Module boundaries

```text
bench/catalog.toml       identities, labels, families and comparison suite order
bench/catalog.ts         typed catalog access and case-ID presentation
bench/plans/             local, continuous, smoke and release selections
bench/config.ts          JSON/TOML syntax decoding
bench/plan.ts            shared plan validation, CLI overrides and JSON Schema
bench/index.ts           command-line interface
bench/workspace.ts       source snapshots, builds and resource lifetime
bench/local/             commit comparison execution, analysis and report
bench/release.ts          algorithm-comparison workflow
bench/rust/              deterministic inputs, adapters and measurement protocols
bench/report/            algorithm-result validation, analysis and report
bench/continuous/        Bencher history, reports, GitHub publication and worker
bench/plot/              host-independent figure templates and visual theme
bench/fetch.py           pinned native-source acquisition
bench/vendor/            ignored third-party source trees
bench/results/           ignored local results and reports
```

The [tooling design](../docs/design/benchmark-tooling.md) describes ownership and extension points. The [figure guide](../docs/development/benchmark-figures.md) documents the visual grammar and includes a browser-rendered example. The [continuous guide](../docs/development/continuous-benchmarks.md#read-feedback-from-a-pr) explains how contributors and agents read PR feedback.

## Checks

```sh
pnpm bench:typecheck
pnpm bench:test
python3 -m unittest discover -s bench/continuous -p "test_*.py"
cargo test -p tessera-bench --features compare --lib --bin memory
```

The native smoke checks run in CI. Third-party sources and raw measurements are not committed.
