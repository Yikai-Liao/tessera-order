# Serial slot-capacity and stability argument

Extracted verbatim from the originating validated prototype README.
This proof covers the serial schedule only.

## Slot budget proof (serial child schedule only)

Let Q=floor(n/B), R=2^digit_bits, and P=P_root. A stable directory with t bucket tails and
m records uses at most m/B + t physical slots. A packed stream has t<=1. Disjoint logical
streams partition the n records even when their actual initialized bytes include stale
Copy values. At most one currently consumed source span adds one slot beyond the bound
from its unread records. Records in a leaf tile only reduce arena occupancy.

The worst active frontier is:

- root sibling directories retaining at most P R tails in total;
- pending/completed siblings of the one second MSD split adding at most R tails;
- the old directory of one oversized LSD leaf adding at most R tails;
- the new output of that same LSD pass adding at most R tails.

Using PR+3R tails intentionally double-counts tails replaced when descending and is a
conservative upper bound. There is one serial streaming cursor, hence at most one extra
partly consumed source slot, and one nonrecyclable short original input slot. Therefore
Q+(P+3)R+2 slots suffice. The physical arena supplies Q+(P+3)R+8 full-capacity slots,
plus the original short slot if present. The six-slot gap absorbs integer endpoints and
means an output allocation cannot exhaust the proved capacity. A consumed source is
released before allocating for its final local Copy record, so no unread record can be
overwritten. Leaf output and packing have at most one output tail, below this frontier.

Every source pass reads each span in logical order, appends stably by digit, and releases
that physical slot only after its final read. Leaf merges select the left record on equal
keys. Stable root adoption follows original shard order. LSD stability preserves already
shared higher bits. Thus the final concatenation has full-key stable order, including MAX,
equal, low-cardinality, heavy-skew and shard-crossing ties.

This proof does not cover concurrent child workers. A future concurrent version must count
root retained tails plus a per-active-worker frontier, per-worker leaf buffers, and slots
retained by every completed output. The current fixed constant must not be reused for P
active child jobs. An unbounded recursive MSD version likewise violates this proof.
