---
name: tessera-measure
description: >-
  Design and run Tessera performance experiments: screen a source change, compare sorting algorithms, or investigate a regression with A/A and placement controls. Not for merely reading PR feedback.
---

<!-- SPDX-License-Identifier: BSD-2-Clause -->

# Measure a change

Run commands from the repository root with Node.js 24, the pinned pnpm and Rust toolchains. Install JavaScript dependencies with `pnpm install --frozen-lockfile`. No documentation index or retrieval CLI is needed.

## Choose the experiment

| Goal | Start with | Read together with this skill |
|---|---|---|
| Screen uncommitted code against a commit | `pnpm bench compare COMMIT` | [Working-tree comparisons](../../../docs/development/benchmarking.md#working-tree-comparisons) |
| Compare sorting algorithms | `pnpm bench run` | [Protocol](../../../docs/development/benchmarking.md) |
| Accept an engine performance change | A targeted paired source A/B experiment | [Protocol](../../../docs/development/benchmarking.md), relevant [engine evidence](../../../docs/design/port.md) |
| Regenerate an existing report | `pnpm bench report bench/results/SESSION` | Read the protocol only if interpreting or changing statistics |

If the task is only to inspect existing PR feedback, use [the read-only workflow](../tessera-pr-performance/SKILL.md), not a new experiment.

## Run

1. State the hypothesis, baseline commit, relevant inputs and expected unaffected controls. Preview coverage before starting an expensive run:

   ```sh
   pnpm --silent bench plan local --types u64,row_u64 --distributions random,zipf2
   ```

2. Choose a CPU allowed by the host affinity policy. Record host and compiler conditions; do not change governor, turbo or SMT settings without operator approval.
3. Choose the command for the goal:

   ```sh
   # Development screen; COMMIT must have a compatible API and harness.
   pnpm bench compare HEAD~1 --types u64,row_u64 --distributions random,zipf2 --cpu 2

   # Rust-only smoke comparison, including verification and reporting.
   pnpm bench run

   # Full comparison, including native opponents and a second SIMD build.
   pnpm bench run --plan release --simd --cpu 2

   # Render retained results without rebuilding or remeasuring.
   pnpm bench report bench/results/SESSION
   ```

   CPU 2 is an example, not a host-independent default. Full native comparisons require the [native prerequisites](../../../docs/development/benchmarking.md#run-a-comparison); do not start one just to inspect a report.

4. Both measurement commands accept `--out DIR` and require a new directory; the default is `bench/results/<timestamp>`. Keep raw results and research harnesses outside Git. Retain the resolved plan, source/build provenance, verification results and controls with the report. Interrupted or partial runs must remain labelled as such.

## Interpret and report

- A dirty-tree comparison is a process-level A/A screen, not a formal source-change speedup claim. Continuous alerts are triage, not acceptance evidence.
- Formal source-change claims require paired measurements, A/A ranges and independent code-placement controls under the protocol. Verify outputs against the stable reference outside timing.
- Investigate changes outside the controls, including apparently untouched cases. Do not assume noise or tune source layout to one linked executable.
- Report baseline and candidate, tested coverage, environment, ratios and control ranges, result location, and limitations. State separately what was not measured.
