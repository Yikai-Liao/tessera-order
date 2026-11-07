# Public Rayon count-window: production checkpoint

## Decision

On the unchanged public Pool/Job/root scheduler, the production count-only candidate C establishes a favorable W8 signal against both the same-core public parallel API A and the original strong serial API S in **3/4 n10M inputs**. u32/d256 remains inconclusive against both because its full controls span 0.301142–3.320696. Across W2/W4/W8, 6/12 cells clear controls against each reference. All W1 comparisons and the once-run u32/equal37 sentinel are inconclusive. No comparison establishes a slowdown; this is not equivalence or proof that regressions are absent.

This is bounded evidence for Direct u32/u64 with an eight-bit full varying window. It is not a fullrandom, generic-record or whole-library speedup. The baseline is the user’s public Rayon experiment; the research-combined scheduler was not substituted. The candidate contains no research feature, selector or research TLS path.

All **1,377 samples and 56,715 individual timed sorts** passed complete stable-reference equality. All 51 separate allocator observations retained zero extra heap. All input, source, registry, feature, route and shape checks passed. No timing sample, individual tail or placement was discarded; no replacement run or tuning occurred. Earlier e1 results remain unchanged and are not pooled into this checkpoint.

The root README.md is the immutable pre-execution scaffold plan, not the current completion state. It intentionally retains its historical source-only wording to preserve the source freeze. This REPORT.md and result-packet/IDENTITIES.json are the final completion record; validation and results below supersede that earlier status without changing the frozen plan.

## Variants and protocol

A1/A2 use one identical public parallel-API function pointer; A_copy is an independently placed source-identical package. C1/C2/C_copy do the same for the count-only candidate. S1/S2 call baseline stable_sort; S_copy calls baseline-copy stable_sort. Four distinct Cargo packages keep source placements independent. S is the original serial public API in the same worker context, **not the fixed-geometry same-kernel serial scheduling control**.

The nine-label strengthening was declared before any new build or run. The sentinel is u32/equal37/n1M/W8, nine blocks and 81 samples. It ran once and permitted continuation only because neither comparison established a slowdown. The matrix contains four n10M inputs × W1/2/4/8 × nine blocks × nine labels = 1,296 samples. Input order was u32 hot99, u64 hot99, u64 d256, u32 d256.

Within each input/block, W order is randomized. Each W stratum launches a fresh subprocess with exactly one Rayon pool; that process exits before the next. Labels are randomized within each stratum and primed outside measurement. Cloning happens on the actual pool worker before every individual timer; full stable-reference equality follows every timer. Every sample accumulates at least 50 ms and two fresh sorts. The complete public call, including its preparation and internal scratch allocation, is timed. Process affinity is CPUs 0–7; no worker pins or sleep/spin tuning are used.

The primary statistic is median(C duplicate mean / reference duplicate mean) over nine matched blocks. C/A and C/S each use their own symmetric full envelope of both groups’ A/A and independent-placement ranges. Claims require the median beyond that envelope, at least 8/9 primary blocks agreeing, and at least 8/9 independent-placement comparisons agreeing. All tails count. A confirmed slower comparison would stop before the next input; none triggered that gate.

## Absolute milliseconds per sort

Each entry is the median across nine blocks of the within-block duplicate mean. Ratios below are paired statistics, not ratios of these displayed medians. For n10M, ns/element is the displayed ms divided by 10; for the n1M sentinel it is numerically equal to ms.

