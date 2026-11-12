---
# Give the interactive figures the full document width, without an outline column.
aside: false
pageClass: performance-page
---

# Main performance history

<script setup lang="ts">
import PerformanceHistory from "../.vitepress/theme/components/PerformanceHistory.vue";
</script>

This page tracks how long Tessera takes to sort fixed inputs on main, measured on Bencher's Intel v1 Bare Metal runners. It answers three questions: whether main is getting faster overall, which cases moved and at which commit, and how long each case takes now.

<PerformanceHistory />

**Filters.** Type, distribution, length and mode filter the cases independently; within a filter, any selected value matches, and "All" clears it. The trend, its group means and the table cover the matching cases.

**All cases.** The trend has one row per case, grouped into serial and four-thread entry points, with each group led by its geometric mean. A row's line shows its run time relative to its first measurement in the selected range; the dashed line is that reference. Blue segments are faster than it and orange segments slower. Every row has its own vertical scale with a minimum span that keeps run-to-run noise flat, so read the size of a change from the value on the right, the change at the case's latest measurement. Rows share the run positions, so a step that lines up across rows came from the same commit. A gap means the run did not measure that case, for example before it joined the plan. The headline number is the geometric mean over the matching cases.

**Compared cases.** Clicking a row adds the case to the comparison above the trend, or removes it. The comparison scrolls with the page, so it does not obscure the trend rows. Up to four cases share one axis; each case keeps its colour while selected. **Run time** plots absolute run time on a logarithmic axis, so equal heights are equal ratios and cases of different lengths fit together. **Change %**, the default, plots `(run time / baseline − 1) × 100%`: negative is faster, positive is slower. Each case's baseline is its first comparable report in the selected range, in commit timestamp order, not the previous commit. The baseline value, SHA and UTC measurement time appear above the chart. Changing the range can change the baseline; switching either axis does not. A pale bar is Criterion's within-run confidence interval. Hovering shows the SHA, commit and measurement times in UTC, and each case's absolute time, change and baseline. Each point opens its source commit.

**Horizontal axis.** Both charts sort by Git committer timestamp, not author time or benchmark completion time. **Commit**, the default, gives each report an equal-width slot labelled by SHA; reruns of the same SHA stay separate, ordered by measurement time, with report identity breaking exact ties. **Time (UTC)** uses the same records and order but spaces them by elapsed commit time; reports with identical commit timestamps can overlap. Short ranges show time-of-day ticks. Older reports without a commit timestamp fall back to measurement time, with a visible notice. Git timestamps describe the commit, not a guaranteed merge order. The all-case trend always shows relative change, independent of the comparison's vertical-axis setting.

**Measured range and zoom.** Presets and date fields filter benchmark start time, not commit time; dates include their whole UTC day. Monthly downloads follow this measured range even when an older commit was measured later. Dragging across the comparison zooms to an inclusive range of reports on either horizontal axis; **Reset zoom** restores the measured range. Changing a preset or date clears zoom. The compared cases, both axes, filters, measured dates and zoom report identities are part of the URL, so a view can be shared.

**Latest values.** The table lists every matching case's latest estimate, interval and change over the range; a value from an earlier run than the latest shows its date. Its checkboxes compare cases by keyboard, and the figure's description reads each row's latest change.

## Reading the trend

Changes include compiler and dependency upgrades alongside library changes. Criterion intervals are within-run estimates, not cross-run regression tests, and they do not remove code-layout effects or variation between machines with the same specification. Missing measurements are absent, not passes or zeroes.

Relative changes, including the comparison, use only runs on the latest measured run's hardware specification and protocol in the measured range. Within that series, the latest values and baselines follow commit timestamp order. Group means chain each case's ratio to its own previous measurement, so a case that joins the plan later does not shift the mean by its own level. With an unchanged set of cases, this equals the geometric mean of the cases' own changes.

The page downloads monthly JSONL from the public data branch according to the selected range and never queries the live Bencher API. See the [main tracking contract](../development/continuous-benchmarks.md#main-long-term-tracking) for coverage, publication, data access and operational limits. PR checks use their own paired comparisons.

## Embed the tracker

In another Vue-enabled Markdown page, import this component and use it directly:

	<script setup>
	import PerformanceHistory from "./path/to/PerformanceHistory.vue";
	</script>
	<PerformanceHistory />

Optional `source` (a data-directory URL ending in `/`) and `case-id` props select another compatible dataset or the initially compared case; a URL hash with the page's view takes precedence. Ordinary webpages can embed this page with an iframe. GitHub Markdown uses the [README’s fixed SVG preview](../../README.md), which shows only the group means. Publishing measurements does not rebuild this website.
