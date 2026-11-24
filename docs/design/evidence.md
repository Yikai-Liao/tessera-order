# Engine evidence

This page keeps only what code and Git history cannot: placement lessons, rejected or paused directions, open measured losses on landed changes, retained workload baselines, and the engine's structure analysis. Full measurements of accepted changes belong in their PR descriptions; raw runs, commands and result paths stay outside Git. Entries describe their recorded revision and environment, not current-head validation. Follow the [benchmark protocol](../development/benchmarking.md#run-an-experiment) for new measurements, and delete an entry once a change makes it obsolete.

## Reading small-case results

Process-level and three-block screens flag changes to investigate; only linked in-process comparisons with independent copies separate a source change from code placement, and even those do not bound all layouts. Identical instructions at another address moved short inputs by 20–90%:

| Investigation | Observation | Conclusion |
|---|---|---|
| Port vs prototype, ordered `u32`/`u128` | Ratios 0.53–1.28 initially, 0.79–1.86 after a comment-only rebuild, 0.98–1.06 with `-x86-branches-within-32B-boundaries` | Skylake-SP keeps branches crossing 32-byte boundaries out of the µop cache (JCC erratum). Redirecting six identical probe functions to one address removed the `u32` gap. |
| Low-fanout tiles (`e0ecd94` → `54926ca`), `random/48` | Identical copies differed by 21–22% against A/A of ±1.4–3.8%; moving the candidate's `sort_tile` body 160 bytes to the baseline address returned 282–287 ns/sort to 243–247 (baseline 237–243) | Tile code placement; the Intel runner's mechanism is unidentified. |
| Serial CI artifacts (`fafaaa2` → `a26cac4`), `row_u64/equal/48` | Alert +6.9%; candidate/baseline 1.0231 against A/A 0.9672–1.0328; both executables ran the same 348 instructions | Placement sensitivity; a small whole-binary cost cannot be excluded. |
| Eight-core parallel | Same-entry A/A 0.80–1.20 with inherited masks, 0.90–1.10 pinned, 0.78–1.22 on the pinned repeat | No stable detection window for a 5–10% source gain on that host. |

PR builds combine 64-byte function entries with LLVM's 32-byte x86 branch-boundary alignment. Replays across two Xeons and an EPYC narrowed `u64/random/49` on PR #241 from +6.6%..+39.5% to −0.6%..+2.6%, and on PR #248 narrowed `u64/random/49` from −5.2%..−0.3% to +0.1%..+1.1% and `row_u32/random/49` from −1.2%..+60.9% to +1.9%..+2.7% ([PR #241 replay](https://github.com/Yikai-Liao/tessera-order/actions/runs/44876013127), [PR #248 replay](https://github.com/Yikai-Liao/tessera-order/actions/runs/44876213334)). These controls support the PR build choice on the observed runners without isolating either flag or bounding future layouts. Branch alignment itself moved the prototype by −30% to +69% across small cases; stable Rust has no alignment attribute, so placement control belongs in the [benchmark harness](../development/benchmarking.md#additional-source-attribution-controls), not the crate.

## Rejected or on hold

Do not repeat these directions without a construction that addresses the recorded blocker.

### Root preparation, order probes and insertion

