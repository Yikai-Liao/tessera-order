# 4. The block partition

**The core idea:** a normal counting pass needs an $O(n)$ output buffer because each bucket writes into a fixed contiguous slice that still holds unread input. Tessera drops that requirement by cutting memory into fixed-size **blocks**: each bucket writes into a linked chain of blocks, and as the input is read left to right, **every finished input block immediately becomes free output space** for whichever bucket fills next.

This mechanism is adapted from Robert Clausecker's `radixsort_permuted.c` ([*Parallel O(√n) Overhead LSD Radix Sort*](https://arxiv.org/abs/2607.05302), Clausecker & Schintke), running one pass per node on the digit or equality classification chosen by the [planner](routing.md#nodes-and-tiles).

## Step by step

Explore the real Rust engine compiled to WebAssembly below. Drag the **Read frontier** across the input during scatter to watch read input blocks turn green and get recycled by filling buckets, hover any bucket row to trace its physical blocks, then switch to **Write back** to see the chains move into their final contiguous ranges:

<BlockPartitionDemo />

Every grey cell is a **stale copy** left behind after its record moved — stale bytes are never read or dropped again. Only the live prefix of each block holds active records (checked on every frame by [`blockPartition.ts`](../.vitepress/theme/sim/blockPartition.ts) against the engine's real output).

## Scatter

Each of the $R$ buckets starts its chain in its own scratch block (`S0`…`S(R−1)`), while `SR`…`S(2R−1)` stay empty as the completion reserve. The engine reads the input strictly front to back, appending each record to its bucket's current block. Whenever a bucket's block fills, it links the next input block (`I0`, then `I1`, …) in a side link array and keeps writing there.

**Why that input block is guaranteed already read:** a bucket can claim the $k$-th input block only after $k$ blocks have filled across all buckets, which requires reading at least $k \cdot b$ input records — meaning the first $k$ input blocks (`I0`…`I(k−1)`) have already been read completely. Because records enter each bucket's chain in input order, every record moves once and the scatter is **stable**.

## Completion

After scatter, the **completion ledger** turns the linked chains into a flat list of nonempty block prefixes (**sources**) and computes each bucket's **final band** in the input array. Every physical block without a live source is marked **free**. Each bucket then writes its sources in chain order into its final band:

1. **Protect.** Before writing into destination blocks, any unread source sitting in those blocks is evacuated to a free block, marking the destination block *final*.
2. **Copy.** The source prefix is copied into its final band and its old block becomes free (if the source's own block overlaps its destination, one overlap-safe move places it and marks that block final).

**Why completion never runs out of free blocks:** each source holds at most one block of records, so after $g$ copies a bucket has touched at most $g + 2$ destination blocks. The $R$ reserve blocks always cover that slack. In serial execution, any completed band small enough for a [tile](tiles.md) is finished immediately while still cache-hot.

## Parallel roots

With the `rayon` feature, large roots divide the input into disjoint shards, each scattering into its own $2R$ scratch blocks in parallel. One shared ledger then joins the chains bucket by bucket in original shard order — guaranteeing identical **stability** regardless of thread scheduling. Each bucket completes and sorts its own final band independently while peers keep moving disjoint records. Equality-partitioned roots reuse the exact same sharded scatter and completion, skipping the finished equal band. Locking, recovery, and deadlock-freedom proofs live in the [engine reference](../design/engine.md#parallel-root-completion).

## Choosing the block size

Choosing block size $b$ balances two competing costs for $n$ records of $s$ bytes, $R$ buckets, and $d$ bytes of per-block ledger metadata (`finish::serial_descriptor_bytes`):

$$
\text{memory}(b) = \underbrace{2 R\, b\, s}_{\text{scratch blocks}} + \underbrace{\frac{d\, n}{b}}_{\text{ledger}}
$$

The sum is minimized where both terms are equal:

$$
b^{*} = \sqrt{\frac{d\, n}{2 R s}}, \qquad \text{memory}(b^{*}) = 2\sqrt{2 d\, R\, s\, n} = O(\sqrt{n}).
$$

<BlockSizeChart />

Because smaller blocks add per-block copy and lock overhead, `block_size` uses **$2 \cdot b^{*}$** (rounded up) — only $1.25\times$ minimum memory while cutting per-block overhead in half. At small $n$, fixed floors dominate; above a few million records, $b^*$ takes over and both terms grow as $\sqrt{n}$.

## What the port adds to the C reference

| Clausecker's C Block LSD | Tessera |
|---|---|
| Integer pairs (zero-initialized scratch) | Arbitrary Rust types (`MaybeUninit<T>`, only live prefixes read) |
| Full LSD over all digits | Single-pass MSD / equality split per node with per-band callbacks |
| Scatters every digit | Skips [constant bits](keys.md#varying-bits) before partitioning |
| No unwinding | On panic, appends unread suffix to bucket 0 without calling the key closure, completes all bands, and resumes unwinding ([6. Ownership and panics](safety.md)) |

The paper's future work (Section 7) notes that naive MSD recursion accumulates partial blocks across levels. Tessera avoids that completely by running completion at the end of every node pass: every bucket becomes a contiguous slice of the input before recursing, so no partial block ever outlives its pass.

## Exact rules & thresholds

| Rule | Value / contract |
|---|---|
| Block-size scale | `2 * b*` (halved from `4 * b*` in [#202](https://github.com/Yikai-Liao/tessera-order/pull/202), cutting scratch ~40% at ≥1M records for 2–5% runtime on large random inputs, up to 8.8% on `{u64, u32}`). |
| Block-size floors | At least **1,024 bytes** and at least **32 records** per block (independent of $n$, keeping $O(\sqrt{n})$ asymptotics; a 12-bit digit reserves 8 MiB, which is why [routing](routing.md) widens only over varying bits). |
| Parallel root gate | Uses serial engine below **1 MiB** of records or in a 1-thread pool; above that, spawns 1 shard per **256 KiB** up to pool width, clamped to full input blocks. Parallel block sizing uses $b^* = \sqrt{dn / (2qRs)}$ for $q$ shards with the same scale and floors. |

## Invariants

Debug builds assert the first two in `Partition::validate` after scatter; the replay tests verify all five on every frame:

1. Each physical block belongs to at most one bucket chain.
2. The chains, read with their prefix lengths, contain every input record exactly once.
3. No input block becomes an output block until every record in it has been read.
4. Within every bucket, records stay in original input order.
5. Every destination block is marked *final* before any copy writes into it, so no unread source is ever overwritten.

Next: [5. Finishing tiles](tiles.md) explains how cache-resident buckets finish quickly inside a 64 KiB buffer.
