# 7. Beyond sorting

**The core idea:** `argsort`, `select_nth`, and `sort_by_cached_key` each ask for less than a full sorted slice. Instead of maintaining separate algorithms, they all run on the same ordering engine and skip the work their answer does not need.

## Argsort

Sorting indices instead of moving wide records avoids copying payloads, but creates a new bottleneck: every key read becomes an indirect random lookup into the source slice — a cache miss on large inputs.

**The trick: pack key bits into the spare bits of each 64-bit index.** An index into 1 million records uses only 20 bits, leaving **44 unused bits** in a `u64`. Tessera fills those upper bits with the key's highest varying bits so sorting the packed words reads only the words themselves:

1. **Reuse root shortcuts:** already ordered, constant, single-tile, and [sparse source order](routing.md#sparse-source-order) inputs finish immediately.
2. **Coarse split:** one 8-bit counting pass distributes indices into 256 bands in input order.
3. **Pack & sort integers:** in dense bands (> 256 average entries), pack the next key bits above the index and sort the words as plain integers (the low index bits break ties in ascending source order, keeping stability automatically!).
4. **Resolve remaining ties only:** touch the source slice again only for words whose packed key prefix tied while lower key bits remain unresolved.

On 1 million random `u32` keys, the 8-bit band plus packed prefix resolves the top 28 of 32 bits — the source slice is touched again only when two keys share all 28 top bits. Sparse bands (averaging ≤ 256 entries) skip packing and sort directly by source keys.

### Parallel `par_argsort` rules

| Rule | Value / behavior |
|---|---|
| Serial fallback gate | Falls back to serial `argsort` in a 1-thread pool, below 4 index tiles, or when `len * max(size_of::<T>(), size_of::<usize>()) < 1 MiB` (sizing by the wider of records and output indices lets 131,072 `u32` keys clear 1 MiB via their 64-bit indices while keeping short wide-record slices serial). |
| Parallel packing & refinement | Packs top varying key bits directly into each index word in parallel, sorts through the shared parallel root, and unmasks indices. If all varying bits fit in the prefix, no tie ever reads the source again. |
| Bounded tie-scan waves | Wide ties unmask and scan remaining varying bits in one fused parallel pass: probes the first 64 slots serially (matching serial early exit), then scans in waves of `P * 8,192` slots; once all candidate bits vary, remaining tails only unmask. This bounds extra key reads past the serial early exit to at most `P * 8,192 - 1` regardless of scheduling. |

## Select one rank

`select_nth` needs the exact record at one zero-based rank (matching a full stable sort, even across equal-key ties) and the right elements on each side, without ordering either side internally.

**The trick: refine only the one bucket containing the target rank.** After each partition, every bucket completes its stable movement so both sides preserve relative equal-key order, and then Tessera discards every bucket that does not contain the requested rank (dropping ~99% of the slice after pass 1). A tile containing the rank finishes completely.

Before partitioning a multi-tile slice, selection checks four cheap shortcuts around the key $p$ currently sitting at the target rank:

- **Already a boundary:** if no key before the rank exceeds $p$ and none after is below $p$, done. When every varying bit of $p$ is all `1`s or all `0`s (ignoring constant padding on tuples, byte arrays, and `Reverse`), that side is certified without scanning it; any key below that varying maximum still scans the prefix.
- **One side clean:** if no key after the rank is below $p$ (or symmetrically none before exceeds $p$), scan forward from the first misplaced key before the rank, taking keys ≤ $p$ in input order until the prefix is full — any key pulled from beyond the rank equals $p$, preserving stability.
- **First or last rank:** find the first minimum (or last maximum); swap directly if the displaced key has no tie elsewhere, otherwise rotate the winner into place to keep equal keys ordered.
- **Short end & dominant key:** ranks close to either end collect candidates in a batch within the [sparse-order budget](routing.md#sparse-source-order), keeping the better half each time it fills (moving only winners and displaced end slots on wide records when varying bits prove no outside tie, otherwise falling back to stable run-and-exception placement). With one dominant key, a rank inside the dominant band only reserves that slot, and a rank outside sorts only the bounded exceptions.

## Sort by a cached key

When a key closure is expensive or must run at most once per record, `sort_by_cached_key` calls it once per record in input order, stores the encoded keys ($O(n)$ cache, matching `slice::sort_by_cached_key`), and learns in that same pass whether keys are already ascending and which bits vary:

- **Already ascending:** return immediately.
- **Narrow variation (≤ 12 varying bits, e.g., categories):** run one stable distribution pass replaying cached keys in input order without calling the closure again (one counting pass for a tile, or one [block partition](block-partition.md) with no recursion for larger inputs).
- **Wider keys:** `argsort` the cached keys with prefix packing, free the key cache, and permute the records in linear time (run by run if sparse, otherwise by cycles).
