# Design

Tessera sorts by key bits. Each pass removes bits from the keys, so the recursion is a
multi-digit radix sort whose remaining work shrinks with every split. This page
describes the stages of one recursion and the reasons behind its thresholds;
the [semantics](../conformance/semantics.md) page owns the contracts.

## Vocabulary

| Term | Meaning |
|---|---|
| **record** | one element of the caller's slice, moved but never cloned or dropped |
| **key** | the encoded unsigned integer of a record, read through an *extractor* |
| **digit** | the window of key bits that one pass partitions by |
| **node** | a slice larger than the tile budget; it splits through the block engine |
| **tile** | a slice that fits the tile budget; it splits through counting scratch |
| **plan** | the one split a slice needs next: rotate, pivot or digit |
| **scratch** | the buffers one call owns and releases before returning |

Names describe values and units: `records`, `keys`, `extractor`, `_bytes`,
`_bits`, `_len`. Every stage works on the caller's slice; no stage copies the
slice into a second buffer of its size.

## Pipeline

One recursion sorts every slice. It learns the slice's order and its varying key
bits in one pass, chooses one split, executes that split with the
implementation the slice's size allows, and recurses into the buckets.

### 1. Preparation

`engine::prepare` finishes a slice before any split is chosen.

- **Order probe.** The extractor scans adjacent pairs. A slice whose keys never
  descend is sorted, and returns without a second scan, a key evaluation more
  than the probe needs, or an allocation.
- **Inspection.** `scan::inspect` reads keys in vector blocks and accumulates, per
  record, the union and intersection of its adjacent scans. Their exclusive or
  is the set of key bits that vary within one scan: the *varying bits* inside the
  candidate mask a parent handed down. A key function that changes its answer
  can hide every difference outside the candidate; the slice is then constant
  by construction and makes no progress, so it ends there.
- **Descending slices.** Keys that strictly decrease are one reversal away from
  sorted. Keys that never increase but tie are reversed stably: the slice is
  reversed, then each equal run is reversed back.

### 2. Insertion probe

For a root larger than the insertion leaf, `insertion::probe` insertion-sorts a
window of one tile within a budget of one swap per window record, then scales
the swaps that window needed to the whole slice. Inputs within their moves of
sorted finish here; anything else pays at most the window plus the budget that
window implies before the radix route takes over. A window that needs no swaps
says nothing about the records behind it, so the slice is left to the recursion.

### 3. Split choice

`cost` is the only module that decides. It prices the routes a slice can take,
and the engine executes the cheapest one, so a policy change is a change to the
model and never a branch that names a distribution.

A plan is priced in estimated cycles per record from five measured terms: a key
read (a load for a sealed key, several times that for a key function), a record
move, a bucket slot, a prepared child and an insertion step. The routes are:

- **Rotate.** A slice that already reads `0*1*` or `1*0*` in its top varying bit
  is two runs. One rotation puts the clear run in front, and both children are
  ordered or nearly ordered slices that the order probe or the insertion probe
  finishes, so the split costs one rotation and two scans with no scratch.
- **Pivot.** A node samples 64 keys in runs of 8 and names the most repeated
  value if it covers at least a quarter of the sample. The model prices the
  band that value finishes against the rest of the slice; an equality partition
  separates it, so heavily repeated keys stop costing one pass per bit. If
  changing keys leave the whole node in one band, the digit route consumes
  candidate bits instead of repeating an unchanged split.
- **Split.** One counted pass by a window of key bits, then the recursion. The
  model prices every width from one bit to the widest the caller allows and
  takes the lowest: a wide window leaves children the leaf finishes, a narrow
  one pays fewer table slots. Windows always start at the top varying bit, so
  each pass removes the highest remaining difference. A node is bounded by 8
  bits, where one write cursor, one count and one output block per bucket still
  fit in L1.
- **Flat.** A tile can also finish with one counting pass per window of varying
  bits, least significant first, and no recursion. The model prices those
  passes against a split: a spread tile splits once into leaves, while a
  concentrated tile that would need a multi-level descent finishes flat.

Two facts the model needs about the keys are measured, not assumed. A sample of
evenly spaced keys names the buckets a window occupies. A sample that repeats
buckets far more than a uniform spread would (about `buckets * (1 - (1 -
1/buckets)^sampled)` occupied slots) is concentrated, and the buckets it names are
the ones the slice occupies; otherwise the slice spreads over as many buckets
as it has records. The same sample answers every candidate width, so pricing
fourteen widths costs one pass over 128 keys.

Pricing a plan together with its consequences is what keeps the routes honest: a
split pays for the children it prepares and the leaf it leaves behind, and a
flat plan pays for its table. A recursion node also pays for planning itself,
which is a fixed cost: a tile of a few hundred records spends more on its sample
and prices than on its pass, so the model keeps slices large enough to amortise
it. The model deliberately ignores branch prediction; when two plans price
equal, it prefers the one that moves each record fewer times. [The port
design](port.md) records the measurements behind the weights, and
[semantics](../../conformance/semantics.md) owns the guarantees the routes keep.

### 4. Split execution

`engine::Sorter` executes the chosen split and recurses. The two partition
implementations leave the records in the caller's slice and report each
bucket's end, so the recursion is the same for either:

- a node uses the block partition below, whose scratch is O(sqrt(n));
- a tile uses a counting pass in one tile-sized buffer plus two bytes of cached
  bucket id per record.

