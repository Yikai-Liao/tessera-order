# 4. The block partition

The block partition is what lets Tessera split a large node stably without an n-record buffer. It is adapted from Robert Clausecker's `radixsort_permuted.c`, described in [*Parallel O(√n) Overhead LSD Radix Sort*](https://arxiv.org/abs/2607.05302) by Clausecker and Schintke, and lives in [`block.rs`](../../rust/tessera-sort/src/block.rs). Tessera runs one pass of it per node, on the node's top digit.

## The idea

A counting pass needs an output buffer because each bucket must be **contiguous** while it is written. The block partition drops that requirement during the pass:

- Cut memory into **blocks** of b records. A bucket is written as a chain of blocks. Each bucket fills one block at a time, and when the block is full, the bucket takes another one anywhere in memory.
- A small **block table** records the logical order of the blocks, so that reading the table front to back visits bucket 0's blocks, then bucket 1's, and so on.
- The blocks a bucket takes are **input blocks that have already been read**. The input is consumed front to back, so freed blocks appear as fast as buckets need them.

After the pass, one **compaction** walk copies the blocks into the input slice in table order. The result is the stable partition.

The extra memory is 2R scratch blocks and the table: O(√n) when b is chosen well (see [below](#choosing-the-block-size)).

## Step by step

The figure runs the algorithm on a small input: R = 4 buckets on the tens digit, blocks of 3 records. It is a TypeScript model of `Partition::new`, `scatter`, `finish_pass` and `compact_with` with the same index arithmetic, tested against a reference stable sort on 4,248 configurations. Every frame holds each record exactly once.

<BlockPartitionDemo />

How to read the figure:

- **Physical blocks.** `S0`–`S7` are the 2R scratch blocks and `I0`, `I1`, … are the full blocks of the input slice. `L5` under a name is the block's position in the logical order.
- **Cells.** A coloured cell is a live record: its key, and a letter for its input position. A grey cell is a stale copy: bytes left behind after the record moved. Stale bytes are never read as a record again.
- **Block table.** The bottom row is the table itself: logical position → physical block. A block's fill colour is the bucket that filled it.

## The four phases

### 1. Set up

Physical blocks are numbered with the 2R scratch blocks first, then the full input blocks. The input usually ends in a partial block, the *tail*. The tail is not a full physical block, so it is copied into scratch block R. The initial table reads:

| Logical positions | Contents |
|---|---|
| 0 … R−1 | free scratch blocks `S0` … `S(R−1)`, one output block per bucket |
| R … R+F−1 | the F full input blocks, in order |
| R+F | the tail, now in `S(R)` |
| R+F+1 … | the remaining free scratch blocks |

The tail block is recorded as a *partial block*: a block whose prefix of `len < b` records is live. All other active blocks are full.

### 2. Scatter

Each bucket starts with one of the R free blocks. The input is read in table order, from logical position R. Each record is classified by its digit and copied to its bucket's next free slot. When a bucket's block fills up, the bucket takes the block at the next **output position** in the table, which starts at R and only grows.

Why that block is always free: a bucket takes its k-th new block only when k blocks have been filled, so at least k·b records have been read. Every input block holds at most b records, so the first k input blocks (logical positions R … R+k−1) have been read completely. The output position never overtakes the read position, so a block is reused only after every record in it has been read.

Each record is written into its bucket's current block in input order, so the order inside a bucket is the input order. That is where stability comes from.

### 3. Rebuild the block table

After the scatter, every record has moved once, and only the table changes. `finish_pass` reads the bucket of every filled block and writes a new table that lists bucket 0's blocks in the order they were filled, then bucket 1's, and so on. Each bucket's current block is only partly filled, so it is recorded as that bucket's partial block. Read front to back, the table now *is* the stable partition, just not contiguous in memory.

The table costs 10 bytes per block position: two `u32` block orders (the current and the next one) and one `u16` bucket ID.

### 4. Compact

`compact_with` copies the logical sequence back into the input slice. It walks the physical input blocks in order. For each one:

1. If the block still holds a later part of the result, that block is first moved to the free block, so it is not overwritten.
2. The next logical block's live records are copied to the next free position in the input. Partial blocks are packed together, so the result has no gaps.

The table is kept up to date as blocks move, and an inverse table (physical block → logical position) answers "where is this block now?" in O(1). The last few logical blocks, which live in scratch, are copied last.

A bucket is contiguous in the input as soon as its last record has been copied. The engine passes a callback that **finishes each tile-sized bucket right then**, while its records are still in cache.

## Parallel roots

The optional Rayon path uses the serial engine below 1 MiB or in a one-thread pool. A larger root scatters disjoint input shards independently, requesting at most the pool width and about one shard per 256 KiB. Actual sharding is also clamped by the number of complete blocks. Its directory reads bucket 0 from the first shard, then bucket 0 from the second, and so on before bucket 1, preserving input order whichever worker finishes first.

Block storage owns its size independently of how children sort. With q requested shards, fan-out R and record size s, the initial balance charges both local and adopted directories: `ceil(sqrt(20n / (2qRs)))` records. As in serial compaction, the shared factor of four amortizes per-source ownership work, with the existing byte and record floors. The shard clamp then uses that block size. Enlarging a child's tile allowance does not change the root block layout.

A root with a frequent sampled key uses the same sharded scatter and final-band completion, with the [equality digit](routing.md#repeated-key) as its classification. The equal bucket's callback returns at once; see the [execution contract](../design/engine.md#parallel-root-completion).

Each movement task transfers one bucket's prefixes directly into its final band in directory order. Before copying, it protects the complete destination interval by relocating unread sources, then resolves the source's possibly changed location. Once the band is complete, its callback may sort it while peers continue moving their disjoint records. The [whole-child tile allowance](tiles.md#parallel-child-allowance) does not depend on a staging buffer, root size window or an all-buckets-fit test. Locking, recovery and suspended scratch accounting belong to the [engine reference](../design/engine.md#parallel-root-completion).

The tested [parallel finishing model](../.vitepress/theme/sim/parallelFinish.ts) adopts scattered frames from the serial model and replays protection, relocation and direct final writes in different bucket orders. It checks stable source order and one live owner after each action, counts direct and evacuation transfers, and bounds evacuation by the initialized prefixes of protected input blocks. It models serial linearizations, not mutex scheduling, panic recovery or Rust pointer provenance.

## Why 2R scratch blocks

- **R blocks** give every bucket an empty block before the first input block has been read. Nothing has been freed yet at that point.
- **R more blocks** absorb the slack. When the pass ends, every bucket's last block is only partly filled, so the result occupies up to R blocks more than the input did. The input's tail, which does not fill a block, starts in this reserve.

The total is `F + 2R` physical blocks for F full input blocks, and the scatter can never run out.

## Choosing the block size

The block size b trades scratch against tables. For n records of s bytes and R buckets:

$$
\text{memory}(b) = \underbrace{2 R\, b\, s}_{\text{scratch blocks}} + \underbrace{\frac{10\, n}{b}}_{\text{block tables}}
$$

Larger blocks need more scratch, and smaller blocks need more table entries. The sum is smallest where the two terms are equal:

$$
b^{*} = \sqrt{\frac{10\, n}{2 R s}}, \qquad \text{memory}(b^{*}) = 2\sqrt{20\, R\, s\, n} = O(\sqrt{n}).
$$

With b = c·b\*, the total is (c + 1/c)/2 times the minimum, which grows on both sides. Minimum memory is not minimum time, though: compaction reads every block at a random address, and that read does not shrink with the block. So the formula takes **4·b\*** (rounded up), which costs 2.1 times the minimum memory, with two floors:

- at least **1,024 bytes** of records per block, because compaction moves a block with one copy, and a smaller copy costs more than it saves;
- at least **32 records** per block, because a block's bookkeeping (its table entries and one copy call) costs the same whatever the record width.

Neither floor depends on n, so the bound stays O(√n). The engine computes `max(1, 4 * ceil(b*), floor(1024 / s), 32)` in `default_block_size`.

<BlockSizeChart />

Move n and watch the minimum: at small n the 1,024-byte floor decides b, and the scratch term dominates. From a few million records on, b\* takes over, and both terms grow as √n.

## What the port adds to the C reference

| C reference | Tessera |
|---|---|
| sorts integer pairs, so scratch can be zero-initialized | any record type: scratch is `MaybeUninit<T>`, and only the live prefix of a block is ever read |
| all digits, LSD | one pass per node, on the top digit, plus the compaction callback for tiles |
| scatters every digit | the engine has already removed [constant bits](keys.md#varying-bits) |
| no unwinding | on a key-function panic, the pass is completed without calling the key function again, the records are compacted back into the input, and the panic resumes ([ownership and panics](safety.md)) |

## Invariants

These hold between passes in `block.rs`, where debug builds check the first two in `Partition::validate`. The model also checks that every intermediate frame holds each record exactly once.

1. The block table is a permutation of the physical blocks.
2. The active range of the table, read in order with each partial block's length, holds every record exactly once.
3. A block becomes an output block only after every record in it has been read.
4. Inside each bucket, records appear in input order.
