# Global simplification from necessary contracts

Use for whole-system simplification, or when repeated conversion, coordination, or dual implementations turn up while coding. Understand the overall cost along the relevant workflows and search for candidates; for adoption and migration, see [comparing and evolving designs](evolve-design-strategically.md).

## Set existing mechanisms aside

First ask: if today you only had to satisfy the known contracts, how would this workflow be implemented? Separate three kinds of facts:

- **Necessary contracts**: results, invariants, public commitments, consumer dependencies, and permission, persistence, failure, and performance requirements.
- **Existing mechanisms**: representations, conversions, caches, scheduling, adapters, configuration, extension points, and lifecycles.
- **Constraints to verify**: historical assumptions lacking a source or consumer evidence; verify them, then keep or drop them.

## Find mechanisms that can disappear together

Follow real workflows from input to result, failure, and cleanup, together with callers, runtime configuration, persistence, and external boundaries, looking for opportunities such as:

- Extend the reach of one authoritative representation, removing intermediate models, mappings, duplicate validation, and syncing along with it.
- Reassign state ownership, retiring mirrors, duplicate bookkeeping, invalidation, or compensation coordination.
- Take back a complete operation, eliminating external ordering, pairing, cleanup, and manual maintenance or proof obligations.
- Cover real use cases with complete primitives, retiring parallel entry points, old implementations, and their separate rules.
- Narrow freedom that has no consumers and is not a public commitment, or move an upstream boundary so a class of downstream branches no longer arises.

Work back from the burden to its upstream cause; compare how much of the old mechanism actually retires against the coordination added. Before adopting a candidate, verify the contracts and external dependencies the replaced mechanism carries.

A specialized path, flag, or state usually carries a fact or saving: an observation already made, a scan skipped, a capacity granted. A candidate that retires it names that fact and how the general mechanism obtains it at no greater cost; write that cost model before measuring, and measure only what the model leaves open. If no construction keeps the fact, the candidate is a trade-off, not a simplification. When one mechanism serves several purposes, separate them: a new design may discharge one purpose, and the remaining ones can move to independent owners, letting the mechanism retire.

## Evaluate candidates by net complexity

Keep searching separate from accepting. A candidate states which decision changes, which mechanisms disappear, and who carries the original guarantees, compared against the status quo or a local fix. Search aims at overall payoff; implementation advances in verifiable, reversible complete slices per the migration rules.

Compare the total burden across core implementation, callers, tests, configuration, deployment, and migration: which concepts, states, syncs, and failure paths go away, and which branches, conventions, or dependencies are added? Keep the status quo when the net payoff is insufficient; after adoption, retire the old mechanism and any temporary dual path per [retire old mechanisms](evolve-design-strategically.md#retire-old-mechanisms).

Record a rejected or shelved candidate in the repository, with the owner of the affected decision: the construction tried, the fact it lost or the cost it added, whether the direction is settled or only this construction failed, and the condition for reopening. Check these records before proposing candidates; a PR description or conversation is not a reachable record.
