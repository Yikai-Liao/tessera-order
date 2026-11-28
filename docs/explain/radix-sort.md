# 1. Radix sort in one pass

**The core trade-off:** comparison sorts take $O(n \log n)$ comparisons, LSD radix sort runs in linear time and stays stable but needs a full $O(n)$ second buffer (**76 MiB** for 10 million 8-byte records), and in-place MSD drops the buffer at the cost of breaking stability. Tessera breaks that trade-off: one stable $O(\sqrt{n})$ split on the top digit, then ordinary cache-resident passes inside each small bucket.

## The counting pass

A radix sort never compares two keys. Instead it reads one **digit** (a fixed window of key bits) and uses that digit directly as a bucket index. One counting pass groups all $n$ records by a single digit in three steps:

1. **Count.** Scan the input once and count how many records fall into each digit value.
2. **Offsets.** Take the prefix sum of those counts so each bucket knows where its contiguous slice starts in the output.
3. **Scatter.** Read the input left to right and copy each record into the next free slot of its bucket.

<CountingPassDemo />

Take the twelve keys `[31, 12, 7, 25, 14, 38, 3, 19, 22, 11, 30, 16]` grouped by their tens digit (`0–9`, `10–19`, `20–29`, `30–39`). Counting finds 2, 5, 2, and 3 records per bucket, so prefix sums reserve output ranges `0–1`, `2–6`, `7–8`, and `9–11`. Reading the input left to right fills every bucket in input order, so equal digits never cross each other—the pass is **stable**, which is what lets multi-digit radix sort work.

Notice why an in-place scatter breaks: moving key `31` from slot 0 to output slot 9 would overwrite unread key `11`, and moving key `12` from slot 1 forward to output slot 2 would overwrite unread key `7`. Even though a backward move (such as key `7` from slot 2 to slot 0) writes into a slot already read, forward moves in the same pass collide with unread input, so a textbook counting pass needs a separate $n$-record output buffer. Tessera runs this exact counting pass only inside small cache-sized [tiles](tiles.md), where the buffer is 64 KiB instead of 76 MiB (see the [engine reference](../design/engine.md) for the code map).

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

For a modeled serial top-digit split into $R = 256$ buckets on a 64-bit target (derived in [choosing the block size](block-partition.md#choosing-the-block-size), before the separate tile buffer), the gap grows rapidly with input size:

| Records ($n$) | 8-byte records: Tessera partition | 8-byte records: LSD buffer | 16-byte records: Tessera partition | 16-byte records: LSD buffer |
|---:|---:|---:|---:|---:|
| $10^6$ | 1.1 MiB | 7.6 MiB | 1.6 MiB | 15 MiB |
| $10^7$ | 3.5 MiB | 76 MiB | 4.9 MiB | 153 MiB |
| $10^8$ | 11 MiB | 763 MiB | 16 MiB | 1.5 GiB |
| $10^9$ | 35 MiB | 7.5 GiB | 49 MiB | 15 GiB |

Tile scratch (at most 64 KiB of records plus cached bucket IDs) sits on top of these numbers; see [memory](index.md#memory). Next: [2. Keys as unsigned integers](keys.md) shows how signed integers, floats, tuples, and `Reverse` become radix keys and how constant bits disappear for free.
