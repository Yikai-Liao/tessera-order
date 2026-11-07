# Scope and safety argument

## Admission and exact values

Only sealed Direct u32/u64 overrides opt in. Varying width must be1..8; all other sealed types and ByKey decline. No arbitrary owned record is reconstructed from a key. Equal unsigned keys have identical values, so filling a bucket from its encoded bits preserves exact records and stability.

The public prepare_sort computes a full-input varying-bit mask before sort_prepared. That full scan, not MODE sampling, proves all bits outside the selected window constant. The helper combines the shared fixed prefix with each bucket number. Narrow forwards the new hook using low+SHIFT; it does not gain unrelated serial-count forwarding. Its only current offsets are0 and64, and unsupported u128 still declines.

The preexisting sorted/reverse/probe, P1 and public small-input gates remain unchanged. Hook failure leaves the input unchanged and reaches the existing fallback. Successful counting uses disjoint mutable input chunks with private table rows; prefix merging is serial; output-rank chunks are disjoint and split even hot buckets. No user callback or key-triggered Rayon wait is introduced.

Window and allocation arithmetic is checked before allocation. Prefix totals are bounded by input length. Output chunk starting ranks are within input length; fill handles empty buckets and cuts at exact cumulative ends. The new code contains no unsafe.

## Storage

For P workers and R<=256, requested vector storage is exactly `(P*R + R + 1)*size_of::<usize>()`: P histogram rows and R+1 prefix entries. Neither vector grows; debug assertions check requested capacities. There is no n-record data or ordinal buffer.

Admission separately includes a conservative4096*P allowance for algorithm task/closure/scalar-frame storage. This is not measured heap usage or a guarantee about complete Rayon worker stacks. Existing Rayon stacks and pool bookkeeping are outside this algorithm's auxiliary allocation accounting.

Checked admission bounds the accounted bytes by `8*P*ceil(sqrt(n))*record_bytes`, with C=8 independent of P, and requires P>=2 and P<=n/4096. Small inputs, excess workers, overflow and budget excess decline to the original path. Thus fixed P=8 observations are not the space proof.

No padding or alignment tuning was introduced. Actual histogram address modulo64 and row ranges were observed in an untimed diagnostic. Adjacent-row cacheline sharing remains a layout hypothesis, not a measured performance conclusion.

## Preserved control

The original stable_sort and its existing count-fill remain unchanged. A same-kernel sequential schedule would not be a substitute for that true fullserial baseline; this public checkpoint has no research scheduler at all. Any later comparison must use a matched public scheduling baseline, not attribute unrelated private producer/pipeline differences to counting.
