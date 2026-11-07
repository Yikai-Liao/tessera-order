# Root-only shard-budget controls

This research patch applies after `combined-rayon.patch`. It preserves the
validated planner implementation and its seven additional unit tests. The
exported constant defaults to zero (uncapped); no globally winning policy is
selected. The independent timing snapshots used minimum four and minimum eight
floor-sized blocks per bucket. Child and block-permutation concurrency are not
capped with the root, and explicit block-size overrides keep their test policy.
The existing serial-root callback path is used when the budget selects one shard.

## Evidence scope

All three 0/4/8 library variants passed 48 tests. The cap8 package separately
passed 69 all-target tests, six doctests, 48 Rust 1.85 library tests, no_std, and
seven focused Miri planner/publication tests. Twelve untimed input cases across
11 variants verified 132 complete outputs. An inherited unchanged Clippy warning
in compact.rs (`for_each(&run)`) prevented a warnings-clean claim.

Ten timing cases used nine randomized blocks and 11 variants: 990 samples and
30,311 fully verified sorts, every accumulated sample at least 25 ms. Original and
both candidate policies have A/A and source-placement controls; full symmetric
envelopes and all outliers are retained. Only owned d256 at 32,768 records has a
cap-versus-original improvement outside the full envelope: paired ratios 0.285/0.286,
nine wins, control envelope [0.366, 2.730]. It restores about 0.20 ms behavior versus an
approximately 0.70 ms regressed parallel path; strong serial is about 0.19 ms.

No direct cap8-versus-cap4 comparison establishes a winner. u32 d256/1M trends
slower with cap8, so lower scratch is not a universal speed claim. Protected 10M
random u64/Row16 remain inside controls. The frozen harness has no separately
linked tight-serial or serial A/A controls; its serial comparisons are descriptive.

Thirty-five independent allocator rows fully verify outputs and retain zero
additional bytes at return. u32 d256/1M extra heap is 4,478,528 B uncapped,
2,301,388 B cap4, and1,220,720 B cap8. Owned/wide 262K cap8 roughly halves scratch.
Protected 10M u64/Row16 keep 6,168,395 / 7,971,658 B. Per-thread square-root workspace
is permitted; these practical floor costs are measured, not an automatic reason
to reject a faster candidate.

Raw samples, harness and detailed reports remain outside the repository in the
root-budget evidence bundle. This export only packages exact source/tests and
review notes, not new performance evidence or production integration.