| Input | n | W | A public ms | C production ms | S strong serial ms |
|---|---:|---:|---:|---:|---:|
| u32/equal37 | 1000000 | 8 | 0.084757 | 0.074808 | 0.074881 |
| u32/d256_hot99 | 10000000 | 1 | 15.486409 | 15.638721 | 15.951214 |
| u32/d256_hot99 | 10000000 | 2 | 15.922159 | 9.059049 | 15.679538 |
| u32/d256_hot99 | 10000000 | 4 | 15.981920 | 6.179514 | 15.635836 |
| u32/d256_hot99 | 10000000 | 8 | 16.119348 | 5.778520 | 16.177278 |
| u64/d256_hot99 | 10000000 | 1 | 19.209868 | 19.065253 | 18.943697 |
| u64/d256_hot99 | 10000000 | 2 | 18.887911 | 11.457444 | 19.155724 |
| u64/d256_hot99 | 10000000 | 4 | 18.763057 | 9.037906 | 18.708005 |
| u64/d256_hot99 | 10000000 | 8 | 18.998487 | 8.587694 | 19.117289 |
| u64/d256 | 10000000 | 1 | 14.172796 | 14.442163 | 14.223940 |
| u64/d256 | 10000000 | 2 | 13.765331 | 9.337015 | 13.877200 |
| u64/d256 | 10000000 | 4 | 14.076565 | 8.542893 | 13.716872 |
| u64/d256 | 10000000 | 8 | 14.162656 | 8.635312 | 14.113539 |
| u32/d256 | 10000000 | 1 | 10.706055 | 10.720049 | 10.829755 |
| u32/d256 | 10000000 | 2 | 11.166848 | 7.601712 | 11.200568 |
| u32/d256 | 10000000 | 4 | 10.678072 | 6.005092 | 11.185997 |
| u32/d256 | 10000000 | 8 | 11.467416 | 5.158540 | 11.124116 |

## Paired comparisons, with full decision controls

F = supported favorable signal; I = inconclusive. Direction columns count favorable primary and independent-placement blocks. A favorable-looking median alone does not override broad controls.

| Input | W | Comparison | Paired median [range] | Full symmetric envelope | Favorable primary / placement | Result |
|---|---:|---|---|---|---:|---|
| u32/equal37 | 8 | C/A | 0.889269 [0.816796–0.920464] | 0.857146–1.166663 | 9/9 ; 6/9 | I |
| u32/equal37 | 8 | C/S | 1.002487 [0.958205–1.041862] | 0.823095–1.214926 | 4/9 ; 5/9 | I |
| u32/d256_hot99 | 1 | C/A | 1.007777 [0.947204–1.070549] | 0.839080–1.191781 | 3/9 ; 4/9 | I |
| u32/d256_hot99 | 1 | C/S | 0.981988 [0.913606–1.072922] | 0.907843–1.101512 | 7/9 ; 7/9 | I |
| u32/d256_hot99 | 2 | C/A | 0.548807 [0.532941–1.189198] | 0.341957–2.924341 | 8/9 ; 9/9 | I |
| u32/d256_hot99 | 2 | C/S | 0.573318 [0.530353–1.221115] | 0.341957–2.924341 | 8/9 ; 9/9 | I |
| u32/d256_hot99 | 4 | C/A | 0.393605 [0.374124–0.669368] | 0.422534–2.366673 | 9/9 ; 9/9 | F |
| u32/d256_hot99 | 4 | C/S | 0.397278 [0.371563–0.674490] | 0.422534–2.366673 | 9/9 ; 9/9 | F |
| u32/d256_hot99 | 8 | C/A | 0.342416 [0.313318–0.684230] | 0.765654–1.306072 | 9/9 ; 9/9 | F |
| u32/d256_hot99 | 8 | C/S | 0.346124 [0.305324–0.681780] | 0.765654–1.306072 | 9/9 ; 9/9 | F |
| u64/d256_hot99 | 1 | C/A | 0.980501 [0.888870–1.102900] | 0.844371–1.184314 | 5/9 ; 5/9 | I |
| u64/d256_hot99 | 1 | C/S | 0.997519 [0.960653–1.074788] | 0.844371–1.184314 | 6/9 ; 5/9 | I |
| u64/d256_hot99 | 2 | C/A | 0.600466 [0.564712–0.804102] | 0.679428–1.471826 | 9/9 ; 9/9 | F |
| u64/d256_hot99 | 2 | C/S | 0.601062 [0.532323–0.793775] | 0.679428–1.471826 | 9/9 ; 8/9 | F |
| u64/d256_hot99 | 4 | C/A | 0.482422 [0.451255–0.668018] | 0.481923–2.075020 | 9/9 ; 9/9 | I |
| u64/d256_hot99 | 4 | C/S | 0.497764 [0.440562–0.687584] | 0.481923–2.075020 | 9/9 ; 9/9 | I |
| u64/d256_hot99 | 8 | C/A | 0.453270 [0.409858–0.498931] | 0.866961–1.153455 | 9/9 ; 9/9 | F |
| u64/d256_hot99 | 8 | C/S | 0.448966 [0.414715–0.465420] | 0.753105–1.327835 | 9/9 ; 9/9 | F |
| u64/d256 | 1 | C/A | 0.999370 [0.790026–1.089242] | 0.772767–1.294051 | 5/9 ; 2/9 | I |
| u64/d256 | 1 | C/S | 1.027089 [0.812105–1.087358] | 0.865731–1.155093 | 3/9 ; 2/9 | I |
| u64/d256 | 2 | C/A | 0.673339 [0.563166–0.930222] | 0.575159–1.738648 | 9/9 ; 8/9 | I |
| u64/d256 | 2 | C/S | 0.661867 [0.619006–0.894068] | 0.575159–1.738648 | 9/9 ; 8/9 | I |
| u64/d256 | 4 | C/A | 0.606261 [0.557513–0.742859] | 0.617392–1.619715 | 9/9 ; 9/9 | F |
| u64/d256 | 4 | C/S | 0.614151 [0.572132–0.761373] | 0.617392–1.619715 | 9/9 ; 8/9 | F |
| u64/d256 | 8 | C/A | 0.640010 [0.529288–0.743814] | 0.835980–1.196201 | 9/9 ; 9/9 | F |
| u64/d256 | 8 | C/S | 0.632252 [0.542094–0.796942] | 0.835980–1.196201 | 9/9 ; 9/9 | F |
| u32/d256 | 1 | C/A | 1.000661 [0.856726–1.089117] | 0.904004–1.106190 | 4/9 ; 6/9 | I |
| u32/d256 | 1 | C/S | 0.987035 [0.925719–1.042653] | 0.874008–1.144154 | 6/9 ; 6/9 | I |
| u32/d256 | 2 | C/A | 0.697250 [0.530815–1.295512] | 0.518667–1.928021 | 8/9 ; 9/9 | I |
| u32/d256 | 2 | C/S | 0.722739 [0.565787–1.250907] | 0.518667–1.928021 | 8/9 ; 9/9 | I |
| u32/d256 | 4 | C/A | 0.550899 [0.468514–0.824317] | 0.392322–2.548927 | 9/9 ; 9/9 | I |
| u32/d256 | 4 | C/S | 0.553600 [0.473019–0.836016] | 0.392322–2.548927 | 9/9 ; 9/9 | I |
| u32/d256 | 8 | C/A | 0.438753 [0.397759–1.326011] | 0.301142–3.320696 | 8/9 ; 9/9 | I |
| u32/d256 | 8 | C/S | 0.461149 [0.386562–1.286521] | 0.301142–3.320696 | 8/9 ; 9/9 | I |

