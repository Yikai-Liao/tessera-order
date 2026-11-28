# System boundaries and failure contracts

Structural recommendations must correspond to real pressure, workflow contracts, and state/rule authority, and compare the added failure cost against simpler candidates.

## From logical modules to deployment units

Compare same-process, one artifact started in different roles, and independent deployment separately. Autonomy gains must pay for communication, versioning, and recovery cost.

Organize pipeline stages around transformation knowledge and necessary isolation boundaries, accounting for coordination cost. The pipeline owns ordering, cancellation, backpressure, and the lifecycle of intermediate artifacts.

API aggregation follows whoever owns each part: domain rules stay with the domain authority, presentation shaping stays with the client side.

## Async tasks and events

Async interfaces cover task identity, idempotent submission, status query/notification, cancellation, result permissions, and retention. Admission must reliably hand over to scheduling, with accepted, executing, result-visible, and final-failure states made explicit.

A single state machine decides cancellation and completion; late messages never move a terminal state backward. By risk, check for unscheduled tasks, worker crashes, duplicate submission, cancellation races, and offline recovery windows.

Queues and logs define capacity, backlog/expiry, poison messages, and recovery responsibility. Events define schema authority, compatibility, per-partition ordering, retention, and the side effects of duplicates, late arrivals, and replay.

Choose orchestration or choreography by where process rules live and how failures recover. Cross-transaction compensation must accept intermediate states and define compensation failure, reconciliation, or manual recovery. Compensation runs as a new operation.

## External side effects and unknown outcomes

Before calling, retryable external writes durably record a business operation ID, idempotency key, request digest, and state; on recovery, query or replay under the original identity.

Distinguish confirmed success, confirmed rejection or non-execution, and an unknown outcome. For an unknown outcome, query the external authority; if it cannot be determined, reconcile or handle manually. Stop automatic retries when you cannot prove the replay window is still within the downstream idempotency retention period.

Acknowledge input only after the local terminal state and the results to publish are reliably committed; publishing may repeat, so consumers must still be idempotent.

## Multiple copies of state

Settle the authority and write rules first; multiple independently writable authorities need explicit coordination. Ground consistency in read-after-write visibility, no session regression, maximum staleness, and failure intermediate states.

| Candidate | Conditions and contract |
|---|---|
| Cache / precomputation | Real reuse, cost, quota, or offline pressure. Define fill/invalidation, staleness, concurrent misses, cold start, and origin protection; offline copies also need conflict handling and sync |
| Read replicas | Route by business freshness; read-after-write goes to the primary or waits to catch up, accounting for latency and fallback load |
| Sharding | Single-node capacity or isolation is a real constraint; use the real key distribution to check hotspots, locality, cross-shard transactions, routing authority, and resharding recovery |
| CQRS / projections | Read and write shapes truly differ; derivation direction, lag detection, and rebuild are defined |
| Event sourcing / CDC | Historical replay or incremental sync is actually needed; the former makes business events the authority, the latter keeps the original state store as authority. Historical schemas and side-effect replay must be controllable |

Versions, locks, or leases that callers must respect are part of the contract; expired holders must be blocked from writing. Verify the relevant invalidation, lag, conflict, and rebuild.

## Failure, capacity, and delivery

Retry classification and budgets are in [error handling boundaries](reduce-exception-surface.md); here, check cross-boundary amplification:

- Queues are bounded and can apply backpressure to producers; resource isolation covers the resources actually shared.
- Availability depends on failure domains, shared databases/identity/control planes, and cutover evidence; cutover must block writes from the old primary.
- Releases account for old/new protocol and schema coexistence, capacity, traffic evidence, and rollback speed; shadow traffic isolates real side effects.
