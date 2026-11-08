---
name: tessera-pr-performance
description: Read current-head Tessera PR benchmark feedback and distinguish regressions, improvements, failed measurements and pending reports. Read-only; no compilation or benchmark run required.
---

<!-- SPDX-License-Identifier: BSD-2-Clause -->

# Read PR performance feedback

Run from the repository root with Node.js 24, the pinned pnpm dependencies and authenticated `gh`. This workflow reads existing feedback; it does not build code, start benchmarks or change remote state.

## Get the report

```sh
gh auth status
pnpm --silent bench pr 123 --json
```

Replace `123` with the requested PR number. If dependencies are absent, run `pnpm install --frozen-lockfile`. If authentication is missing, ask the operator to authenticate; do not claim feedback is unavailable because no benchmark ran.

## Evaluate

1. Match the returned `head` to the PR current commit. Read `checks[].conclusion`, `checks[].output.summary` and the matching `comment`; stale-head feedback is not the current result.
2. Check **both** `Benchmarks (rust, Default)` and `Benchmarks (rust, Portable SIMD)`. A green `Track benchmarks` workflow does not imply those checks passed. Distinguish a measured alert, a failed measurement and a missing comparable baseline.
3. A missing comment or pending check means feedback is not ready, not clean. If checks are absent, inspect the linked `Build benchmarks` run, then `Track benchmarks`.
4. For stacked PRs, verify the comparison uses the parent at the resolved base commit, not the whole stack against `main`.
5. Read each build summary and its alerted cases. Negative latency changes mean less run time. Do not describe a failed or unmeasured build as the clean opposite of a measured alert.

Report the head, each build verdict, relevant cases and baseline, and any pending, missing or failed evidence. Continuous benchmarks have no in-process A/A or placement controls: neither alerts nor clean checks establish a formal speedup or regression claim.

## Only investigate further when needed

- Missing baselines, queueing, tracking configuration or report layout: read the relevant section of [continuous benchmarks](../../../docs/development/continuous-benchmarks.md).
- A measured alert requiring a new experiment: use [the measurement workflow](../tessera-measure/SKILL.md) and its protocol. Do not launch measurements during a read-only review without authorization.
