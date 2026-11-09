# Benchmarks

The CLI is `pnpm tools --help` (Node.js 24; install with `pnpm install --frozen-lockfile`). Choose the task before loading its documentation:

| Task | Command | Read |
|---|---|---|
| Screen uncommitted code against a commit | `pnpm tools compare commit HEAD` | [Working-tree comparisons](../docs/development/benchmarking.md#working-tree-comparisons) |
| Attribute a revision change with placement controls | `pnpm tools compare revisions BASE --current HEAD --simd` | [Linked revisions](../docs/development/benchmarking.md#linked-revision-comparisons) |
| Compare sorting algorithms | `pnpm tools compare algorithms --plan release --simd` | [Capabilities and procedure](../docs/development/benchmarking.md#command-capabilities) |
| Preview inputs without measuring | `pnpm -s tools plan local` | [Experiment plans](../docs/development/benchmarking.md#experiment-plans) |
| Rebuild a report without measurements | `pnpm tools report bench/results/SESSION` | [Result format](../docs/development/benchmarking.md#results-and-provenance) |
| Read current-head PR feedback | `pnpm -s tools status --pr 123` | [Feedback checks](../docs/development/continuous-benchmarks.md#read-feedback-from-a-pr) |
| Change tooling | — | [Ownership and source map](../docs/design/benchmark-tooling.md) |
| Change figures | — | [Figure guide](../docs/development/benchmark-figures.md) |

The smoke algorithm plan is the default for `compare algorithms`; release/native coverage is explicit. Raw results and fetched sources are ignored, not committed. The [CLI reference](../docs/development/tooling.md) covers compact status, logs and CI reproduction. Checks are in [AGENTS.md](../AGENTS.md#commands); native smoke validation runs in CI.
