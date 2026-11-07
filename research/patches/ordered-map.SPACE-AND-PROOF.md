<!-- SPDX-License-Identifier: BSD-2-Clause -->
# Ordered-map proof, ownership and space

## Completed-permutation invariant

Write C for physical block count, H for the free prefix, S=2H for scratch block
count, B for records per block, A for active end, and F=C−2H for input full
blocks. The normal whole-block permutation return establishes
`order[i]=(i+H) mod C`. The valid active sequence is logical `[H,A)`, with sorted
partial-prefix lengths, total live length equal to the input, and A≤C.

For each original main-loop destination d in `[2H,C)`, input i=d−H lies below
C−H. Hence `source=order[i]=d` and `inverse[d]=i`. The evacuation predicate is
false; all four map writes store their original values; the free cursor update
is false. Eliminating inverse construction and those operations leaves every
record operation unchanged. For the wrapped logical suffix `[C−H,A)`, source
is `i−(C−H)`, in the first H scratch blocks.

The ordered and generic paths share the copy/visit body. Each original logical
prefix retains its exact source physical ID, destination record offset and
length. Zero-length and self copies remain, as do overlapping `ptr::copy`
operations. There is no coalescing, non-overlap substitution, changed callback
batching, inlining annotation, or prefetch intervention.

## Ownership and failures

Completed partials only shorten output before the current input block. A main
copy ends no later than that source physical block's end; the remaining input
sources start at subsequent full blocks. The wrapped suffix reads scratch.
After the unchanged per-block visit point, the published input prefix therefore
cannot be read or written by subsequent compaction. Each pointer retains its
original input or scratch-allocation ancestry. Segmented scratch addressing is
unchanged, and no initialized reference is formed over partial-block padding.

The private ordered entry and common const-generic helper explicitly require
their invariants through unsafe call-site contracts. Only the branch immediately
after normal permutation return selects ordered compaction. Environment skip,
one-shard fallback, ordinary serial sorting, Drop and permutation-failure
recovery continue to use general compaction. No new runtime validation scan or
fallible operation is inserted after record movement starts.

The first callback panic is captured, later callbacks are disabled, all remaining
copies complete, and only then is `input_materialized` set and the payload
returned for resumption. This does not claim recovery from an arbitrary new
internal panic halfway through squeeze: neither old nor new directory state
can generally replay that partial squeeze. Test trace reservation occurs before
movement; `3(C−H)+1` entries cover squeeze/visit pairs, possible general
whole-block evacuations, and the final visit without reallocating during copies.

## Space and progress limits

This control adds **zero production allocations** and removes no allocation.
Both directory buffers retain their original lengths/capacities, including the
unused ordered inverse buffer. Every root scratch segment, permutation snapshot,
partial descriptor, child workspace and queued bucket pointer is unchanged.
This is not a measured heap reduction.

Consequently the inherited O(sqrt(n)) auxiliary bound in n, for fixed record
size, radix and configured parallelism, is unchanged for each active sorting
invocation/workspace. No new n-sized index array or buffer is introduced. Exact
next-directory capacity/address preservation is tested, but no new parallel
peak-heap measurement is claimed. Arbitrarily reentrant user code can suspend
multiple workspaces on one physical worker; this experiment does not prove their
count is bounded by the physical worker count or strengthen that broader
per-thread claim.

Materialized bucket ownership transfer, scope joins, standard-thread yielding,
root/child overlap and consumer scheduling remain unchanged. Native bounded
nested Rayon P2/P3/P4 and producer/consumer panic guards passed. These are not a
progress guarantee for arbitrary blocking cycles created by user callbacks.

## Causal reference

Moving the common body behind wrappers may affect generic/child code generation
or code layout. A refactor-only private benchmark copy R therefore replaces the
single `self.compact_impl::<true>(` call inside the ordered wrapper with
`self.compact_impl::<false>(`. A normally permuted directory meets the general
preconditions, as tested by paired reference fixtures. R retains wrappers and
map work; N/R is the closer map-elision comparison. The original combined O is
still the practical reference. This packet's candidate is N, not R, and neither
measurement outcome nor algorithm adoption follows from this proof.
