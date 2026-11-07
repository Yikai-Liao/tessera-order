# Parallel virtual-bucket research checkpoint

This standalone crate preserves a validated structural experiment: parallel child jobs
consume stable root block directories directly, without first materializing contiguous
child slices. It is not a member of the production workspace and is not a supported
Tessera sorting API. No benchmark improvement is claimed for this checkpoint.

Only sealed `u64` and 16-byte key/ordinal records are supported. The root scatter,
final global packing, and physical restoration remain serial. The library intentionally
retains per-block atomic diagnostic counters; performance work uses a separate profile-off
control rather than presenting instrumentation overhead as an algorithmic result.

## Space and ownership

For radix R, block size B, root shards Q, child closures P, and tile capacity K,
additional record capacity is `[Q*R + P*(3*R+8) + 8]*B + 2*P*K` cells.
K is at most 64 KiB divided by record size under the default configuration. The only
n-record storage is the caller's input. Metadata is O(n/B + Q*R + P*R).
The default B grows with the square root of n; arbitrary caller configurations do not
unconditionally have a sublinear bound. All retained output scratch is included.

Exactly P fixed worker closures lease a child directory and a bounded free-slot batch
under a short metadata mutex. Sorting holds no queue lock. Each local dense arena accesses
only its leased slots; finishing a child returns at least its whole loan. Completed
children retain sum(ceil(child_len/B)) slots until the final packing pass, not ceil(n/B).
An explicit short-tail normalization prevents treating the input tail as a full block.
The staged public API rejects overlapping descriptors before concurrent raw access.

MSD depth is capped; oversized leaves use virtual LSD so pending sibling tails cannot
grow with unbounded recursion. Two tile buffers are preallocated per worker. Scoped OS
threads execute the same raw-moving kernel as Rayon to allow focused concurrent Miri
checks without relying on Crossbeam's Miri compatibility.

## Contract limitations

This is a Copy-only experiment. Metadata-allocation or scheduler panics do not guarantee
preservation of the input permutation. It does not establish the production API's owned,
exclusive-borrow, changing-key, or panicking-extractor contract. The arena's validated
infallible sealed key implementations do not permit arbitrary user key callbacks.

Native-root adoption, streaming leaf output, and shared output-boundary packing remain
future experiments. They require new ownership and performance verification.

## Validation

The originating frozen source passed debug and release suites (15 tests, including 3,662
stable-reference scenarios per suite), Clippy with warnings denied, and six selected
default Stacked Borrows strict-provenance Miri tests. Miri covered 12 full sorts, overflow
rejections and widened shard arithmetic, including three forced concurrent scoped leases.
Native Rayon cases vary ambient workers independently of requested child closures.
Full native 32-bit execution, full Miri, and a clean performance matrix are not claimed.

`SOURCE_LINEAGE.json` verifies byte-identical library, test and manifest sources.
The diagnostic benchmark CLI, raw measurements and binaries are deliberately excluded.
`INTEGRATION_VALIDATION.md` records checks run on this packaged location.

Run checks independently from the repository workspace:

    cargo test --manifest-path research/parallel-virtual-bucket/Cargo.toml --release
    cargo clippy --manifest-path research/parallel-virtual-bucket/Cargo.toml --all-targets -- -D warnings

Do not infer production acceptance from this preservation commit.
