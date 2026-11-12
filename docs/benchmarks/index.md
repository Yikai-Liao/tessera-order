---
# The figures are drawn at their natural width; the outline column would shrink them.
aside: false
---

# Main performance history

<script setup lang="ts">
import PerformanceHistory from "../.vitepress/theme/components/PerformanceHistory.vue";
</script>

This page tracks how long Tessera takes to sort fixed inputs on main, measured on Bencher's Intel v1 Bare Metal runners. It answers three questions: whether main is getting faster overall, which cases moved and at which commit, and how long each case takes now.

<PerformanceHistory />

**All cases.** The first figure has one row per case, grouped into serial and four-thread entry points, with each group led by its geometric mean. A row's line shows its run time relative to its first measurement in the selected window; the dashed line is that reference. Blue segments are faster than it and orange segments slower. Every row has its own vertical scale with a minimum span that keeps run-to-run noise flat, so read the size of a change from the value on the right, the change at the case's latest measurement. Rows share the run positions, so a step that lines up across rows came from the same commit. A gap means the run did not measure that case, for example before it joined the plan. The headline number is the geometric mean over all cases.

**One case.** Selecting a row, or a case in the table, shows that case's absolute time per run, with Criterion's within-run confidence interval as a pale bar. The heading gives the latest estimate and its change since the window's first run. Each point opens its source commit; hover for the date, interval and compiler. The selected case is part of the URL, so links to it can be shared.

**Latest values.** The table lists every case's latest estimate, interval and change over the window; a value from an earlier run than the latest shows its date. Its links select a case by keyboard, and the figure's description reads each row's latest change.

## Reading the trend

Changes include compiler and dependency upgrades alongside library changes. Criterion intervals are within-run estimates, not cross-run regression tests, and they do not remove code-layout effects or variation between machines with the same specification. Missing measurements are absent, not passes or zeroes.

Relative changes, including the per-case heading, compare only runs on the latest run's hardware specification and protocol; the per-case chart keeps earlier ones as separate, unconnected lines. Group means chain each case's ratio to its own previous measurement, so a case that joins the plan later does not shift the mean by its own level. With an unchanged set of cases, this equals the geometric mean of the cases' own changes.

The page downloads monthly JSONL from the public data branch according to the selected window and never queries the live Bencher API. See the [main tracking contract](../development/continuous-benchmarks.md#main-long-term-tracking) for coverage, publication, data access and operational limits. PR checks use their own paired comparisons.

## Embed the tracker

In another Vue-enabled Markdown page, import this component and use it directly:

	<script setup>
	import PerformanceHistory from "./path/to/PerformanceHistory.vue";
	</script>
	<PerformanceHistory />

Optional `source` (a data-directory URL ending in `/`) and `case-id` props select another compatible dataset or the initially selected case; a `#case=` URL hash takes precedence. Ordinary webpages can embed this page with an iframe. GitHub Markdown uses the [README’s fixed SVG preview](../../README.md), which shows only the group means. Publishing measurements does not rebuild this website.