The u64/hot99 W4 C/A median 0.482422 is just above its 0.481923 lower envelope, so it remains inconclusive. It is not rounded into acceptance. u32/hot99 W2 and all multiworker u32/d256 cells retain broad candidate controls, including the final W8 envelope reaching 3.320696.

The equal37 sentinel is also inconclusive: A/C/S = 0.084757/0.074808/0.074881 ms; C/A = 0.889269 with 9/9 favorable primary but only 6/9 favorable placement blocks, and C/S = 1.002487 with 4/9 favorable primary blocks. Its controls do not establish either gain or slowdown. It is a new-core single screen, not a rerun pooled with the earlier e1 slower tendency.

## Complete raw A/A and placement ranges

A/A is second same-function label / first label. Placement is copy / same-function duplicate mean. C/A uses A and C controls; C/S uses C and S controls.

| Input | W | A A/A | C A/A | S A/A | A placement | C placement | S placement |
|---|---:|---|---|---|---|---|---|
| u32/equal37 | 8 | 0.932673–1.115200 | 0.953050–1.052286 | 0.907811–1.052716 | 0.917714–1.079339 | 1.083507–1.166663 | 1.041810–1.214926 |
| u32/d256_hot99 | 1 | 0.981000–1.063652 | 0.938129–1.037785 | 0.907843–1.044476 | 0.955886–1.191781 | 0.953312–1.039485 | 0.927081–1.030690 |
| u32/d256_hot99 | 2 | 0.923766–1.046301 | 0.341957–1.097880 | 0.964878–1.023638 | 0.970649–1.055039 | 0.469023–1.192100 | 0.954703–1.082245 |
| u32/d256_hot99 | 4 | 0.848001–1.039465 | 0.422534–1.128911 | 0.885092–1.121202 | 0.936848–1.025577 | 0.893299–1.293504 | 0.961317–1.094146 |
| u32/d256_hot99 | 8 | 0.910430–1.102754 | 0.872297–1.306072 | 0.804498–1.102074 | 0.928652–1.131905 | 0.836318–1.267837 | 0.872778–1.193955 |
| u64/d256_hot99 | 1 | 0.852292–1.120267 | 0.964656–1.122381 | 0.882307–1.069983 | 0.866400–1.018714 | 0.926127–1.184314 | 0.950453–1.080541 |
| u64/d256_hot99 | 2 | 0.809030–1.058357 | 0.878988–1.235805 | 0.906961–1.105346 | 0.906860–1.352787 | 0.940557–1.471826 | 0.902561–1.069674 |
| u64/d256_hot99 | 4 | 0.954301–1.045939 | 0.481923–1.444461 | 0.872915–1.029038 | 0.961458–1.276252 | 0.651248–1.371337 | 0.934894–1.114997 |
| u64/d256_hot99 | 8 | 0.901776–1.062426 | 0.878149–1.089440 | 0.753105–1.184581 | 0.952657–1.142879 | 0.947397–1.153455 | 0.807659–1.012041 |
| u64/d256 | 1 | 0.772767–1.129752 | 0.939015–1.064719 | 0.903847–1.155093 | 0.791162–1.141939 | 0.904654–1.138474 | 0.928829–1.011176 |
| u64/d256 | 2 | 0.915565–1.032372 | 0.575159–1.004256 | 0.892554–1.126035 | 0.853901–1.065392 | 0.893501–1.561185 | 0.921695–1.144527 |
| u64/d256 | 4 | 0.862882–1.091969 | 0.617392–1.121078 | 0.958642–1.093892 | 0.969063–1.096173 | 0.916209–1.318177 | 0.927574–1.077998 |
| u64/d256 | 8 | 0.867817–1.148319 | 0.930446–1.182775 | 0.951576–1.153162 | 0.936282–1.059837 | 0.835980–1.165155 | 0.892597–1.030798 |
| u32/d256 | 1 | 0.957084–1.082712 | 0.935431–1.073555 | 0.874008–1.057813 | 0.904004–1.086680 | 0.942742–1.081224 | 0.958372–1.052045 |
| u32/d256 | 2 | 0.897145–1.642295 | 0.630463–1.247938 | 0.886643–1.236397 | 0.815385–1.040682 | 0.518667–1.229607 | 0.962185–1.270018 |
| u32/d256 | 4 | 0.958967–1.102036 | 0.669057–2.548927 | 0.755623–1.208534 | 0.937228–1.086211 | 0.597282–1.175093 | 0.865414–1.101944 |
| u32/d256 | 8 | 0.744256–1.154046 | 0.301142–1.431743 | 0.881382–1.239426 | 0.918056–1.254776 | 0.409133–1.227225 | 0.885449–1.243434 |

