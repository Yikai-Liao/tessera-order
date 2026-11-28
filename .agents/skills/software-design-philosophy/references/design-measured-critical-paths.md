# Design under performance constraints

Compare representations and boundaries by work volume and resource lifecycle; assess runtime cost and maintenance payoff separately.

## Work volume and residency cost

Estimate, at the expected scale, the scanning, copying, encoding, allocation, and simultaneous residency along the full path; state assumptions when scale is unknown. Compare algorithms, layout, batching, co-location, and eliminating conversions before considering caching, parallelism, or special paths. Measure key trade-offs and hard thresholds in proportion.

The first version prioritizes the main flow. Before optimizing, estimate the share of work saved and the added maintenance cost.

Follow inputs, intermediates, final representations, scratch space, and stale-but-held payloads from creation and handover through last use to release. Count object counts, inline reservations, heap capacity, slices that keep a parent buffer alive, and concurrently active tasks.

## Advanced entry points

Ownership of capability gaps follows [deep modules and interfaces](design-deep-modules.md#assign-each-gap-an-owner): when a general interface lacks a batch or consistency primitive, add the capability first.

Views/spans, shared buffers, and similar must state ownership, lifetime, reference invalidation, mutability, thread safety, and the fallback to the ordinary or copying path.

## Decide the trade-off

Performance conclusions need representative load, a current baseline, and a comparable environment; check the parallel resources actually available, and account for variance, cold start, tail latency, memory, and maintenance cost. Fast and slow paths must uphold shared invariants. When an implementation is duplicated to protect code generation, inlining, or layout, record the measurement it rests on next to it, check shared invariants under [retained copies](design-module-boundaries.md#retained-copies), and register the evidence needed for merging under [cumulative debt](design-module-boundaries.md#cumulative-debt).

Without measurements, distinguish derivable cost, suspected bottlenecks, and undecided payoff, and pick the smallest measurement that can decide the trade-off; verify what an existing mechanism actually does before keeping or removing it.
