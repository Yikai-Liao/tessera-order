---
name: software-design-philosophy
description: >-
  Use for structural decisions within or across modules: designing modules or interfaces; choosing data representations, state ownership, responsibility, invariant boundaries, call protocols, or error contracts; auditing architecture; or searching for whole-system simplifications. Also use when a minimal change would add a special case, flag parameter, parallel entry point, or second interpretation of a rule. Not for mechanical implementation with structure and contracts already settled, pure syntax or library usage, local tidying, incident mitigation, or performance troubleshooting alone; applies when performance shapes a representation or boundary choice.
---

# Software Design Philosophy

Decide software structure and mechanisms through the lens of *A Philosophy of Software Design*: complexity, information hiding, and deep modules. Applies from the first version of the code through simplifying an existing system.

## Shared judgments

- Judge **change amplification, cognitive load, and unknown unknowns** from real modification or call tasks, weighted by task frequency and failure impact. Measure cognitive load and unknown dependencies for a maintainer who works only from the code, contracts, and documents reachable in the repository; a convention that exists only in the author's session or memory counts as an unknown dependency.
- **Cyclomatic complexity > 10 triggers a review, not a required change.** Use coverage and CRAP to set priorities, then decide whether to keep, split, or reorganize based on conceptual integrity, information hiding, invariants, caller burden, and readability.
- Bound test maintenance cost by required behavior and past regressions; when existing protection is sufficient, adding no tests is acceptable.
- Identify concepts, cardinality, and invariants from real use cases before choosing representations and responsibilities; distinguish identity, instance, current fact, and historical snapshot.
- A **deep module** delivers strong capability for a low interface burden. **Information leakage** is one decision that must be interpreted independently in several places and kept consistent.
- Before adding a new consumer of format parsing or building, a lookup table, a threshold, or an identifier rule, find that knowledge's existing interpreters and its authority; route through the authority, or keep a copy under [retained copies](references/design-module-boundaries.md#retained-copies). Reviews judge against all consumers and list the new consumers with their authority.
- Before introducing an abstraction, state, or protocol, compare it with a direct implementation and with what the project and its libraries already provide: which knowledge does it hide, which invariants does it maintain, which caller obligations does it remove? Extension seams serve changes that have evidence and would be expensive or hard to reverse later.
- Give the complete operation to the party with the information to maintain the invariant, reducing external pairing, assembly, and cleanup. Leave callers the correctness, permission, persistence, and lifecycle choices that only they can make; when callers routinely take an escape hatch, first look for a gap in the core capability.

## Read by problem

| Current problem | Read |
|---|---|
| Auditing a repository or subsystem; separating architectural problems from local defects; cyclomatic complexity, coverage, and structural duplication | [Architecture audit](references/audit-software-architecture.md) |
| Verification gaps, test admission, maintenance cost, stopping; dedicated test / agent debugging surfaces | [Test protection and maintenance cost](references/design-behavioral-tests.md) |
| Concepts, representation, state authority, combining or splitting responsibilities, retained copies of a rule | [Module and knowledge boundaries](references/design-module-boundaries.md) |
| Complete operations, caller burden, internal organization, extension | [Deep modules and interfaces](references/design-deep-modules.md) |
| Normal interface settled; organizing errors and recovery responsibility | [Error handling boundaries](references/reduce-exception-surface.md) |
| Whole-system simplification, or repeated representations, conversions, coordination, or parallel implementations found while coding; retiring a specialized path, flag, or state; recording rejected candidates | [Global simplification search](references/search-global-simplification.md) |
| Structural candidates for important decisions; evolution under deadlines and compatibility constraints; sequencing a multi-step simplification or migration | [Comparing and evolving designs](references/evolve-design-strategically.md) |
| Work volume, resident memory, layout, batch capability, advanced entry points | [Design under performance constraints](references/design-measured-critical-paths.md) |
| Cross-process work, async tasks, replicated state, recovery contracts | [System boundaries and failure contracts](references/design-system-boundaries.md) |

## Coding and simplification

Apply design judgment while implementing. When implementing a new design or a requested refactor, carry responsibilities, representations, and protocols through the whole affected operation, then remove old mechanisms that lost their purpose. When structure or contracts change, re-evaluate the affected parts and finish the in-scope adjustments.

- For a new module or public interface, write the interface comment first: the decisions it hides, the contract callers must know, and the failure semantics. If the comment cannot be written briefly, or using the interface correctly requires describing internal steps, adjust the design before implementing.
- For a new module or public interface, sketch at least one alternative that changes the authority, representation, or call protocol, and compare both on the same call task before deciding. Private implementation details are exempt.
- When modifying existing code, if the minimal change would add a special-case branch, a flag parameter, a parallel entry point, or a second interpretation of the same rule, first ask what the structure would look like had this requirement existed from the start. If the comparison reveals a meaningful structural gap, refactor within this task only when the user asked for a refactor; otherwise stay within the requested work, register the gap as [cumulative debt](references/design-module-boundaries.md#cumulative-debt), and propose the refactor. Differences in external protocols can stay at the adapter edge.
