# Benchmarks

The CLI is `pnpm bench --help` (Node.js 24; install with `pnpm install --frozen-lockfile`). Choose the task before loading its documentation:

| Task | Command | Read |
|---|---|---|
| Screen uncommitted code against a commit | `pnpm bench compare HEAD` | [Working-tree comparisons](../docs/development/benchmarking.md#working-tree-comparisons) |
| Compare sorting algorithms | `pnpm bench run --plan release --simd` | [Capabilities and procedure](../docs/development/benchmarking.md#command-capabilities) |
| Preview inputs without measuring | `pnpm --silent bench plan local` | [Experiment plans](../docs/development/benchmarking.md#experiment-plans) |
| Rebuild a report without measurements | `pnpm bench report bench/results/SESSION` | [Result format](../docs/development/benchmarking.md#results-and-provenance) |
| Read current-head PR feedback | `pnpm --silent bench pr 123 --json` | [Feedback checks](../docs/development/continuous-benchmarks.md#read-feedback-from-a-pr) |
| Change tooling | — | [Ownership and source map](../docs/design/benchmark-tooling.md) |
| Change figures | — | [Figure guide](../docs/development/benchmark-figures.md) |

The smoke algorithm plan is the default for `run`; release/native coverage is explicit. Raw results and fetched sources are ignored, not committed. Checks are in [AGENTS.md](../AGENTS.md#commands); native smoke validation runs in CI.
