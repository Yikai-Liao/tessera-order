# 1. Radix sort in one pass

**The core trade-off:** comparison sorts take $O(n \log n)$ comparisons, LSD radix sort runs in linear time and stays stable but needs a full $O(n)$ second buffer (**76 MiB** for 10 million 8-byte records), and in-place MSD drops the buffer at the cost of breaking stability. Tessera breaks that trade-off: one stable $O(\sqrt{n})$ split on the top digit, then ordinary cache-resident passes inside each small bucket.

## The counting pass

A radix sort never compares two keys. Instead it reads one **digit** (a fixed window of key bits) and uses that digit directly as a bucket index. One counting pass groups all $n$ records by a single digit in three steps:

1. **Count.** Scan the input once and count how many records fall into each digit value.
2. **Offsets.** Take the prefix sum of those counts so each bucket knows where its contiguous slice starts in the output.
3. **Scatter.** Read the input left to right and copy each record into the next free slot of its bucket.

<CountingPassDemo />

Reading the input left to right fills every bucket in input order, so records with the same digit never cross each other — the pass is **stable**. That stability is what lets multi-digit radix sort work.

Notice the catch when you hover any record above: its destination slot in the output usually still holds an unread record in the input. That is why a textbook counting pass needs a separate $n$-record output buffer. Tessera uses this exact counting pass (`counting_pass` in [`tile.rs`](../../rust/tessera-order/src/engine/tile.rs)) only inside small cache-sized [tiles](tiles.md), where the buffer is 64 KiB instead of 76 MiB.

## Four ways to sort multiple digits

A multi-digit key (such as a 32-bit integer split into four 8-bit bytes) can be processed low-to-high or high-to-low. Each choice trades stability, memory, and cache locality differently:

- **LSD (least significant digit first):** runs one stable counting pass per digit from lowest to highest. Stability preserves the lower-digit order inside every higher-digit group. Simple and branch-free, but every pass streams through all $n$ records and requires an $n$-record output buffer.
- **In-place MSD (American flag sort):** splits on the highest digit first using in-array swap cycles, then recurses into each bucket. Needs almost no extra memory and stops early on small buckets, **but swaps jump records past unread equal keys, breaking stability**.
- **Block LSD:** Clausecker and Schintke's technique ([*Parallel O(√n) Overhead LSD Radix Sort*](https://arxiv.org/abs/2607.05302), named Block LSD here to avoid confusion with the unrelated Rust crate `radsort`). It keeps LSD's stability while replacing the $n$-record buffer with $O(\sqrt{n})$ recycled blocks, though every pass still moves all $n$ records.
- **Tessera:** splits large inputs on the **most significant** digit using a single stable [block partition](block-partition.md) in $O(\sqrt{n})$ scratch, then finishes each resulting bucket once it fits in L2 cache using insertion sort, LSD, or tile-local MSD.

<RadixComparison />

## How much memory O(√n) saves

| | LSD | In-place MSD | Block LSD | Tessera |
|---|---|---|---|---|
| **Stable** | Yes | **No** | Yes | Yes |
| **Extra memory** | $O(n)$ buffer | $O(R \cdot \text{depth})$ | $O(\sqrt{n})$ | $O(\sqrt{n})$ + one 64 KiB tile |
| **Digit order** | Low → high | High → low | Low → high | High split, then cache tiles |
| **Adapts to data** | No | Small buckets only | No | Probes order, drops constant bits, isolates ties, picks tile leaf |

For a top-digit split into $R = 256$ buckets on a 64-bit machine (derived in [choosing the block size](block-partition.md#choosing-the-block-size)), the difference grows dramatically with input size:

| Records ($n$) | 8-byte records: Tessera | 8-byte records: LSD buffer | 16-byte records: Tessera | 16-byte records: LSD buffer |
|---:|---:|---:|---:|---:|
| $10^6$ | 1.9 MiB | 7.6 MiB | 2.7 MiB | 15 MiB |
| $10^7$ | 5.9 MiB | 76 MiB | 8.4 MiB | 153 MiB |
| $10^8$ | 19 MiB | 763 MiB | 26 MiB | 1.5 GiB |
| $10^9$ | 59 MiB | 7.5 GiB | 84 MiB | 15 GiB |

Tile scratch (at most 64 KiB of records plus cached bucket IDs) sits on top of these numbers; see [memory](index.md#memory). Next: [2. Keys as unsigned integers](keys.md) shows how signed integers, floats, tuples, and `Reverse` become radix keys and how constant bits disappear for free.
