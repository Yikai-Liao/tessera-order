# 7. Beyond sorting

Argsort, selection and cached-key sorting each ask for less than a sorted slice. They run on the same engine and skip exactly the work their answer does not need.

## Argsort

**Problem.** Argsort sorts indices, not records. Every key read becomes an indirect lookup into the source, a likely cache miss on a large input.

**Idea: pack key bits into the index.** An index into a million records needs 20 bits; a 64-bit word has 44 more. Tessera fills them with key bits, so most of the sorting reads only the words themselves.

1. The input is first routed like a sort: ordered inputs, constant keys, inputs that fit a tile and [sparse source order](routing.md#sparse-source-order) finish there.
2. One counting pass on the top 8 varying key bits writes each source index into one of 256 bands, in input order.
3. Within each dense band, every word holds the next key bits above the index. Sorting the words as plain integers orders them by those key bits and then by source position, which keeps the order stable.
4. Only words whose packed bits tie, with key bits still unresolved, look up their source keys again.

For a million random `u32` keys, the band and the packed prefix resolve the top 28 of 32 bits; the source is read again only to separate words that agree on all 28. When bands average at most 256 entries, packing does not pay for itself, and each band is sorted directly by source keys.

With the `rayon` feature, `par_argsort` and `par_argsort_by_key` fall back to serial argsort in a one-thread pool, when the input has fewer than four index tiles, or when `len * max(size_of::<T>(), size_of::<usize>())` is below 1 MiB (the byte gate uses the wider of records and output indices, so 131,072 `u32` keys clear 1 MiB via their 64-bit indices while a wide-record slice with fewer than four index tiles stays serial). Eligible inputs pack the top varying key prefix directly into the spare bits of each index word in parallel, sort those words through the shared parallel root, and unmask the indices. When all varying bits fit in the prefix, no tie reads the source again; otherwise, prefix-aligned slices refine small ties in parallel and ties wider than a worker share unmask and scan their remaining varying bits in one parallel pass before refining any still-varying bits through the parallel root. During that fused pass, each of at most `P` chunks merges its observed 1-bits and 0-bits into shared monotonic bitmasks at entry and at bounded batch intervals (64 growing to 4,096); once every remaining candidate bit has been seen as both `1` and `0` globally (within one chunk or across chunks), active workers stop reading keys at the end of their current batch—reading at most 4,096 extra keys per peer worker past global saturation—while still unmasking every index.

## Select one rank

**Problem.** `select_nth` needs one record in its place and the right records on each side, not the whole order.

**Idea: refine only the band that holds the rank.** Selection runs the same node and tile decisions as a sort. After each partition, every band completes its stable movement, so both sides hold the right records in a stable order, but only the band containing the rank is analyzed further. A tile that contains the rank is still sorted completely.

Before partitioning an input larger than a tile, selection tries shortcuts that may avoid partitioning altogether. Let *p* be the key currently at the rank.

- **Already a boundary.** If no record before the rank has a key above *p* and none after it has a key below *p*, the rank is already correct. When every varying bit of *p* (from its [key encoding](keys.md)) is `1` or `0`, no valid key can exceed or precede *p*, so that side is certified without scanning it — including padded tuples, byte arrays and reversed narrow keys whose unused bits are constant; any key below that varying maximum still scans the prefix.
- **One side clean.** Suppose no key after the rank is below *p*. Then the prefix through the rank must consist of the smallest records, all at most *p*. Starting at the first record before the rank whose key exceeds *p*, take records with keys at most *p* in input order until the prefix is full; the skipped records follow. Any record taken from beyond the rank has key exactly *p*, so taking them in input order keeps the stable choice. The mirrored rule handles a clean prefix.
- **First or last rank.** Find the first minimum (or last maximum). If the displaced record has no equal key elsewhere, swap the two; otherwise rotate the winner into place so the records in between keep their order.
- **A short end.** If the rank lies close to one end, collect that end's candidates in a batch. Whenever the batch fills, keep its better half. For wide records, the engine moves only the winners and displaced end slots when it proves no outside key ties the original end; it checks only varying bits, so an end whose keys share no varying bit (ignoring constant padding) falls back directly to the stable run-and-exception placement instead of running tie lookups. The batch stays within the [sparse-order budget](routing.md#sparse-source-order), and the end is placed with the same run-and-exception movement.

With one dominant key, a rank inside the dominant key's band needs only that position reserved; a rank outside it sorts only the bounded exceptions.

## Sort by a cached key

**Problem.** A costly key function, or one that must not run twice, cannot be called during every pass.

**Idea: cache once, then decide from the cached keys.** `sort_by_cached_key` calls the key function once per record, in input order, and stores the encoded keys. The same pass learns whether the keys ascend and which bits vary.

- **Already ascending:** done, with no further work.
- **Keys vary within 12 bits**, as with a small category ID: one stable distribution moves each record once into its final bucket. It never calls the key function: it replays the cached keys in input order. A tile uses a single counting pass; a larger input uses one [block partition](block-partition.md) without recursive finishing.
- **Otherwise:** argsort the cached keys, with the packing above, and release them. Then move the records along the permutation: run by run if the order was sparse, otherwise by following its cycles, in linear time.

The cache costs one key per record, the same O(n) as the standard library's cached-key sort.
