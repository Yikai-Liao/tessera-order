# Module and knowledge boundaries

## Start from semantics and knowledge

Identify data units, cardinality, and temporal relationships along real use cases; distinguish identity, historical instance, current value, snapshot, and derived value, and choose types and boundaries accordingly.

Trace the rules that must be interpreted together along representative modifications, prioritizing frequent or high-consequence tasks.

Give authority to the party with the information to maintain the invariant, and make explicit how state is created, modified, handed over, invalidated, and released. Before adding derived state, or removing state or parameters, check whether the value is derivable on every legal call and state change, and at what cost.

For concurrency, first check whether one mutable state really needs to be shared; independent state can be separated. When sharing is required, set the write scope by the shared invariant and cover every legal write path with an existing transaction, lock, ownership, or single-writer mechanism; account for the copying, memory, and join cost after separation.

When successive patches fail under the same condition, check the shared premise and the assignment of responsibility. Distinguish unreachable knowledge, an insufficient contract, an implementation error, and a verification gap; adjust authority, representation, or protocol only when evidence shows the premise is wrong.

## Choose a boundary

- **Merge**: one rule or state machine always has to be understood and changed together, and merging removes an intermediate representation or duplicated recovery; also check whether it introduces unrelated dependencies, coupled releases, or wider permissions.
- **Split**: responsibilities have independent reasons to change and can be understood separately after the split; account for the new interface, state passing, navigation, and coordination cost.
- **Keep**: a thin boundary genuinely translates an external protocol, isolates trust or failure, or protects a stable contract.

Use named records for fields that hold together and variants for genuinely exclusive choices, with each branch carrying only the data and handling it needs. Use standard containers or iterators directly when they satisfy the consumer contract. Choose representations by real growth limits and lifecycles.

Options the module can reliably decide move inside; choices only the caller can make stay in the contract.

Personal choice: **no multiple-base implementation inheritance**. For plugins, prefer composition, interfaces, traits, or concepts.

## Retained copies

When a second interpretation of the same knowledge is kept, whether for a cross-language, cross-process, or trust boundary, for performance, or because an adjustment is deferred:

- Name the authority and the reason the copy is kept.
- Detect drift with generation from one source, shared fixtures, or contract tests; the check must catch mismatches between the copies, not only each copy's own validity.

## Cumulative debt

Register deferred structural improvements, avoidable duplicate interpretations, and missing mismatch checks in the project's backlog or roadmap, even when they fall short of the architectural problem criterion. Record the affected callers or consumers, the responsible authority, the reason for deferral, and the evidence or condition for revisiting the adjustment.

Necessary boundary copies with a clear authority and effective mismatch checks can remain part of the intended design.

## Check the actual payoff

Walk a representative call or replay the original task: is authority consolidated, do necessary adapters stay at the edge, do callers need less internal knowledge?

Parse and validate external data at the actual trust boundary, and protect expressible invariants with existing types, schemas, or controlled write entry points. While validated facts remain protected by those constraints, use the valid representation directly inside; check new trust boundaries, mutable permissions, or concurrent state at the operations where they arise.

When type assertions, casts, or ignored checks repeatedly cross a boundary, check whether the interface fails to express the contract.
