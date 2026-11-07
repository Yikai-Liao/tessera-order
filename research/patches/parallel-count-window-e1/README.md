# e1 Direct count-window source and Tier 1 archive

This isolated follow-on pairs [source replay and safety scope](source/README.md) with [production-feature-OFF confirmation](results/REPORT.md). Each subdirectory has its own SHA256SUMS. It extends the [e8 archive](../parallel-count-window/README.md), preserving that evidence unchanged. No patch is applied to the repository runtime.

The shared runtime is frozen at e1d25297685cecb205d18114e898a39878e3fd83. Public comparison U is fixed at 3653c3607b9f567e2f03ceb6fdd131e18eaee519. Later main changes are excluded.

Tier 1 has 1,548 verified samples and 7,173 timed sorts; the width1 sentinel adds 63 samples and 187 sorts. At W8, candidate C clears full controls in three of four cases versus same-core A and four of four versus public U. Several lower-worker cells and the width1 sentinel remain inconclusive; no tested cell establishes a regression. These are bounded Direct u32/u64 results, not general acceptance or default adoption.

All observed full-sort heap peaks are 65,536 bytes with zero retention. At W8/R256 the new vectors add 18,440 requested bytes; equal peaks do not establish allocation-lifetime causation. Research ON/OFF shows no detected deviation, not equivalence. The report retains full controls, tails, absolute times, feature and scope limits. Tier 2 is not part of this checkpoint.

Publication independently verified source/result checksums, source archive members, both lineage chains forward/hash/reverse/second-forward, and exact frozen Rust bytes. Only private Cargo identities are normalized. No build, test or timing was rerun. Raw samples, harnesses, binaries and local logs remain external.
