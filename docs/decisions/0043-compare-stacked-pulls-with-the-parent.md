# 0043: Compare a stacked pull request with its parent

Date: 2026-11-08. Status: accepted. Amends the comparison base in the [continuous benchmark guide](../development/continuous-benchmarks.md).

## Context

Bencher stores a branch's samples under its branch name, and the tracker submits a pull request's samples under `pr-<number>`. A base branch's GitHub name is therefore not a Bencher branch: for a stacked PR it is the parent's feature branch, which Bencher never holds. The tracker passed that name as `--start-point`, Bencher recorded no start point without reporting an error, and the report marked every case as new while both checks passed. [Pull request #42](https://github.com/Yikai-Liao/tessera-sort/pull/42) showed this: 42 measured cases per configuration, `start_point` null, checks green.

The alternatives were to compare a stacked PR against `main`, which attributes the whole stack to the child, or to keep the branch name and accept a report without a comparison, which is the false green above.

## Decision

Resolve the parent pull request whose head is the child's base branch, and use its `pr-<number>` as the start point at the child's `base.sha`. Measure without a comparison and record the reason when the base branch has no parent or several.

A pull request check fails when the promised comparison is missing: no start point, a start point at another commit, or no comparable case. A `main` run records samples without a comparison by design. Charts and the trend axis name the comparison base instead of assuming `main`.

## Consequences

A stacked PR is measured against the code it builds on, so its alerts describe its own commits rather than the stack. Its parent must have been measured at the child's exact base commit; otherwise the check fails until that sample exists, instead of passing without one. Because the tracker runs default-branch code, the change applies after it merges. Comparisons against `main` are unchanged.
