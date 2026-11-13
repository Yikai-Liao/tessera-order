# 1. Radix sort in one pass

A comparison sort learns about the order by comparing two keys at a time, and it needs Ω(n log n) comparisons. A radix sort never compares keys. It reads a *digit*, a fixed window of key bits, and uses it as an array index. This page builds the textbook versions from one primitive, the counting pass, and compares them on the same input with Tessera.

## The counting pass

One counting pass groups records by one digit, in three steps:

1. **Histogram.** Count how many records have each digit value.
2. **Prefix sum.** Turn the counts into start positions: bucket *d* starts after all records with a smaller digit.
3. **Scatter.** Read the input front to back and write each record to the next free position of its bucket.

<CountingPassDemo />

Step 3 reads the input in order and fills each bucket in order. Records with the same digit therefore keep their relative order: the pass is **stable**. Stability is the property that makes multi-pass radix sorts work.

It also needs an output buffer as large as the input, because a record cannot be written to its bucket position until that position has been vacated, and nothing guarantees that it has. This buffer is what Tessera avoids.

In the engine, this is `counting_pass` in [`engine.rs`](../../rust/tessera-sort/src/engine.rs). It is used only inside tiles, where the buffer is small enough to stay in cache.

## Two ways to use more than one digit

A key has several digits. A 32-bit key read 8 bits at a time has four.

**LSD (least significant digit first)** runs one stable counting pass per digit, from the lowest digit to the highest. After the pass on digit *k*, the records are sorted by digits 0 to *k*. The pass on a higher digit regroups them, and stability keeps the order the lower digits established inside each group. Every pass moves all n records, so the cost is fixed: *digits × n* moves. Memory: an n-record buffer, reused by every pass.

**MSD (most significant digit first)** splits on the highest digit first, then sorts each bucket on its own by the next digit, recursively. It can stop early: a bucket with one record, or a bucket that fits a cheap insertion sort, needs no more digits. The textbook in-place version, **American flag sort**, permutes records inside the array with swap cycles. It needs only a counter per bucket. A swap moves a record past others that have not been examined yet, so equal keys can change order: **in-place MSD is not stable**. A stable MSD sort scatters into a buffer instead, which costs an n-record buffer again.

**Block LSD**, by Clausecker and Schintke, is an LSD sort without the n-record buffer. Each pass fills buckets block by block and reuses input blocks it has finished reading as output blocks. A small table records the logical order of the blocks. The extra memory is O(√n), and the records are written back into the input once, at the end. Tessera's [block partition](block-partition.md) adapts this scheme to one digit at a time, linking each bucket's blocks into a chain instead of a table.

## Side by side

All four sorts get the same twelve two-digit keys, with three repeated pairs. The letter under each key is its input position. A stable result lists equal keys in alphabetical order.

<RadixComparison />

What the figure shows:

- **LSD** and **Block LSD** produce the same sequence after every pass. They differ only in where the records wait in between: a full second buffer, or recycled blocks with a block table.
- **In-place MSD** groups by the tens digit first. Its swap cycles move the second `12` (*e*) in front of the first (*b*), and the second `25` (*i*) in front of the first (*d*). The result is sorted but not stable.
- **Tessera** splits on the tens digit like MSD, but with a stable block partition, so no equal keys swap. Each bucket then becomes a small, independent problem that fits in cache.

## Comparison

| | LSD | Stable MSD (buffered) | In-place MSD | Block LSD | Tessera |
|---|---|---|---|---|---|
| Stable | yes | yes | **no** | yes | yes |
| Extra memory | n records | n records | O(R × depth) counters | O(√n) | O(√n) + one tile |
| Digit order | low → high | high → low | high → low | low → high | high digit split, then tiles |
| Records moved | every pass moves all n | only buckets that still need sorting | only buckets that still need sorting | every pass moves all n | each split level moves its node once; tiles are sorted in cache |
| Adapts to data | no | stops on small buckets | stops on small buckets | no | probes root and child order, drops constant bits, partitions repeated keys, picks a leaf per tile |
| Cache behaviour on large n | each pass streams through n records twice | good once buckets are small | random swaps over the whole array | each pass streams through n records | one streaming split, then cache-resident tiles |

Tessera takes **stability and O(√n) memory** from Block LSD, the **top-digit-first split** and early stop from MSD, and uses **LSD counting passes** only inside tiles, where the second buffer is one tile.

## How much memory O(√n) is

For a top-digit split into R = 256 buckets on a 64-bit target, the block partition needs this much memory beyond the input. An LSD pass needs a second buffer of n records. The [block partition page](block-partition.md#choosing-the-block-size) derives the formula.

| n | 8-byte records: block partition | 8-byte records: LSD buffer | 16-byte records: block partition | 16-byte records: LSD buffer |
|---:|---:|---:|---:|---:|
| 10⁶ | 1.9 MiB | 7.6 MiB | 2.7 MiB | 15 MiB |
| 10⁷ | 5.9 MiB | 76 MiB | 8.4 MiB | 153 MiB |
| 10⁸ | 19 MiB | 763 MiB | 26 MiB | 1.5 GiB |
| 10⁹ | 59 MiB | 7.5 GiB | 84 MiB | 15 GiB |

The tile buffer and its cached bucket IDs come on top of these numbers; see [memory](index.md#memory).
