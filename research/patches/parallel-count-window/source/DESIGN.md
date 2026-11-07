# Mechanism and limits

Only sealed Direct u32/u64 implementations opt in, and only when the completed varying-bit scan proves a contiguous window of 1..8 bits covers every key difference. Other unsigned types, signed types, floats, Reverse and payload-bearing ByKey records decline. Narrow forwards its SHIFT. No arbitrary records are reconstructed from their keys, and no Sync bound is added to the generic public ByKey API.

Disjoint input chunks count into private rows of one preallocated P-by-R usize histogram, where P is pool size and R=2^width. A serial merge produces R+1 prefix endpoints. Independent output-rank chunks intersect these endpoints and fill their bucket runs. Shared constant bits plus the digit reconstruct the exact u32/u64 value. Equal encoded values are bit-identical, so these runs preserve the result and stability. A hot bucket may span multiple output tasks rather than becoming one serial fill job.

The new patch adds zero unsafe tokens/blocks. It introduces no callback or key-function wait. The existing probe/equality routes, root algorithms and generic scheduling remain unchanged. This is original BSD code inspired by private histograms and prefix-derived independent intervals; no external algorithm source was copied.

## Storage accounting

Exact requested vector element storage is `(P*R + R+1)*size_of::<usize>()` bytes. Both vectors use their requested fixed lengths/capacities and never grow. There is no representative table, ordinal array, n-record output buffer or collected task list.

A separate conservative allowance of `4096*P` bytes accounts for algorithm task/closure/scalar-frame state. This is not measured heap usage and is not an exact guarantee about physical Rayon/runtime stacks. Preexisting pool stacks and scheduler-global bookkeeping are excluded from algorithm-created auxiliary storage. Each nonoverlapping iterator pass has at most P leaves and P-1 branch nodes; the algorithm-owned task/frame state is O(P).

Before allocation, checked arithmetic rejects invalid windows, P<2, P>floor(n/4096), overflow, and any request whose exact vectors plus allowance exceed `8*P*ceil(sqrt(n))*size_of::<record>()`. C=8 is independent of P. Planning tests include large P values without spawning corresponding pools. Original serial counting remains the fallback. Counts/prefix sums are bounded by n because input chunks are disjoint, and nonempty output ranks remain below n.

## What has not been established

The native allocation diagnostic records actual histogram base alignment and row byte ranges. Some observed rows can touch the same cache line, and small rows can share a line even with an aligned base. That supports a possible false-sharing mechanism, not a measured coherence penalty or slowdown. No padding or allocation-alignment change was made.

No claim applies to full-random generic-root acceleration. Gains against original serial, if measured later, cannot all be attributed to threading: this kernel also removes the original per-record representative check and reconstructs intrinsic values directly. Public-policy and same-kernel serial/parallel controls must be reported separately.