## Keep the two speed-ratio summaries distinct

The ratio of displayed median A or S totals to C differs from the reciprocal of the median paired C/reference ratios. They answer different summary questions and are not interchangeable. The decision rule uses paired ratios, with controls.

| n10M input, W8 | Displayed median A/C | Reciprocal paired C/A median | Displayed median S/C | Reciprocal paired C/S median |
|---|---:|---:|---:|---:|
| u32/d256_hot99 | 2.789529× | 2.920420× | 2.799554× | 2.889141× |
| u64/d256_hot99 | 2.212292× | 2.206190× | 2.226126× | 2.227339× |
| u64/d256 | 1.640086× | 1.562476× | 1.634398× | 1.581648× |
| u32/d256 | 2.222996× | 2.279184× | 2.156446× | 2.168497× |

Only the first three rows establish gains under controls; the u32/d256 ratios remain descriptive despite their favorable appearance.

## Public-policy scaling, not same-kernel scaling

Each ratio is median(C at W / C at W1) within matched outer blocks. Worker-specific C A/A and placement uncertainty is included in the displayed envelope. W1 is positively witnessed taking the original serial fallback, while W2/4/8 invoke the new count kernel. These curves therefore combine policy, kernel and worker-context effects. They are not a pure thread-scalability proof, and no linear or monotonic scaling claim is made.

