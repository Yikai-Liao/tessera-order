# Architecture audit

Fix the scope, then trace entry points, users, state, and runtime configuration along real call, modification, or recovery paths, checking only the boundaries relevant to the chosen tasks. Verify entities, cardinality, authority, and temporal meaning (current versus historical) against actual definitions and usage.

## What counts as an architectural problem

Architecture assigns units, ownership of knowledge and invariants, collaboration contracts, and lifecycle responsibility.

An architectural problem needs all three: **a concrete structural object, a causal mechanism that damages a class of tasks, and a structural change that cuts the causal chain**.

## Metrics review

Prefer the project's existing reports, measure in proportion to the question, and confirm source revision and counting rules. Record missing coverage or a failed measurement as unknown, not as zero.

`CRAP = CC² × (1 − c)³ + CC`, where `CC` is cyclomatic complexity and `c` is coverage as a ratio from 0 to 1. Use it to locate complex, thinly covered functions and order the review, following the project's effective threshold; past the threshold, judge structural and verification payoff against real maintenance tasks.

Check assertions on key boundaries, failures, and state transitions; choose verification per [test protection and maintenance cost](design-behavioral-tests.md).

## Inspection checklist

| Object | Check |
|---|---|
| Change and knowledge | For a frequent or high-consequence change, who must interpret which rules in sync? Are the contracts reachable? Where structural similarity reaches the project's threshold, are copies independently interpreting one rule, and what would sharing cost in coordination? Are retained copies handled per [retained copies](design-module-boundaries.md#retained-copies)? |
| Modules and interfaces | Does the path from creation to result, failure, and cleanup have an owner? Where does input become a valid representation, and how are its constraints kept? What pairing, conversion, ordering, and recovery remain with callers? Is shared responsibility scattered, or are independent responsibilities bound together? Do artifacts of advanced entry points return to shared handle and resource governance? Do common escape hatches reveal a primitive gap? |
| Errors and recovery | Who owns the recovery information? Are there repeated catches, wrapping, or retries? Is unknown treated as failure or success? Does an aggregated error keep the semantics callers need to decide? |
| Lifecycle | Do startup, admission, shutdown, cleanup, scaling side effects, and recovery have conflicting owners? |
| Evolution | Has a temporary bypass become a permanent authority? Does a hard-to-reverse recommendation have a structurally different viable alternative? |
| Performance | How do layout, conversions, copies, cross-boundary calls, and resource contention affect the goal? When do intermediates, scratch space, and stale payloads reside at the same time? Separate derivable work from bottlenecks that need measurement; baselines and structural changes may be recommended |
| Async and events (when present) | Does admission reliably reach scheduling? Who decides terminal state, cancellation, and recovery? Who governs event schemas, ordering, replay, and side effects? |
| Shared or multiple copies of state (when present) | Which state truly needs to be shared and mutable, and can independent state be separated? Who may write, and who is authoritative? Who maintains freshness, read-after-write, invalidation, rebuild, and partition invariants? |
| Failure and delivery (when present) | Are deadlines, retries, queue bounds, and backpressure consistent? Can an optional dependency block core capability? Do replicas share a failure domain? Who owns old/new version coexistence, cutover, and data recovery? |

## Recommendations and report

Each finding states the boundary, task evidence, causal chain, impact, a proportionate change, and verification; order by frequency, consequence, and strength of evidence, using the project's severity levels.

Record deferred improvements below the architectural criterion under [cumulative debt](design-module-boundaries.md#cumulative-debt); assess necessary boundary copies under [retained copies](design-module-boundaries.md#retained-copies).
