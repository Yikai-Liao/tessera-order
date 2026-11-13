---
# Give the interactive figures the full document width, without an outline column.
aside: false
pageClass: performance-page
---

# Performance

<script setup lang="ts">
import AlgorithmComparison from "../.vitepress/theme/components/AlgorithmComparison.vue";
import PerformanceHistory from "../.vitepress/theme/components/PerformanceHistory.vue";
</script>

<AlgorithmComparison view="overview" />

## Main history

How long Tessera takes to sort fixed inputs on main, measured on Bencher's Intel v1 Bare Metal runners: whether main is getting faster, which cases moved at which commit, and how long each case takes now.

<PerformanceHistory />

## Using the history

**Filters.** Type, distribution, length and mode filter independently; any selected value matches, and "All" clears a filter. The trend, its group means and the table cover the matching cases.

**All cases.** One row per case, grouped by serial and parallel entry point, each group led by its geometric mean. A row shows run time relative to its first measurement in the range (the dashed line): blue is faster, orange slower. Each row has its own vertical scale with a minimum span that keeps noise flat, so read the size of a change from the value on the right. Rows share run positions, so a step aligned across rows came from one commit; a gap means the run did not measure that case. The headline is the geometric mean over the matching cases.

**Compared cases.** Click a row to add or remove it from the comparison above the trend; up to four cases share one axis and keep their colours. **Change %** (default) plots `(run time / baseline − 1) × 100%`; **Run time** plots absolute time on a log axis. Each case's baseline is its first comparable report in the range, by commit timestamp, and is shown above the chart; changing the range can change it, switching axes does not. Pale bars are Criterion's within-run confidence intervals. Hover for SHAs, times (UTC) and baselines; click a point to open its commit.

**Horizontal axis.** Both charts order reports by Git committer timestamp. **Commit** (default) gives each report an equal slot; reruns of one SHA stay separate, ordered by measurement time. **Time (UTC)** spaces the same reports by commit time, so reports with equal timestamps can overlap. Reports without a commit timestamp fall back to measurement time, with a notice. Commit time is not a guaranteed merge order. The all-case trend always shows relative change.

**Range and zoom.** Presets and dates filter benchmark start time, not commit time, and include whole UTC days; downloads follow this range. Drag across the comparison to zoom to a range of reports; **Reset zoom**, or changing the range, restores it. The URL records cases, axes, filters, dates and zoom, so a view can be shared.

**Latest values.** The table lists each matching case's latest estimate, interval and change; a value older than the latest run shows its date. Its checkboxes select compared cases by keyboard.

## Reading the trend

Changes include compiler and dependency upgrades as well as library changes. Criterion intervals are within-run estimates, not cross-run regression tests, and do not remove code-layout effects or differences between machines of the same specification. Missing measurements are absent, not passes or zeroes.

Every view uses only runs on the latest run's hardware specification within the range, so environments are never mixed; select an earlier range to inspect older environments. Group means chain each case's ratio to its own previous measurement, so a case that joins later does not shift the mean; with an unchanged case set this equals the geometric mean of the cases' changes.

The page reads monthly JSONL from the public data branch and never queries the live Bencher API. Coverage, publication and limits belong to the [main tracking contract](../development/continuous-benchmarks.md#main-long-term-tracking); PR checks use their own paired comparisons.

## Embed the tracker

In another Vue-enabled Markdown page:

	<script setup>
	import PerformanceHistory from "./path/to/PerformanceHistory.vue";
	</script>
	<PerformanceHistory />

Optional `source` (a data-directory URL ending in `/`) and `case-id` props select another compatible dataset or the initially compared case; a URL hash takes precedence. Other webpages can use an iframe. GitHub Markdown uses the [README's fixed SVG preview](../../README.md) of the group means. Publishing measurements does not rebuild this website.