| Input | W | C(W)/C(W1), median [range] | Combined C control envelope |
|---|---:|---|---|
| u32/d256_hot99 | 2 | 0.580454 [0.544030–1.140969] | 0.341957–2.924341 |
| u32/d256_hot99 | 4 | 0.401194 [0.370053–0.669837] | 0.422534–2.366673 |
| u32/d256_hot99 | 8 | 0.373733 [0.321267–0.705259] | 0.765654–1.306072 |
| u64/d256_hot99 | 2 | 0.584479 [0.542498–0.843881] | 0.679428–1.471826 |
| u64/d256_hot99 | 4 | 0.485222 [0.409455–0.654065] | 0.481923–2.075020 |
| u64/d256_hot99 | 8 | 0.456393 [0.404598–0.532495] | 0.844371–1.184314 |
| u64/d256 | 2 | 0.671203 [0.566971–0.826042] | 0.575159–1.738648 |
| u64/d256 | 4 | 0.602041 [0.524404–0.798265] | 0.617392–1.619715 |
| u64/d256 | 8 | 0.597924 [0.502203–0.850731] | 0.835980–1.196201 |
| u32/d256 | 2 | 0.711148 [0.598358–1.324644] | 0.518667–1.928021 |
| u32/d256 | 4 | 0.552878 [0.471883–0.848367] | 0.392322–2.548927 |
| u32/d256 | 8 | 0.458098 [0.411511–1.421879] | 0.301142–3.320696 |

A and S worker-context curves are retained with the complete paired data. For example, u64/d256 C medians are 9.337015/8.542893/8.635312 ms at W2/4/8; W8 is not automatically a further improvement over W4.

## Exact full-sort heap observations

The following rows exactly summarize all 51 observed A1/C1/S1 allocator runs. All four n10M inputs have identical reported memory counts at a given variant/W; this equality was checked, not assumed. Every row was full-reference verified and retained zero heap. Counts exclude input/reference construction, the fresh clone and pool creation, but include internal public-call scratch allocation.

| Inputs | W | Variant(s) | Observations | Peak extra bytes | Total requested bytes | Allocation calls | Retained bytes |
|---|---|---|---:|---:|---:|---:|---:|
| u32/equal37/n1M | 8 | A, C, S | 3 | 0 | 0 | 0 | 0 |
| All four n10M inputs | 1/2/4/8 | A, S | 32 | 65536 | 65536 | 1 | 0 |
| All four n10M inputs | 1 | C | 4 | 65536 | 65536 | 1 | 0 |
| All four n10M inputs | 2 | C | 4 | 65536 | 71688 | 3 | 0 |
| All four n10M inputs | 4 | C | 4 | 65536 | 75784 | 3 | 0 |
| All four n10M inputs | 8 | C | 4 | 65536 | 83976 | 3 | 0 |

C adds 6,152/10,248/18,440 requested bytes at W2/4/8. At W8 A/C/S all have the same observed 65,536-byte peak and zero retained heap, while C requests 83,976 bytes cumulatively rather than 65,536. No phase trace was collected: equal peaks do not establish allocation lifetimes or identify the cause of a peak.

The new kernel has no n-record output scratch. Its requested histogram/prefix vectors use (P×R + R + 1)×sizeof(usize), R≤256. Admission additionally accounts a conservative 4,096×P task/frame allowance and bounds the accounted total by 8×P×ceil(sqrt(n))×record_bytes. This is the frozen source admission check, not a measurement of Rayon stacks. Fixed stack use, allocator overhead, RSS and peak per-thread heap were not measured here.

## Exact vectors and route evidence

