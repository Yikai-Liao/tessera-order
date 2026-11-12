# Main performance history

<script setup lang="ts">
import PerformanceHistory from "../.vitepress/theme/components/PerformanceHistory.vue";
</script>

This page tracks the absolute time to sort fixed inputs on main, measured on Bencher's Intel v1 Bare Metal runners. Select a case and time window, then hover over a point for its build. Points link to source commits; the table provides measurement evidence.

<PerformanceHistory />

The trend includes compiler and dependency upgrades alongside library changes. Intervals are Criterion's within-run confidence intervals, not cross-run regression tests; they do not remove code-layout effects or variation between machines with the same specification. Serial and four-thread entry points are distinct cases. Missing measurements are absent, not passes or zeroes. Hardware/protocol changes are not connected by a line.

The page downloads monthly JSONL from the public data branch according to the selected window, combines months in one chart, and never queries the live Bencher API. See the [main tracking contract](../development/continuous-benchmarks.md#main-long-term-tracking) for coverage, publication, data access and operational limits. PR checks continue to use their independent paired comparisons.

## Embed the tracker

In another Vue-enabled Markdown page, import this component and use it directly:

	<script setup>
	import PerformanceHistory from "./path/to/PerformanceHistory.vue";
	</script>
	<PerformanceHistory />

Optional `source` (a data-directory URL ending in `/`) and `case-id` props select another compatible dataset or initial case. Ordinary webpages can embed this page with an iframe. GitHub Markdown does not run Vue or iframes; use the stable SVG preview linked to this page:

	[![Main performance](https://raw.githubusercontent.com/Yikai-Liao/tessera-sort/benchmark-data/main/preview.svg)](https://yikai-liao.github.io/tessera-sort/benchmarks/)

The SVG URL stays fixed; publication replaces its contents without rebuilding this website. GitHub's image cache may delay refreshes.