Each bucket is then a contiguous slice, and the recursion repeats from
preparation with the candidate bits reduced by the digit. A partition whose
buckets are all at or below 48 records needs no recursion: their keys are
ordered by bucket, so one insertion pass over the slice moves exactly what the
separate bucket passes would move and skips the dispatch between them.

### 5. Leaves

A slice at or below 48 records is finished by insertion sort, which is the
cheapest stable sort at that size. A window that covers every remaining key bit
difference is finished even earlier: when a slice's variation spans at most
eight bits, direct keys fill each run from one representative of that run, in
place, without scratch and without moving a record twice.

## Block partition

The digit partition uses the block LSD scheme of Clausecker and Schintke.
Buckets are filled block by block, consumed input blocks are reused as output
blocks, and permutation metadata records their logical order. Stability holds
without a full-size output buffer, and the extra space is O(sqrt(n)) for a fixed
digit width and record size. The block size in records is

```
max(1, 4 * ceil(sqrt(10n / (2 R s))), floor(1024 / s), 32)
```

where `n` is the input length, `s` the record size and `R` the number of
buckets.

The root is the paper's trade-off evaluated for this implementation. A block
size `b` costs `2 R b s` bytes of scratch — one output block per bucket, plus
as many in reserve, because an input block may only be recycled once every
record in it has been read — and `10 n / b` bytes of tables, because a block
position owns two `u32` block-order entries and one `u16` bucket id. The two
terms are equal at that root, so it is the minimum of the sum: substituting
`b = c b*` multiplies the total by `(c + 1/c) / 2`, which grows in both
directions. The engine takes four times that root: the minimum is where memory
is smallest, not where the sort is fastest, and a larger block amortises the
random read that compaction pays per block.

The lower bounds are not from the paper. The compaction pass moves a block with
one copy, so a block holds at least a kilobyte of records; and the bookkeeping
of a block does not shrink with the record, so a block holds at least 32 records
whatever their width. Their cost does not depend on `n`, so the O(sqrt(n)) bound
holds for a fixed record size and digit width.

One pass of this scheme is also the equality partition: the classifier compares
the key with the sampled pivot instead of extracting a digit.

## Tile counting

A tile pass moves its records through one scratch buffer of at most 64 KiB of
records and leaves them back in place. The histogram stores a two-byte bucket id
per record for every record type, so the scatter runs without a key call:

- a user key function runs once per record per pass, which keeps `String`, `Box<T>`
  and `&mut T` records safe and every record type on one path;
- the block scatter for direct keys is already the first read, so the cache costs
  no code there, only bandwidth.

The pass writes to scratch only after the histogram finished, and never reads
scratch as records, so a key panic leaves the slice untouched without a
duplicated record pass.

## Parallel root completion

A large root runs the serial preparation and one split, then distributes that
split:

- the block scatter of a classifier spreads over disjoint shards, each with its
  own block directory and extractor clone; adoption concatenates every bucket's
  prefixes in bucket-major, original-shard order, which is what keeps equal keys
  stable across shards;
- a window that covers every remaining bit counts in parallel shards and then
  fills the disjoint runs in bucket order, needing only `Send` records;
- the buckets of the split are sorted on the caller's Rayon pool, each job with
  its own tile scratch and extractor clone.

Every bucket is a contiguous slice of the input once compaction finishes, so the
serial recursion sorts it unchanged. A slice that already holds its two runs is
finished by the rotate plan without a pass, so it is not distributed. Small
roots and one-thread pools use the serial engine.

## Insertion and scans

Every insertion pass runs one loop, `insertion::run`: it scans for the first
adjacent pair whose keys descend, moves the later record left, and resumes the
scan after it. The loop is inlined so a vector kernel selected for the pass also
inlines its scan; expanding the inversion first and then shifting keeps the
adjacency knowledge, so the pair it compared is swapped without re-reading keys.

| Key source | Adjacency scan |
|---|---|
| Key function (`stable_sort_by_key`) | One record at a time, keeping the previous key, so the key function runs once per scanned record |
| Direct keys, short slices | One record at a time below the vector length |
| Direct keys, otherwise | Vector scan, with the instruction set selected once per pass |

`fearless_simd` selects a supported instruction set at run time on x86; other
targets use their compile-time baseline. Same-width adjacent comparisons are
the only vector kernel the engine needs, so one scan serves the order probe, the
variation scan and insertion adjacency. Dispatch stays inside one insertion
pass: per-scan dispatch repeats overhead on short runs, and moving a SIMD token
outside its target-feature context loses the intended code generation.

## Record handling

Records are moved by bitwise copies between the input and `MaybeUninit<T>`
scratch. Every record has exactly one live owner at any time; other physical
copies are stale bytes that are never dropped, borrowed or returned. Key
extraction borrows only live records, and no user code runs while scratch holds
a second copy of the records.

## Panic safety

Key functions are user code and may panic. On a panic:

- the block partition rebuilds each block's offsets from the bucket fill counts
  and the number of complete input blocks, then moves every live record back
  into the input slice;
- a tile pass writes scratch only after its histogram finished, so the slice was
  never modified;
- the panic is then resumed.

The slice holds every record afterwards, in an unspecified order. No record is
dropped twice or leaked.

## Stack usage

Each tile-local split keeps a cursor table of under 3 KiB on the stack: one
`u16` cursor per bucket the tile digit can address, which is a full tile
divided by the insertion leaf. The depth is bounded by the digit widths, so a
128-bit key keeps a bounded frame. Actual frame sizes depend on the compiler.
