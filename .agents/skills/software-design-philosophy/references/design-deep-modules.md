# Deep modules and interfaces

## Module depth and internal organization

One boundary is externally responsible for a complete operation; internally it may be organized into stages with clear inputs, outputs, and effects. Business decisions with clear input-to-result mappings can be separated from external effects. Splitting preserves invariants, transactions, lock scope, and resource lifecycles; state reads, decisions, and commits cooperate with the atomicity they require.

## Test primitives against complete workflows

Check primitives against real use cases from creation to result, failure, and cleanup, including the relevant long tail. Judge depth by the pairing, conversion, ordering, and cleanup callers actually carry.

Organize primitives around a shared object, range, or state transition, and let the module that has the information to guarantee it complete the operation. Where read/write phase constraints exist, take back the necessary joins and failure cleanup so that writing starts only after all readers finish and the operation returns only after all writers finish.

State success, ownership, side effects, and failure boundaries from the caller's point of view, and check the guarantees for preparation, internal mutation, commit, and publication against the implementation. Plug into the project's existing conventions for errors, cancellation, parallel scheduling, and observability, with narrow adapters for necessary differences.

For complex systems or hard-to-reproduce workflows, follow the [dedicated test / agent debugging surface preference](design-behavioral-tests.md#dedicated-test--agent-debugging-surface).

## Assign each gap an owner

| Gap | Owner |
|---|---|
| Needed repeatedly by several callers and part of the module's knowledge | Core primitive |
| Primitives suffice; the difference is specific business logic | Composition in an upper layer |
| Device, vendor, or external protocol differences | Specialized implementation or adapter behind a shared contract |
| An important choice only the caller can make, or an object-specific capability | Narrow, typed advanced entry point |

The ordinary path uses defaults the module can reliably decide. Advanced entry points reuse the shared identity, lifecycle, permissions, errors, and observability, and their artifacts return to shared handles and resource governance.

When the same escape hatch keeps becoming a common path, re-evaluate the core primitives. For the layout and lifecycle semantics required of advanced entry points, see [design under performance constraints](design-measured-critical-paths.md#advanced-entry-points).
