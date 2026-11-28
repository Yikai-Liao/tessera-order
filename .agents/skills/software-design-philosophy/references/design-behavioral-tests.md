# Test protection and maintenance cost

Choose verification around this change and its actual impact; do not use TDD by default to generate architecture.

## Confirm the verification gap first

Check existing assertions first; a pure refactor that is already well protected only needs the relevant checks run. When there is a gap, prefer strengthening existing assertions or parameter tables; add a new test only when the gap cannot be clearly absorbed there, and add fixtures only when setup has an independent need.

Each test added or rewritten in this round needs independent value: a regression for a real defect, a contract or consumer dependency, a boundary or state transition that can fail differently, a plausible fault existing checks miss, or carrying an existing guarantee at lower cost.

Exploration may sample broadly; permanent tests pass admission separately. Keep a proportionate regression for each real defect, preferring a minimal reproduction or a compact property.

## Choose the verification method

Assertions use independent expected values and check behavior and invariants; keep reference models simple and independent.

Place seams at dependencies that are genuinely uncontrollable or expensive, preferring controllable, fast real implementations.

Verify independent dimensions separately and keep a connecting check; keep targeted combinations where interaction risk exists.

## Dedicated test / agent debugging surface

Personal preference: simple programs keep the existing test entry points; for complex systems or hard-to-reproduce workflows, ask the user whether to consider a dedicated test / agent debugging surface.

The interface expresses operations as typed schemas; retryable operations have explicit identity and idempotency. For debugging surfaces that span environments or carry privileged capabilities, use short-lived credentials scoped to the environment and capability, with auditing and automatic revocation; keep production unreachable through separate deployment or environment gates, and test the rejection path.

Provide a repeatable starting point and the state observation needed; drive target transitions through the real public operations.

## Optional mutation checks

Use targeted mutation according to unresolved risk, tool fit, and runtime cost; skip it when existing assertions, boundary, or differential evidence suffice. When used, separate behavior gaps, equivalent or low-value mutants, uncovered code, and execution errors, and strengthen verification only for confirmed behavior gaps.

## Trim and stop

Cost covers maintaining tests, fixtures, mocks, reference implementations, generators, helpers, snapshots, and data, plus running them and locating failures. When large setup or a new framework is introduced, the project's budget is exceeded, or run or diagnosis cost rises noticeably, review duplicate protection, setup, and test level. Merge or delete by behavioral purpose, carry over the original guarantees, and run the relevant checks.

When this round added or rewrote tests, trim them once by these rules before delivery. Stop adding tests once required behavior is verified, real defects have proportionate regressions, and required checks pass.