SplitMix64 seed1 generates all vectors. d256 draws next()&255. d256_hot99 contains exactly 99% zero and 1% nonzero draws 1+next()%255, shuffled by Fisher–Yates using the continued stream. Both move existing [255,0,1] to the first three positions without changing the multiset, guaranteeing a non-flat first probe and a full eight-bit varying window. These are the same exact diagnostic vectors used earlier, but their timings are independent. d256_hot99 is not the standard full-width ten-value skew99 distribution. equal37 is exactly one million u32 values equal to37.

| Input | n | FNV-1a of native-width little-endian keys |
|---|---:|---|
| u32/equal37 | 1000000 | 99b1063bc410ad25 |
| u32/d256_hot99 | 10000000 | f6fe43a8cddc8ae4 |
| u64/d256_hot99 | 10000000 | 558a5e14eed7a934 |
| u64/d256 | 10000000 | fa9d13862cbe1a44 |
| u32/d256 | 10000000 | 2e2841855d838d04 |

Diagnostic-only copies positively witness A’s original serial count-fill route in all narrow inputs, C’s admitted count route at W2/4/8, the original serial fallback at W1, and the equal early returns. S’s original serial route is independently observed. C has P histogram rows, R256 buckets and P count plus P fill tasks, each exactly 10,000,000/P records. Actual task-to-worker mappings and allocation mod64 are recorded, not presumed fixed. Histogram sharing/alignment was not tuned or measured as a cause of performance tails.

The unchanged public Pool, Job, bucket-provenance and root scheduling source remains present; the candidate merely checks its count hook before the original narrow fallback. No generic scheduler replacement or new unsafe was introduced. A/C/S route observations total51, candidate geometry observations17. The uninstrumented binaries additionally pass153 full-label warmup references and17 negative mutating-validator checks. All51 heap observations pass. The timing binary has neither allocator counters nor test-only route tracing. Actual sort outputs, not only hashes, are compared with the complete stable reference.

## Tails retained

| Input | Samples | Individual timed sorts | Individual sort ms range, all labels |
|---|---:|---:|---|
| u32/equal37 | 81 | 50610 | 0.064487–3.160067 |
| u32/d256_hot99 | 324 | 1568 | 4.120970–46.951665 |
| u64/d256_hot99 | 324 | 1184 | 7.222480–40.643475 |
| u64/d256 | 324 | 1439 | 6.974515–36.069838 |
| u32/d256 | 324 | 1914 | 3.750490–107.027623 |

Every individual duration, block, label, placement, random order and full-reference result remains in the private raw evidence. JSON round trips preserve elapsed nanoseconds, per-sort duration sums and counts. Broad controls are unresolved uncertainty, not permission to remove a tail or declare a regression harmless.

## Actual lock windows and validation

- Final source safety gates completed before this screen: public baseline22 unit+23 integration and candidate32 unit+23 integration with rayon+portable-simd; default backend17/27 unit+23 integration; plain default11 each and no-default8 each. Clippy-Dwarnings passed. MSRV1.85.1 check was compilation, not execution, and excluded portable SIMD. Fresh strict Miri covered six focused scalar/bucket-borrow tests, not real Rayon pool execution. These are the source owner’s completed paired gates, verified against the exact final source hashes.
- Selective harness release build held shared FD9 from16:08:59 to16:09:42 UTC. Four runtime placements resolved to the exact frozen registry records; only the two selected binaries were built. Diagnostic-only builds held shared FD9 from16:12:11 to16:12:21. Focused diagnostic warnings from disabled unrelated fixtures are preserved; no new complete runtime-suite claim is made for these copies.
- All17-cell route/oracle/heap preflight held shared FD9 from16:15:18 to16:16:04 UTC. All checks passed before any timing. The driver/source/script/diagnostic freeze did not change afterward.
- The sentinel queued at16:16:52 UTC but did not acquire exclusive access until16:18:44, after the separate video renderer released its shared lock. No performance sample was taken during that wait or render. The cooperative lock serialized heavy local jobs; wider machine variability remains reflected in the controls.