- **Moving insertion-key cache, on hold.** Caching the moving record's encoded key across shifts helped local keyed-row screens, but hosted [PR #210](https://github.com/Yikai-Liao/tessera-order/pull/210) measured `row_u32/random/49` +79.1% without improvement alerts elsewhere; linked local runs did not reproduce the loss. The same case is a hosted outlier without the cache ([PR #224](https://github.com/Yikai-Liao/tessera-order/pull/224): +63.9% hosted, −15.1% on rerun, 0.879 [0.838–0.922] locally). Treat a solo alert on `row_u32/random/49` as hosted variance unless reproduced, and reopen the cache only with cross-placement and hosted evidence.
- **Precomputed insertion point plus rotation, rejected.** Preserved panic safety without a hole guard and helped wide records, but slowed small direct-key insertion substantially. Reopen only with a construction that keeps the small-record cost.
- **Pair insertion for short leaves, rejected.** Added two unwind guards and regressed highly skewed inputs in hosted runs despite helping random short leaves.
- **Flat samples as ordinary order candidates (R2), rejected.** Removing `Order::Flat` ([PR #105](https://github.com/Yikai-Liao/tessera-order/pull/105)) lost the vectorizable constant-input scan: `u64/equal/65536` 0.839/0.834 and `row_u64/equal/65536` 0.925/0.921 locally (A/A ±0.6%), and hosted serial `u64/equal/65536` +70.0%, four-thread `u64/equal/1M` +82.0%. A replacement must keep the constant-input scan.
- **Always-stable reversal (R5), rejected.** Dropping `Descending { strict }` ([PR #136](https://github.com/Yikai-Liao/tessera-order/pull/136)) made every hosted `reverse` case 1.74–2.25× slower (`f64/reverse/1000` +125%, four-thread `u64/reverse/1M` +115%). Strictness, already observed during confirmation, is what skips the equal-run scan.
- **Separate root insertion shortcut and unit-return finishing (R4/X4), retained.** Consolidating root insertion into `insertion_probe` kept both low-level insertion entries: deleting the shortcut slowed `u64/random/48` by ~49.5% (baseline/current 0.6691/0.6692, A/A ±0.13%), and routing unlimited finishing through the counted entry changed code generation across the direct-extractor dispatch boundary.
- **One root helper for half-key dispatch (X3), rejected.** Sharing `KeyHalf` dispatch between serial and parallel roots cannot unify `Clone + Send + Sync` parallel bounds with serial `FnMut` closures without a macro for ~12 lines; narrowing 64-bit keys further is a routing change, not a cleanup.
- **Direct argsort preparation without `0..len` prefill ([#286](https://github.com/Yikai-Liao/tessera-order/pull/286)), closed.** Probing directly on source slices to avoid pre-filling `0..len` before `scatter_indices` measured hosted 65,536-record random `argsort` slower (`u64` +7.3% [1.049–1.089], `{u64,u32}` +3.9%) against one hosted gain (`u64` 1% far swaps −10.0%; local gains were on inputs outside the hosted plan) and altered `sort`'s root codegen. Reopen only if random `argsort` stays at least parity with `main` without perturbing `sort`.
- **Closure-routed cached varying-bit reuse in `prepare` ([#290](https://github.com/Yikai-Liao/tessera-order/pull/290)), closed.** Parameterizing `prepare`'s variation scan through a closure changed `sort`'s optimized root IR and measured hosted `u64/reverse/48` +21.5%, against small non-alerting cached-route gains (65,537-category `u64` −2.2% [0.962–0.986], `{u64,u32}` −2.9% [0.953–0.979]). Duplicating `prepare` with a macro to preserve `sort`'s IR was not worth skipping one key scan on the cached wide-key route.
- **Other rejected root/probe micro-variants:** first-distinct-key order confirmation (14–29% slower on 1,000-record constant inputs); ascending-first `confirm_order` (+6.6% instructions, 8–13% slower on `f64/sorted/65536`); streaming scalar 99%-sorted budgeted insertion (`u64/sorted99/65536` 1.288 vs A/A ±1.1%); out-parameter strict descent (2.4–5.3% slower on `u64/equal/65536` and `{u32,u32}/random/49`); early insertion finish for tiny inputs; geometric collection of insertion-probe keys; exact-length key collection for cached grouping.

### Tiles, counting and selection

- <span id="recomputed-tile-buckets-rejected">**Recomputed tile buckets, rejected.**</span> Recomputing `ByKey` buckets during tile scatter instead of caching `u16` bucket IDs removed 30 net lines and ~20% of 8-byte-row tile heap scratch, but needed up to 32 KiB of stack bucket bounds and slowed several 8-byte-row groups by 8–10% and owned `String` by ~5%. The cache is tile-bounded, not `O(n)`.
- **MSD-only tile finishing (T2), rejected on a cost model.** Replacing two-pass LSD with one top-digit MSD pass ([PR #138](https://github.com/Yikai-Liao/tessera-order/pull/138)) slowed hosted serial `u32/random/1M` by +55.5% and `u64/random/65536` by +7.2%. Both move records twice on uniform tiles, but LSD is branch-free while insertion over random small buckets mispredicts almost every inversion (1M random `u32`: −8% instructions but ~10× branch misses, 0.185 vs 0.019/record, explaining ~2/3 of extra cycles). Reopen only for non-uniform tiles with a branch-free second level whose table work stays below LSD's two windows.
- **Tile-level micro-variants, rejected:** removing the histogram maximum and testing insertion eligibility from bucket ends (regressed `random/49` by 11–24% and `u64/random/1M` by ~14%); sample-weighted child work in tile planning (`bimodal/1000` 1.316 → 1.057, `random/1000` +2.7–3.5%); individually sorting every small MSD bucket instead of one contiguous tile insertion sweep (18–31% slower on short/medium keys and rows).
- <span id="tile-local-selection-experiment">**Tile-local selection and classified-prefix follow-up, rejected.**</span> Running a top-digit MSD pass for partial requests and finishing only requested bands slowed every partial operation in the continuous plan: on 65,536 random `u64`, multiple selection +59%, rank range +38%, single selection/prefix/top-k +19–20%, and `row_u64` +6–24%. Binary search over completed cursors ([PR #236](https://github.com/Yikai-Liao/tessera-order/pull/236)), skipping empty/singleton buckets and narrower initial digits did not recover the loss against one contiguous insertion sweep. In the same follow-up, a runtime block scatter limit added ~8.5% data reads on Zipf2 argsort (eliminated by a static limit), and the former binary-partition API flagged an unresolved +40.4% slowdown on 65,536 random `u64` before removal in [PR #239](https://github.com/Yikai-Liao/tessera-order/pull/239). Reopen tile-local selection only with a workload and design that beat full tile sorting.
- **Pre-root 11-bit histogram + three-way `block::partition` for dense interior `select_nth`, rejected.** Against `1d6e35f`, locating the rank key with a read-only 11-bit histogram before one three-way `block::partition` ahead of `prepare` slowed 9 of 12 serial 65,536-record rank-32,768 cases across `u64`, 16-byte rows and 128-byte records (`u64/zipf2` 3.21× [3.19–3.22], `row_u64/zipf2` 2.94× [2.90–2.97], `u64/d16` 2.67× [2.59–2.80], `record128/random` 1.31× [1.17–1.36]; only `record128/far_swaps` won at 0.911×; process-level `compare commit`, i7-11700B, CPU 2, ASLR off, Rust 1.99.1; [retained report](https://github.com/Yikai-Liao/tessera-order/pull/285#issuecomment-7110150468)). Bypassing `prepare` lost `try_count_window`, `SourceOrder` and `NodePlan::Equal` while adding a full read pass on uniform inputs.

### Nodes, movement and unstable contracts

- **Bucket-order detection inside the block partition (N2), on hold.** Skipping movement requires knowing bucket order before scatter, which is the scan `two_runs` already stops on the first violation; folding it in saves no scan and loses the rotated `1*0*` route (`sorted_tail`, `bimodal`, `far_swaps`). Reopen only if rotation is kept without adding a scan to unordered nodes.
- **Unstable node-partition kernel and unstable public APIs, retired.** An in-slice swap-based unstable node partition (against `0499d35`, sharing preparation, planning and stable tiles; i7-10875H, 9-block linked screen at 1M records) traded streaming single-classification block passes for a histogram plus dependent swaps and reclassification: it ran 1.74–2.35× slower on `u64` (`random`/`zipf2`; `d16` 1.00×) and 16-byte rows (`random`/`d16`/`zipf2`) and 1.07× slower on 128-byte `random`, winning only on repeated-key 128-byte records (`d16` 0.723×, `zipf2` 0.687×; peak heap 8.1/2.0/34.6 MB → 42/4/95 KB). Delegating only small leaves to `slice::sort_unstable` improved mainly 16–48-record full sorts while leaving all 90 rank cases within A/A noise and identical heap; rejecting frozen callback keys there (where non-total-comparator panics are permitted) does not weaken cached bucket IDs in raw counting scatter, which remain a memory-safety requirement. Reopen unstable APIs only with a generally applicable primitive; if pursued, keep one recursive engine with a compile-time node-movement choice rather than duplicating planning or adding a strategy trait.
- **Serial scatter micro-variants, rejected:** binary-partition prefix replay and suffix-only distribution with leading-run rotation; fixed four-record scatter groups (slower on wide records than width-scaled grouping); deriving writer positions from block ends (extra arithmetic and cold-path division).

### SIMD

- **Signed-integer lane comparison for `f32`/`f64`, rejected.** Against `1d6e35f`, encoding floats as `bits ^ ((zero - (bits >> (BITS - 1))) >> 1)` for signed SIMD lanes produced no win across 48 serial cases (`u64`/`f64`/`reverse_u64`/`reverse_f64` × `random`/`sorted`/`sorted99`/`far_swaps` × 1k/65k/1M, 3-block linked `compare revisions`, two placements each, i7-11700B, CPU 4, Rust 1.99.1; [report part 1](https://github.com/Yikai-Liao/tessera-order/pull/285#issuecomment-7110151405), [part 2](https://github.com/Yikai-Liao/tessera-order/pull/285#issuecomment-7110152307)): both baselines beat it on `f64/sorted99/1M` (0.942/0.958 vs copy 0.983, A/A ±1.8%), `reverse_f64/sorted/65k` (0.974/0.944 vs copy 1.059, A/A ±1.7%) and `reverse_f64/random/1k` (0.963/0.957 vs copy 1.006, A/A ±1.0%). Retain the unsigned total-order encoding.
- **SIMD constraints (AVX2, separate binaries, outside the paired protocol — not formal speedup claims):** dispatch belongs once per insertion pass (per-scan dispatch cost more than it saved on random input, and a kernel outside its `#[target_feature]` context ran 3.5× slower on sorted `u64` and 7× on sorted `f64`); materializing user keys on an 8,192-record near-sorted tile ran 1.4–1.6× slower than scalar callbacks; 16-lane prefix sums lost to 8 lanes (cross-lane swizzle); stable 3-way compress scatter of 64K `u32` took 460 µs vs 147 µs scalar, and AVX-512CD conflict detection would double the 32 KiB cursor table.

### Parallel execution

- **Job-local finisher scratch (C), rejected ([PR #181](https://github.com/Yikai-Liao/tessera-order/pull/181)).** Replacing the engine-local scratch pool with a factory/mutable-visitor protocol leaked initialization and lifetime obligations into block entry points without simplifying the interface. Reopen only for a required job-local mutable consumer or measured pool-lock cost.
- **One shared parallel free-block stack, rejected ([PR #183](https://github.com/Yikai-Liao/tessera-order/pull/183)).** Local paired comparisons reproduced parallel slowdowns, so per-bucket caches stay for parallel completion (serial completion uses one pre-reserved free stack since [PR #281](https://github.com/Yikai-Liao/tessera-order/pull/281)).
- <span id="repeated-key-root-sharing">**Repeated-key parallel root sharing ([PR #78](https://github.com/Yikai-Liao/tessera-order/pull/78)), unaccepted.**</span> Its [paired screen](https://github.com/Yikai-Liao/tessera-order/actions/runs/43033277279) (`c2c84d5` vs `c168df3`) showed serial `u64/random/49` +15.2% and `u64/random/1000` +12.7% despite unchanged serial source (layout hypothesized, unproven). Historical gains stay in [PR #78](https://github.com/Yikai-Liao/tessera-order/pull/78#issuecomment-6755255766); the earlier cache-lock batching variant was rejected.
- **Parallel micro-tuning, paused:** index-only leaves/indexed final writes (14.8–18.4 ms vs 11.6 ms), frozen-directory wave (15.3 ms), cache-line-padded owners (inconclusive, A/A ±9.1%), source-aware free-block placement (−15% relocation volume, timing unverified), sorted free list, and final-scatter fusions A (0.982–1.102×) and B (0.957–1.254×, slower on `u32`, against parallel A/A 0.80–1.20).

<a id="landed-with-open-losses"></a>

## Open losses and unresolved controls

These changes are in the engine; each measured loss stays open individually and no distribution or size exception creates a performance budget.

| Change | Gain that justified it | Open loss or caveat |
|---|---|---|
| Direct source-ledger completion ([#120](https://github.com/Yikai-Liao/tessera-order/pull/120)) | Retires parallel permutation/compaction recovery, keeps direct final placement | Scoped acceptance: hosted 4-thread `row_u64/skew99/1M` +9.8% (final head +8.5%), local `skew99/1M` `row_u64` +18.6% / `row_u32` +11.2% / `u64` +10.0%, and serial `u64/skew99/1000` +6.6%. Causes unresolved; see [shared ledger study](#shared-ledger-study). |
| Bounded tile planner (vs `3db3860`) | `random/49` 1.6–1.9× faster | `row_u32/random/65536` 0.977/0.982, `u64/bimodal/65536` 0.977/0.990, `u32/sparse_digits/65536` 0.959/0.961 (A/A ≤ ±1.5%); `u64/bimodal/49` current B 0.806. |
| Shared planning & required SIMD (`b8968b6` → `383ddb7`) | `d16` 0.88–0.89 | `random` 1.035 (`u64`) / 1.047 (`row_u64`), cause unidentified. |
| Shared order confirmation (vs `28e6a96`) | `sorted/1000` 1.36 (`u64`) / 1.55 (`f64`); descending −20% to −25% instructions | `u64/sorted/65536` 0.958/0.961, `f64/sorted/65536` 0.983 (+29.5% / +5.3% instructions in AVX2 scan after inlining change). |
| Candidate-masked root/child scans (vs `ce71415`) | One scan implementation | `row_u64/sorted_tail/49` 0.804/0.800 (A/A 0.855–1.145). |
| Counting-pass inlining | `u32/random/1M` −11.45% instructions, −14.42% reads | `row_u64/random/1000` 0.954/0.950. |
| Scalar insertion finishing (vs `26b340c`) | Random `u64` 0.94–0.97 (8,193 to 1M) | `u64/sorted_tail/65536` 1.0219/1.0108. |
| Ordered children & bucket-order split (vs `8bd64d9`) | `sorted_tail/65536` 0.333; scratch 8.7 MB → 65 KiB | `d16/65536` 1.1214 (tracks placement; Portable SIMD −1.1%). |
| Child order before variation ([#211](https://github.com/Yikai-Liao/tessera-order/pull/211)) | Four `sorted_tail` cases −8.1% to −19.0% | Hosted `u128/sparse8/1M` +3.3% (consistent locally), `u64/random/8193` +4.2%, `row_u64/sorted/48` +6.0% (no alerts, placement unproven). |
| Full-sort comparison vs `rust/v0.1.0` ([#227](https://github.com/Yikai-Liao/tessera-order/pull/227), `d44bf77`) | 16/98 continuous cases faster (`row_u64/random/1000` −29.5%, 4T `u64/d4096/1M` −27.2%, `sorted_tail/1M` −17% to −21%, short random −7% to −14%) | `u64/equal/48` +12.7% [1.116–1.133] (single placement). |

Neutral or accepted without recorded losses: strictly descending reversal (1.24–1.56×), direct insertion-sized roots (`u64` 0.889, rows 0.943), active block directory (0.998–1.001), terminal parallel completion (0.9965), and counted terminal tiles ([PR #214](https://github.com/Yikai-Liao/tessera-order/pull/214): repairing `KeyCounts` in-place histogram construction and bounds assertion turned an initial 12–14% `u128/d256/1000` loss into −3.8% process-level; equivalence across all widths/placements remains unproven).

### Short insertion leaves

The [width-based insertion limit](../explain/tiles.md#three-leaves) routed wider records to radix sooner, bringing random 48-record inputs to 0.62–0.76× of `slice::sort` (nine-block linked comparison across seven layouts). Halving both comparison allowance and minimum length routed 16 owned records and 32 low-cardinality floats to radix too early. Single-guard hole-shift insertion later landed in [PR #284](https://github.com/Yikai-Liao/tessera-order/pull/284). Open losses from the pre-#284 width-limit comparison (still pending re-evaluation against `slice::sort` and the prior engine):

- Random `u64` at 8, 16 and 32 records took 2.07×, 1.53× and 1.27× of `slice::sort`.
- Nearly sorted `u32`, `u64` and `{u32, u32}` at 48 records were 18–22% slower than the previous engine in both baseline placements (new width-based limits send them to radix under the small-slice probe swap budget).
- Nearly sorted `f64` at 24 records was 19% slower in the primary placement (copy ran 8% faster, so placement accounts for part of the gap).

<a id="shared-ordered-finishing-controls"></a>
<a id="shared-order-confirmation-for-serial-operations"></a>

### Shared ordered finishing and order confirmation

- **R3 `finish_ordered` ([PR #107](https://github.com/Yikai-Liao/tessera-order/pull/107)):** short-input linked screens at 48 records had wide A/A noise (up to ±34% on `row_u64/equal` and ±23% on `row_u64/sorted`) and +0.30–0.56% `u64` instructions; short controls stay unresolved.
- **R4 `insertion_probe`:** short linked screens at 48 records left `row_u64/random/48` (0.992/0.892 vs copy 0.922, A/A ±0.65%) and `u64/random/48` (0.978/0.981, A/A ±0.88%) unresolved; no source-causal speedup claimed.
- **Order probe stride floor and grouped scalar confirmation ([PR #234](https://github.com/Yikai-Liao/tessera-order/pull/234)):** lowering the target sample to 512 pairs at all lengths regressed 1M `sorted_tail` by +19% serial and +10–15% 4-thread; keeping the [order-probe](../explain/routing.md#order-probe) stride floor restored `sorted_tail` to noise while keeping medium-input ordered savings. Short-input placement sensitivity (including `u32/sorted99/1000`) stays open.

### Shared explicit direct-key scan

[PR #126](https://github.com/Yikai-Liao/tessera-order/pull/126) (S1) replaced separate integer, float and reversed scans with one explicit `fearless_simd` scan; scalar-loop prototypes hit register-pressure and shuffle losses, whereas explicit SIMD kept parity even with LLVM loop/SLP vectorization disabled (2.1–2.75× faster than parent reversed floats without autovectorization) and ran 1.45× faster on native short sorted floats. S1 individually accepted ~6% losses on `u32/random/49` and `u64/random/49` plus unresolved short-input placement differences.

[PR #128](https://github.com/Yikai-Liao/tessera-order/pull/128) retired `VectorKeys` by passing a non-dispatching inversion scan alongside `Direct` into `insertion_loop`. Local long-input results stayed near parity; hosted alerts on `u64/two_values/65536` (+24.8%, later absent) and `u64/d4096/65536` (+12.1–12.2%, unreproduced locally or in cloud-binary replay) and 49–1,000-record differences (up to 29%, `u64/skew99/1000` +7.4%) were accepted individually for that PR without waiving later regressions.

### Operation-extension foundations

Foundations A (scalar order primitives below `refine.rs`), B (tile planning/scratch/restoration extracted), and C (final ranges published directly from ledger start/length) all passed correctness/allocation checks but left short-input (48/49-record) linked placement differences unresolved; keep timing acceptance open without tuning source order to a favorable copy.

#### Refinement tile-budget ownership

Moving the default tile budget into `refine.rs` kept identical peak heap across all 27 inputs (`u64`, 16B rows, 128B records × `random`/`d16`/`sorted` × 48/4,096/65,536, i7-10875H, Rust 1.99.1, 3-block and 9-block linked screens). Open unresolved short-input differences in the 9-block repeat (larger inputs stayed ≤ 1.020): `record128/d16/48` (1.157 vs A, 1.012 vs B, A/A ±9.37%), `record128/random/48` (1.153 vs A, 1.011 vs B, A/A ±17.12%), `row_u64/sorted/48` (1.178/1.152, A/A ±13.00%) and `row_u64/random/48` (1.010 vs A, 1.001 vs B, A/A ±0.36%). Distinct current root bodies prevented isolating address placement; keep acceptance open.

## Operation and parallel baselines

### Port parity baseline

The engine is a Stable Rust port of `sort-research-rs` (`6c0877f0491dc3b8fc052aa529bce44bf8a53fa8`). Inside one Nightly binary of the prototype's harness (fixed seed, one pinned core, rotated order), median `port / prototype` was 0.984–1.004 over 27–136 cases per matrix (ranges 0.53–1.23, placement-dominated) for both `sort` and `sort_by_key` in default and `target-cpu=native`+LTO builds. Outliers stayed within 3% of the prototype forced onto cached bucket IDs (`record128` Zipf 1.3 n = 128: 1.168 vs 1.030), matching the intentional caching policy ([record handling](engine.md#record-handling)).

### Parallel completion baseline

Host: AMD Ryzen 7 8845H, CPUs `0,2,4,6,8,10,12,14` (one SMT sibling per core), `powersave`, boost and SMT on, Rust 1.99.1, release (`compare,rayon`). Whole-call speedups vs serial A/A across 9 blocks (peak heap excludes input, pool and thread stacks):

| 10M input | 8-worker speedup | Block range | Serial A/A | Parallel peak heap |
|---|---:|---:|---:|---:|
| Random `u32` | 6.01× | 4.86–6.62× | 0.981–1.019 | 12,076,304 B |
| Random `{u32, u32}` | 4.58× | 4.32–4.77× | 0.987–1.013 | 18,909,498 B |
| `d256` `u32` | 1.97× | 1.59–2.15× | 0.995–1.005 | 65,536 B |
| `d256` `{u32, u32}` | 1.83× | 1.79–1.89× | 0.976–1.024 | 15,757,602 B |
| Random `u64` | 3.80× | 3.59–4.12× | 0.996–1.004 | 18,275,618 B |
| Random `{u64, u32}` | 2.95× | 2.79–3.17× | 0.994–1.006 | 29,635,068 B |

Fivefold scaling holds for random `u32` only (4M/8M/16M: 4.09×/6.23×/5.56×; 8B rows: 4.76×/4.56×/4.09×). Adaptive single-thread roots did not scale (`zipf2` 0.96–1.00×, `sorted` 0.42–0.89×, `reverse` 0.87–0.94×; `row_u32/sorted` varied 0.42–1.41× across binaries, cause unresolved). At `7ab58c9` (10M random), relocation moved 0.76–0.79× input bytes (~40% re-relocation), lock contention was negligible, scatter reached ~39 GB/s logical traffic (STREAM: 35 GB/s cached, 56 GB/s NT), and cycles split 39% tile sorter (28% L1 store misses on 12-bit second passes), 23% scatter, 26% copies, 10% bucket finishing.

Historical bounded staging vs materialization (block permutation + sequential 7.4 ms compaction chain) at equal root plans (8 workers, 10M random, median ms): `u32` 7.7 vs 11.1 (1.33×, only direction passing both copies), `{u32,u32}` 14.0 vs 16.8 (1.15–1.19×), `u64` 18.3 vs 17.0 (0.97–0.98×), `{u64,u32}` 35.5 vs 31.7 (0.86–0.91×). All four lay inside the [former staging window](https://github.com/Yikai-Liao/tessera-order/blob/76a985fd47275ef60b024d7b0b20c237a1069efb/docs/design/engine.md#bounded-staging); uniform roots above it remain unmeasured.

<a id="sparse-order-as-source-spans"></a>

### Sparse order and stable endpoints

Local Ryzen 7 8845H ratios (reference / Tessera; full-sort reference `ddb2f6d`). Run sparse discovery *after* insertion and *after* parallel scheduling: trying it first on 65,536 1%-adjacent-swap records slowed throughput (0.31× `u64`, 0.68× 16B rows, 0.58× 128B records), and running it before parallel scheduling on 1M distant-swap records slowed 4-worker sorts (0.73×/0.80×/0.96×). On 65,536 random 128B records against `select_nth_unstable_by_key`, bounded ends reached 1.15–1.26× at ranks 1–4 and 0.67× at rank 0, while the median reached 0.48× (`random`) and 0.38× (`d16`); whole-slice tie checks (0.53×) and membership bitmaps (0.43×) still moved payloads stably. Keep the disjointness proof bounded to the end.

In `refine/indices.rs` (stable source permutations), routing packed index words through a fresh root partition was +29.7% slower than `ddb2f6d` on 65,536 random `u64`, while bare indices without packing dropped 1M random 128B argsort from 6.80× to 3.68× vs `slice::sort` index sorting; direct sparse span application raised 65,536-record 99%-skew wide-record cached-key sorting from 0.87× to 2.70× vs `slice::sort_by_cached_key`.

#### Stable endpoints

For rank 0 or `len - 1`, one forward scan tracking both the winner and whether the displaced endpoint has a peer avoids a second scan, block metadata and heap allocation (0 extra heap across all 24 screened cases). On 65,536 random 128B records, two-pass min/max search reached 0.65×/0.64× and general bounded-end permutation 0.77×/0.73× of unstable stdlib selection, vs 1.04×/0.95× for the single scan ([issue #242](https://github.com/Yikai-Liao/tessera-order/issues/242)). Cross-build Ryzen 7 8845H comparison (`ce88a8f` → single scan, unstable stdlib / stable Tessera, 9-block A/A in brackets):

| Random records | First rank | Last rank |
| --- | --- | --- |
| `u64` | 0.33× → 0.50× [0.98, 1.02] | 0.51× → 0.70× [0.77, 1.23] |
| 16-byte row | 0.62× → 0.90× [0.86, 1.14] | 0.43× → 0.68× [0.71, 1.29] |
| 128-byte record | 0.68× → 1.04× [0.96, 1.04] | 0.71× → 0.95× [0.76, 1.24] |

Reverse and `d16` endpoints improved across all three widths; longer-sample repeats reversed initial last-rank slowdowns on `row/distant-swaps` (−9%) and `u64/random` (−31%), while `u64/distant-swaps` first (+8%) was not repeated. Full-sort A/A controls stayed neutral (widest A/A 0.57–1.43), leaving small costs unresolved.

<a id="serial-slice-operation-baseline"></a>
<a id="stable-category-partition-baseline"></a>

### Serial slice-operation and category baselines

These screens cover the pre-0.2 development surface (including operations [removed before 0.2](operation-extensions-proposal.md#release-surface-alignment)) and predate `refine/indices.rs`; current comparisons live on the [implementations page](../benchmarks/implementations.md).

- **Pre-0.2 serial slice operations (65,536 and 262,144 records; `u64`, 16B rows, 128B records × `random`/`d16`/`zipf2`; 9 paired blocks):** random partial workloads saved substantial work against stable full-sort references (`slice::sort`, Glidesort, Radsort), while native unstable `select_nth_unstable` beat stable selection on every measured Top-k/prefix case and most single ranks (weaker tie contract). Stable-opponent losses were concentrated in `zipf2` (argsort 0.83–0.99× vs `slice::sort`, segments/multiple-selection 0.79–0.86× vs `slice::sort`) and 128B segments vs Radsort (0.90–0.98×).
- **Removed category partition entries (i7-10875H, 3 blocks, 4,096 and 65,536 records, 2/7/257 categories):** dedicated category movement beat sorting compositions on 117/126 pairs; all 9 losses were constant-category 128B records (+5.7% to +37.5% vs `slice::sort`/Glidesort, +1.5% vs Radsort), where distribution still moved records even when all keys fell in one bucket (isolated contribution unmeasured). At 257 categories and 65,536 `u64`, peak extra heap reached 1.18 MB (> 524 KB input) from block floors.
- **Cached-key range routing vs removed count-taking partition (Xeon Gold 6140, 3 blocks):** routing `sort_by_cached_key` by key *type* paid for the type's full range (`u8` always allocated 256-bucket floors: 0.60–2.25 MB vs 27–211 KB; `u16` always fell back to index pairs). Routing by the cached keys' *actual varying bits* scaled heap with observed categories and finished constant inputs in 0.47–0.87×, trading 0.84–2.23× at two categories and 1.36–2.12× at 7–257 categories on nonconstant grouping (partly from the reference's second classification to build boundaries) for removing the count-taking public API.

## Structure analysis

Compare constructions by their rules, states and invariants, not source length. A replacement must satisfy [semantics](../../conformance/semantics.md) and the [per-case acceptance rule](../../ROADMAP.md#simplification-candidates). The maintainer-approved study ceiling is O(P^{3/2}√n) with fixed root fan-out R (excluding an n-record buffer); the public [O(P√n) bound](../../conformance/semantics.md#rust-rayon-entry-points) changes only with an explicit contract decision.

### Mechanism audit

Ownership is concentrated rather than proven minimal (paths under `rust/tessera-order/src/engine/`):

| Change | Authority and invariant | Coordinated docs/model edits |
|---|---|---|
| Sampling | `refine/plan.rs::KeySample` advises equality/occupancy (never certifies ownership/progress); `refine.rs::probe_order` samples adjacent order. | `explain/routing.md`, `sim/digits.ts` |
| Tile capacity | `tile.rs::capacity` and `finishing_capacity` derive counts from byte and `Cursor` budgets. | `explain/tiles.md`, `sim/digits.ts` |
| Partition selection | `plan_node` feeds serial `execute_node` and parallel `sort_root`; both classify with `KeyDigit`/`Rank` and `Split::equality`, consuming the top varying bit. | `explain/routing.md`, recovery/inconsistent-key tests |
| Block layout | `block.rs` owns chains, block sizing (`2R` reserve per shard) and scatter recovery; `block/finish.rs` owns descriptor costs and one source ledger for serial and parallel completion. | `explain/block-partition.md` (replayed via `trace.rs`) |
| Task size | `refine/parallel.rs` owns cutoff, shard quota and scratch pool; `block/parallel.rs` clamps shards and runs bucket finishing. Reentrant callbacks forbid assuming one live buffer per worker. | `design/engine.md`, parallel/memory tests |

| Structural alternative | Mechanism trade-off and disposition |
|---|---|
| **C: Direct source ledger** | **Chosen ([#120](https://github.com/Yikai-Liao/tessera-order/pull/120), [#154](https://github.com/Yikai-Liao/tessera-order/pull/154)).** Retires parallel permutation/compaction recovery and private staging within O(P√n); keeps source-protection metadata. See [shared ledger study](#shared-ledger-study). |
| **A: Independent packing** | **Rejected ([#117](https://github.com/Yikai-Liao/tessera-order/pull/117)).** Removes owner locks/free caches but adds O(P^{3/2}√(Rn)) snapshots, keeps permutation recovery, and lost up to +26.9% in 4-thread screens. |
| **B: Scattered completion** | **Modeled rejection.** Gather–append–compact adds a third full-record transfer and a post-join sequential copy ([`scatteredCompletion.test.ts`](../.vitepress/theme/sim/scatteredCompletion.test.ts)). Other scattered forms remain unexcluded. |
| **Fused order/variation scan** | **No saving-preserving construction.** Must keep masked early exit, constant vector reduction, ascending SIMD and strict-descent savings; separate modes restore current mechanisms. |
| **Block passes for tiles** | **Not prototyped.** Block scatter+materialize adds packing over alternating tile buffers and lacks an LSD replacement. |

<a id="post-ledger-simplification-proposal"></a>

### Post-ledger simplification

Evaluated against the merged source-ledger engine; [ROADMAP.md](../../ROADMAP.md#simplification-candidates) tracks remaining acceptance work.

| ID | Scope | Status and conclusion |
|---|---|---|
| S1 | One explicit `fearless_simd` direct scan & non-dispatching insertion scan | **Done & accepted** ([#126](https://github.com/Yikai-Liao/tessera-order/pull/126), [#128](https://github.com/Yikai-Liao/tessera-order/pull/128)); see [above](#shared-explicit-direct-key-scan). |
| P1 | Retire private staging; stream with the bounded [whole-child tile allowance](../explain/tiles.md#parallel-child-allowance) | **Implemented ([#154](https://github.com/Yikai-Liao/tessera-order/pull/154)); acceptance pending.** Initial hosted screens ([#152](https://github.com/Yikai-Liao/tessera-order/pull/152)/[#153](https://github.com/Yikai-Liao/tessera-order/pull/153)) vs `76a985f` showed 4T `u64/random/4M` +6.6% ordinary vs −5.0% extended, and exposed that enlarging *descendant* tiles regressed `u64/bimodal/1M` (+27.4%), motivating the whole-child-only allowance. |
| N1/N3/N4 | Shared `Classify` (`KeyDigit`, `Rank`) and `Split` completion | **Done** ([#158](https://github.com/Yikai-Liao/tessera-order/pull/158)). Keeping classification state owned by scatter and full-width `Rank` arithmetic avoids the borrowed-extractor and partial-register hot-loop slowdowns seen before #158. |
| N5 | Shared node and tile recursion | **Stopped after tile child unification ([#145](https://github.com/Yikai-Liao/tessera-order/pull/145), −0.1% hosted).** Unifying `sort_child` with `tile::Scratch::finish_child` would either add order-confirmation scans to every MSD bucket or leak tile scratch/pointers across the tile boundary. |
| N6 | Equality digit consumes the top varying bit | **Implemented; timing acceptance pending.** See below. |
| T2 | MSD-only tile finishing | **Rejected** ([#138](https://github.com/Yikai-Liao/tessera-order/pull/138)); see [above](#rejected-or-on-hold). |
| X1/X2 | Reconstruct direct keys from counts; single `Extract::DIRECT` fact | **Done** ([#146](https://github.com/Yikai-Liao/tessera-order/pull/146), −0.3% hosted; solo `u64/random/1000` +7.4% alert unreproduced locally or on rerun). See below. |
| P4 | In-place `Partition` directory; retire `ends` pool | **Done** ([#144](https://github.com/Yikai-Liao/tessera-order/pull/144), −0.3% hosted). Returning `Vec<usize>` avoids stacking parent+child block scratch during compaction callbacks. |
| P5 | Recursive parallel nodes | **Deferred.** Resource model below; requires simplification payoff before implementation. |

#### Equality digit consumes the top bit

Previously, equality splits left outer bands with all candidate bits, allowing consistent keys (placing a new minimum on sampled indices) or changing keys to peel a slice to stack-overflow depth (`peeled_equality_chains_stay_shallow`). `key::Rank` now partitions into four buckets split at both the sampled key and the highest varying bit's boundary: every unfinished bucket holds the top bit constant, bounding node depth and retained boundary vectors by key width for any extractor, retiring `plan_after_equality` and parallel equality chains, and supplying the continuation bound required by P5. The extra comparison and fourth bucket remain pending timing acceptance on the [roadmap](../../ROADMAP.md#simplification-candidates).

#### X1/X2: Reconstruct direct keys from counts

Sealed `RadixKey` encodings are bitwise bijections (preserving NaN payloads, signed zero, `Reverse` and omitted `KeyHalf` halves). `KeyCounts` reconstructs each bucket's key from one encoded key's shared outside bits plus the bucket digit, so parallel counting reduces histograms only. `Extract::DIRECT` couples uncached tile scatter with serial/parallel count-fill eligibility through one sealed unsafe fact, preventing disagreement; widening the existing counting-window width requires a separate budgeted routing experiment (table init, reduction and O(P·2^width) space).

#### P5: Recursive parallel nodes

A restricted schedule stays within O(P√n) space: finish all siblings, join all parent jobs and release every parent capacity (`Buffers`, ledger, pool, caches), then tail-continue in parallel on at most one unfinished child (e.g. a strict majority), retaining only range/candidate metadata across the transition. Dropping `Partition` alone is insufficient because capacity stays in `Buffers`. Key-width depth (N6) bounds serial continuations without a tail-recursion invariant; only one parallel phase is live per public invocation, and no lock or permit may span user code. Evaluate skewed critical paths (majority-equal roots, multiple non-majority large children, delayed-child overlap loss, reallocation cost) before prototyping; broader concurrent frontiers need a new space proof.

#### Structural acceptance needs calibration

Distinguishing structural from routing changes guides evidence choice, but fixed-work instruction counts do not establish latency equivalence (they miss cache, vector width, dependency chains, dispatch and contention, and Callgrind's ISA can differ from the host). Equal instruction counts do not waive timing alerts; until a calibrated structural lane is approved, the per-case rule and [source-attribution controls](../development/benchmarking.md#additional-source-attribution-controls) stay in force.

<a id="shared-ledger-study"></a>

### Completion studies

- **Direct source ledger (C, [PR #120](https://github.com/Yikai-Liao/tessera-order/pull/120), accepted):** protects the full destination prefix before resolving its relocatable source, eliminating the first C prototype's per-bucket block temporary and mandatory streaming copy ([PR #118](https://github.com/Yikai-Liao/tessera-order/pull/118), rejected as submitted on process-level 4-thread alerts up to +22.6% on `row_u64/skew99/1M` and +13.4% on `u64/skew99/1M`, without isolating source effects). Retires parallel cycle snapshots and sequential compaction recovery within O(P√n), with the scoped `skew99/1M` exception listed under [open losses](#landed-with-open-losses). Above-window revision timing and placement-controlled skew attribution remain open.
- **Independent packing (A, [PR #117](https://github.com/Yikai-Liao/tessera-order/pull/117), rejected):** permutes blocks then snapshots unread source suffixes per segment before packing (`O(P²Rb) = O(P^{3/2}√(Rn))`, exceeding the public bound). Removed owner locks and free discovery, but kept permutation recovery and triggered process-level 4-thread alerts on wide `sparse_digits/1M` (+26.9%) and `u64/far_swaps/1M` (+15.9%), rejecting the prototype as submitted without ruling out other packing forms. Dependent frontier waiting would reintroduce coordination and deadlock hazards under suspending callbacks.
- **Scattered completion (B, modeled rejection):** per-node arenas risk linear frontier growth; a P-lane gather–append–compact arena retires staging locks but turns direct placement into three full-record transfers plus a post-join sequential materialization ([`scatteredCompletion.test.ts`](../.vitepress/theme/sim/scatteredCompletion.test.ts)). Direct-final scattered variants remain untested.
