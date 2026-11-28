---
# Give the interactive history dashboard the full document width, without an outline column.
aside: false
pageClass: performance-page
---

# Main history

<script setup lang="ts">
import PerformanceHistory from "../.vitepress/theme/components/PerformanceHistory.vue";
</script>

How long Tessera takes to sort fixed inputs on `main`, measured on Bencher's Intel v1 Bare Metal runners: whether `main` is getting faster, which cases moved at which commit, and how long each case takes now. For standings and memory comparisons against other Rust and C++ sorting libraries, see [vs Other Sorts](implementations.md).

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

Every view uses only runs on the latest run's hardware specification within the range, so environments are never mixed; select an earlier range to inspect older environments. The filters, trend matrix and latest-values table show only active cases: each case must appear in its execution mode's latest report, and that mode must have reported on either the latest commit or the immediately preceding comparable commit. If a new commit publishes its serial report before its parallel report arrives, the preceding commit's parallel cases stay visible (with their earlier date shown in the latest-values table); once an execution mode publishes a report that omits a case, or stops reporting across commits, those cases are omitted. Group means chain each case's ratio to its own previous measurement, so a case that joins later does not shift the mean; with an unchanged case set this equals the geometric mean of the cases' changes.

The page reads monthly JSONL from the public data branch and never queries the live Bencher API. Coverage, publication and limits belong to the [main tracking contract](../development/continuous-benchmarks.md#main-long-term-tracking); PR checks use their own paired comparisons.
