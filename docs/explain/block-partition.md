# 4. The block partition

The block partition splits a large node stably without an n-record buffer. It is adapted from Robert Clausecker's `radixsort_permuted.c`, described in [*Parallel O(√n) Overhead LSD Radix Sort*](https://arxiv.org/abs/2607.05302) by Clausecker and Schintke. Tessera runs one pass per node, on the digit or equality classification the [planner](routing.md#nodes-and-tiles) chose. Scatter lives in [`block.rs`](../../rust/tessera-order/src/block.rs), completion in [`block/finish.rs`](../../rust/tessera-order/src/block/finish.rs).

## The idea

A counting pass needs an output buffer because each bucket must be contiguous while it is written. The block partition drops that requirement during the pass:

- Memory is cut into **blocks** of b records. Each bucket writes a **chain** of blocks, one block at a time.
- When a bucket's block fills, the bucket links the next **input block that has already been read**. The input is read front to back, so read blocks appear as fast as buckets need them.
- After the pass, a **completion ledger** copies each bucket's chain into its final range of the input, moving any block that is still needed out of the way first.

The extra memory is 2R scratch blocks plus per-block metadata, O(√n) when b is chosen well (see [below](#choosing-the-block-size)).

## Step by step

A bucket is not an array of its own. Each record's bucket is one digit of its key, and a pass with a d-bit digit has R = 2^d buckets, numbered by the digit's value; the [planner](routing.md#nodes-and-tiles) chooses d for each node. A bucket is a singly linked list of fixed-size blocks: its records sit in the blocks it has filled, in input order. The head is the bucket's scratch block, each full block's link names the next block, and the bucket keeps a write cursor in its last block. The links live in one side array indexed by physical block, not inside the blocks, so a block holds only records. After the pass the ledger records which blocks each list holds.

The figure runs the engine's own serial block partition, compiled to WebAssembly, on two-digit keys 0–39. The digit is the tens place, so R = 4 and a key's tens digit is its bucket. It calls the partition directly, past the [routing](routing.md#budgeted-insertion) that would send an input this short to insertion sort, and its blocks hold 2–4 records instead of the [derived size](#choosing-the-block-size). The engine reports each step it takes; [`blockPartition.ts`](../.vitepress/theme/sim/blockPartition.ts) only draws them. Its tests check that the replay ends in the engine's stable order and that every frame holds each record exactly once.

<BlockPartitionDemo />

How to read the figure:

- **Rows.** The top row is the input slice with each position's index. `I0`, `I1`, … are its full blocks; the last, partial block is the *tail*, which is read but never becomes an output block. `S0`–`S3` are the first block of each bucket and `S4`–`S7` the reserve that completion needs: 2R scratch blocks in all. The bottom rows show each bucket's chain, the same records as above in chain order. Hovering a block marks it in both views.
- **Cells.** A coloured cell is a live record: its key, with the tens digit in bold, and its input position below. A grey cell is a stale copy left behind after the record moved. Stale bytes are never read as a record again.
- **Labels.** Each tinted box is one block of b slots. During scatter, `k writes` marks bucket k's last block, the one it is filling, and `next X` under a full block is its link to the next block of the same bucket. From the ledger on, a block is a `source` (coloured by its bucket), `free`, or `final` (no unread source remains in it), and coloured bars under the input mark each bucket's final band, whose positions the bucket's row lists.
- **Steps.** The phase buttons jump to scatter, the ledger or completion; Play steps on its own at the chosen speed. Clicking an input position jumps to the step that reads it, and clicking a block in a bucket row during completion jumps to the step that copies it.

## Scatter

Each bucket k starts its chain in scratch block `Sk`; blocks `SR`…`S(2R−1)` stay free for completion. The input is read front to back. Each record is classified and copied to the next slot of its bucket's current block. When the block fills, the bucket links the next input block in order (`I0`, then `I1`, …) and continues there.

Why that block has always been read: a bucket links the k-th input block only when k blocks have filled, so at least k·b records have been read, and the first k input blocks are therefore read completely. Records enter their bucket's chain in input order, which makes the partition stable. Every record moves once.

## Completion

The ledger turns the chains into a flat list of **sources**, one per nonempty block prefix, bucket by bucket. Bucket k's **final band** starts where bucket k−1's ends. Every block that holds no source is free. Then each bucket copies its sources, in chain order, into its band:

1. **Protect.** Every full input block the next copy will overwrite is made safe first. A free block becomes final. A block still holding another source has that source moved into a free block, then becomes final.
2. **Copy.** The source is copied into the band and its block becomes free. If the source's own block lies inside its destination, it is skipped by protection and copied with one overlapping move, after which the block is final.

Once a bucket's sources are all copied, its band is contiguous and the callback receives it. A serial partition finishes a tile-sized band right then, while its records are still in cache; larger bands are routed after the partition returns.

Each copy writes at most a block of records, so after g copies a bucket has touched at most g + 2 destination blocks. The R free scratch blocks cover that slack: protection never runs out of free blocks.

## Parallel roots

The optional Rayon path uses the serial engine below 1 MiB of records or in a one-thread pool. A larger root scatters disjoint input shards independently, one per 256 KiB of records up to the pool width, clamped to the number of full blocks. Each shard has its own 2R scratch blocks. One ledger joins the chains bucket by bucket in original shard order, so the result is stable whichever worker finishes first.

Each bucket completes as an independent task: it protects and copies its own sources, then finishes its band while peers keep moving disjoint records. A root with a frequent sampled key uses the same sharded scatter and completion with the [equality classification](routing.md#repeated-key); the complete equal band skips its callback. The figure shows the serial path; the Rust parallel tests cover sharded ledgers and every bucket order. Locking, recovery and the progress argument belong to the [engine reference](../design/engine.md#parallel-root-completion).

## Choosing the block size

The block size b trades scratch against metadata. Each full input block costs d bytes of ledger metadata (`finish::serial_descriptor_bytes`): its chain link, address, owner cell, source entry and free-cache slot. For n records of s bytes and R buckets:

$$
\text{memory}(b) = \underbrace{2 R\, b\, s}_{\text{scratch blocks}} + \underbrace{\frac{d\, n}{b}}_{\text{ledger}}
$$

The sum is smallest where the two terms are equal:

$$
b^{*} = \sqrt{\frac{d\, n}{2 R s}}, \qquad \text{memory}(b^{*}) = 2\sqrt{2 d\, R\, s\, n} = O(\sqrt{n}).
$$

With b = c·b\*, the total is (c + 1/c)/2 times the minimum. Minimum memory is not minimum time: every block costs an ownership check and a copy at a scattered address, whatever its size. `default_block_size` therefore takes **2·b\*** (rounded up), at 1.25 times the minimum memory, with two floors:

- at least **1,024 bytes** of records per block, so one copy is worth its overhead;
- at least **32 records** per block, because a block's bookkeeping costs the same whatever the record width.

Neither floor depends on n, so the bound stays O(√n). The floors' scratch grows with R instead: 2R·max(1024, 32s) bytes, so a 12-bit digit reserves 8 MiB at any n. [Routing](routing.md) therefore widens a digit only over bits some key varies in. A parallel root balances the same metadata against the scratch of all shards, b\* = √(dn / (2qRs)) for q shards, with the same scale and floors.

<BlockSizeChart />

The chart uses d for 64-bit targets. At small n the 1,024-byte floor decides b and scratch dominates; from a few million records on, b\* takes over and both terms grow as √n.

## What the port adds to the C reference

| C reference | Tessera |
|---|---|
| sorts integer pairs, so scratch can be zero-initialized | any record type: scratch is `MaybeUninit<T>`, and only a block's live prefix is ever read |
| all digits, LSD | one pass per node, on the top digit or an equality classification, with a callback for each finished band |
| scatters every digit | the engine has already removed [constant bits](keys.md#varying-bits) |
| no unwinding | on a key-function panic, scatter appends the unread suffix to bucket 0 without calling the key function again, the ledger completes every band, and the panic resumes ([ownership and panics](safety.md)) |

## Invariants

Debug builds check the first two in `Partition::validate` after scatter; the model checks every frame.

1. Each physical block appears in at most one chain.
2. The chains, read with their prefix lengths, hold every record exactly once.
3. A block becomes an output block only after every record in it has been read.
4. Inside each bucket, records appear in input order.
5. A destination block is final before any record is copied into it, so a copy never overwrites an unread source.
