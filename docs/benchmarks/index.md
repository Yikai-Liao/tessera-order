---
# Give the interactive figures the full document width, without an outline column.
aside: false
pageClass: performance-page
---

# Performance

<script setup lang="ts">
import ImplementationComparison from "../.vitepress/theme/components/ImplementationComparison.vue";
import PerformanceHistory from "../.vitepress/theme/components/PerformanceHistory.vue";
</script>

In the current measurements:

- **Large random inputs:** Tessera is the fastest stable sort for most key and record types. On `u32` keys, unstable SIMD and radix sorts are faster.
- **Sorted and nearly sorted inputs:** sorted inputs are faster than with `slice::sort`, and nearly sorted ones many times faster.
- **Weak spots:** reversed input, a sorted run followed by a random tail, heavily skewed (Zipf) keys and very short random inputs can be slower than `slice::sort`. [Engine evidence](../design/evidence.md#landed-with-open-losses) tracks these losses.
- **Memory:** a serial `sort` needs extra memory that grows as √n, while `slice::sort`, radsort and glidesort allocate buffers proportional to the input. Parallel sorting, argsort and cached-key sorting have [their own bounds](../../conformance/semantics.md#space).

The [implementation comparison](implementations.md) shows every figure behind these statements.

<ImplementationComparison view="overview" />

## Main history

How long Tessera takes to sort fixed inputs on main, measured on Bencher's Intel v1 Bare Metal runners: whether main is getting faster, which cases moved at which commit, and how long each case takes now.

<PerformanceHistory />

## Using the history

- **Filter** by type, distribution, length and mode; the trend, group means and table follow the filters.
- **All cases** shows one row per case, grouped by the call it measures, relative to its first run in the range: blue is faster, orange slower. Each row has its own scale, so read the size of a change from its value. A step aligned across rows came from one commit.
- **Click a row** to compare up to four cases on one axis, as change in percent or as run time. Pale bars are Criterion's within-run intervals. Hover for details; click a point to open its commit.
- **Axis and range:** reports are ordered by commit time, one slot per report or spaced by time. Presets and dates select benchmark days (UTC); drag across the comparison to zoom. The URL records the whole view, so it can be shared.
- **Gaps** mark reports for a case's execution mode that omitted the case. Each line skips report slots from other execution modes.
- **Latest values** lists each case's latest estimate, interval and change.

## Reading the trend

Each case's baseline is its first report in the selected range, so changing the range can change it. Commit time is not a guaranteed merge order. Changes include compiler and dependency upgrades as well as library changes. Criterion intervals are within-run estimates, not cross-run regression tests, and do not remove code-layout effects or differences between machines of the same specification. Missing measurements are absent, not passes or zeroes.

Every view uses only runs on the latest run's hardware specification within the range, so environments are never mixed; select an earlier range to inspect older environments. Group means chain each case's ratio to its own previous measurement, so a case that joins later does not shift the mean; with an unchanged case set this equals the geometric mean of the cases' changes.

The page reads monthly JSONL from the public data branch and never queries the live Bencher API. Coverage, publication and limits belong to the [main tracking contract](../development/continuous-benchmarks.md#main-long-term-tracking); PR checks use their own paired comparisons.