| Input/window | Actual exclusive acquisition UTC | Actual release UTC |
|---|---|---|
| u32/equal37 | 2026-11-07T16:18:44Z | 2026-11-07T16:19:00Z |
| u32/d256_hot99 | 2026-11-07T16:19:21Z | 2026-11-07T16:20:36Z |
| u64/d256_hot99 | 2026-11-07T16:21:05Z | 2026-11-07T16:22:42Z |
| u64/d256 | 2026-11-07T16:23:06Z | 2026-11-07T16:24:49Z |
| u32/d256 | 2026-11-07T16:25:19Z | 2026-11-07T16:26:37Z |

The whole shell held FD9 on the existing device27/inode1578019 lock. Every timing window released immediately before analysis. There were153 fresh timing processes:9 sentinel plus4×36 matrix strata. No timing, extra case, tuning, cleanup or rerun followed the final16:26:37 release.

## Build, source and binary identities

Rust1.99.1 (58cf8f9c3), Cargo1.99.1, LLVM23.1.1; Host identified by /proc/cpuinfo as AMD EPYC9V74. Rayon1.12.0, rayon-core1.13.0, fearless_simd1.2.0 and all retained registry package records match both frozen sources exactly. Actual Fearless detected and selected levels are AVX512 in all timing processes. This records scan dispatch, not a claim that every kernel operation is vectorized. lscpu could not read the absent /sys/devices/system/cpu/possible file; /proc/cpuinfo and process affinity metadata were preserved instead.

The four package feature sets are exactly default/std/rayon/portable-simd. Production has no count-window research feature or selector. Default release optimization with debuginfo, no target-cpu/native, LTO or RUSTFLAGS override. TESSERA_* and relevant Rust/Rayon overrides were cleared and logged. Builds were offline, -j1, CARGO_INCREMENTAL=0, reusing the existing target without copying or deleting it. The last observed free disk was421MiB, above the reserved150MiB.

- Public baseline commit:987fbdfe757cc81330db04ee11ed6e454b2e5142; tree1a45e496ce7cd0a965b4851f08e002f5f3ed3796.
- Shared main runtime core:231e1f360a45647d98809539b1a779a62770f88c. A later report-only public commit is not a different runtime baseline.
- Baseline full source identity:19659df2fd57dfcde3b82892bb01b84a6a5770cadd5094bb9815a6cfd70d9f17.
- Candidate full source identity:b4c51ddca807a94260e6399800f17f06b4463fde6a4162f07a9f6df094d84f45.
- Source identities hash json.dumps(files, sort_keys=True) with the owner’s default separators. Complete23/26 file sets and every file hash match; no mutable checkout was substituted.
- Reviewed source diff SHA-256:4acd02044b9d26a03ddded1af04b1a92219b723b732c5c4c9dc56061be9bdf92.
- Driver/script/diagnostic manifest SHA-256:2e019b0358de1328c6eea82f5eeb4399ebe8f61866beb1050267dffedeaaf248.
- Timing binary SHA-256:546c213822516ba915a07f3dbd5a650d0bd12d75d82e5a25d6b239a94a6486c6.
- Allocator binary SHA-256:5ddbaa9bccf98337c32b81d8079034afdb88798b004e783c3c07b62d5fa71fa0.

The final owner-manifest field names and identity serialization differed from the earlier source-only template. That schema was explicitly adapted and pinned before materialization; the old draft and adaptation record are retained. No algorithm or input changed. Earlier frozen sources, reports, packets and seven binaries remain byte-for-byte unchanged. The new publication packet contains only this report, identities and checksums; raw timing data, harness and binaries remain private.

## Limits and next boundary

Only these17 cells, one seed, one machine and the selected lengths were measured. u32/d256 and the equal37 sentinel remain inconclusive. Multiworker cells with broad controls remain inconclusive even when their medians are lower. The measurements are full-API effects: serial preparation and the changed count/fill implementation remain included, so C/A or C/S gain is not wholly attributable to thread scheduling. There is no new same-kernel serial control in this public checkpoint.

Fullrandom, standard wide-skew99, positive u128/Narrow forwarding, ByKey/owned records, ordered distributions beyond the single equal sentinel, other windows/sizes/seeds/platforms and regression behavior outside the admitted narrow route remain unmeasured in this public-core screen. No runtime adoption, source edit, commit or push was performed by this benchmark worker.
