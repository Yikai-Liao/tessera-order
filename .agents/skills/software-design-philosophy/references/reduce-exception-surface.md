# Error handling boundaries

Reduce the states and places callers must handle, while keeping diagnostic information and necessary failure semantics. Choose by who owns the recovery information:

- When user intent allows, define errors out of existence, e.g. "ensure the object is absent" instead of "delete the object".
- Recover locally when this layer has enough information and will not repeat a dangerous side effect; otherwise give the caller that owns the transaction or business policy a stable, distinguishable result and let it decide.
- Aggregate internal failures when callers would handle them the same way, keeping causes and trace information.
- When an invariant is broken and continuing is unsafe, terminate the smallest failure domain; first assess persistent state and recovery capability.

A timeout only means you stopped waiting. Distinguish permanent rejection, transient failure, overload, and unknown outcome.

One primary owner manages the end-to-end deadline, retry count and time limits, and backoff with jitter. Handle overload with rate limiting, backpressure, or rejection. When idempotency and side-effect boundaries are unclear, query or reconcile; for the concrete protocol see [system boundaries and failure contracts](design-system-boundaries.md#external-side-effects-and-unknown-outcomes).
