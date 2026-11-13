# Benchmark figures

Benchmark figures share a visual grammar, not just a palette. A reader should recognize the same typography, spacing and mark meanings in a release report, a PR comment and the documentation site.

<script setup lang="ts">
import BenchmarkFigure from "../.vitepress/theme/components/BenchmarkFigure.vue";
import type { RowChart } from "../../bench/plot/rows.ts";

const example: RowChart = {
  id: "figure-guide",
  title: "Three stable sorts in a recorded release",
  guide: "1,000,000 u64 keys · AMD Ryzen 7 8845H · speed vs slice::sort, geometric mean over 10 inputs",
  columns: ["Speed"],
  groups: [{
    title: "Stable sorts",
    rows: [
      {
        name: { input: "Tessera", sub: "", tag: "", strong: true },
        marks: [{ value: 2.6, tone: "primary" }],
        cells: ["2.6×"], strong: true,
      },
      {
        name: { input: "fluxsort", sub: "", tag: "" },
        marks: [{ value: 1.5, tone: "comparison" }],
        cells: ["1.5×"],
      },
      {
        name: { input: "voracious", sub: "", tag: "" },
        marks: [{ value: 1.3, tone: "radix" }],
        cells: ["1.3×"],
      },
    ],
  }],
  scale: {
    low: 0.5, high: 4, origin: 1, clamp: true,
    ticks: [{ value: 0.5, label: "1/2×" }, { value: 1, label: "slice::sort" }, { value: 2, label: "2×" }, { value: 4, label: "4×" }],
  },
  legend: [[["primary"], "Tessera"], [["radix"], "other radix sorts"], [["comparison"], "comparison sorts"]],
  axis: ["← slower", "faster →"],
};
</script>

<BenchmarkFigure :chart="example" />

These rounded values come from the recorded `release-benchmark-raw-2d6fac2` session, measured at commit [`2d6fac2`](https://github.com/Yikai-Liao/tessera-order/commit/2d6fac27d2584694eb4d3c989f2f09f538dd78aa) with the default build, nine blocks and ten input patterns. This is a three-sort excerpt, not the complete ranking or a measurement of the current commit. The site renders it with the same [row template](../../bench/plot/rows.ts) used by offline reports.

## Choose an expression

| Question | Template | Visual encoding |
|---|---|---|
| How do algorithms rank? | Named rows | One dot per algorithm on a shared ratio axis |
| Where is Tessera among competitors? | Grouped rows with a field of marks | Competitor dots, Tessera emphasized, value and rank columns |
| How much memory is required? | Named range rows | Ring for random input, dot for worst input, input-size reference |
| Which build regressed? | PR comparison matrix | Shared cases, one column per build, stems from zero |
| What changed during a PR or on main? | Trend matrix | One time series per case and build, reference at the baseline or first run |
| How do selected cases compare over time? | Case comparison | Up to four cases on one log-time or relative-change axis with intervals; independent commit-slot or UTC-time spacing; identity colours that are neither blue nor orange |

A new dataset does not require a new template; a genuinely different question may.

## Fixed visual decisions

[`theme.ts`](../../bench/plot/theme.ts) owns the palette, typography and spacing tokens. [`figure.ts`](../../bench/plot/figure.ts) owns the panel, shared stylesheet and accessible SVG envelope. Templates use those foundations, not copied style blocks.

- One figure answers one question. Its title states a measured finding in a few words when results support one, such as "Fastest on 9 of 14 inputs"; Tessera is the implied subject. The guide below it names the length, key type, quantity, reference and aggregation, so the large title carries no parameters.
- A rank cell for a first place takes Tessera's blue, so leads stand out from the other ranks.
- Labels form a left column; numbers form right-aligned value columns. Words do not repeat inside every plotted cell.
- Blue emphasizes Tessera or less run time, orange identifies other radix sorts or more run time, and neutral grey provides context. A chart's legend states its meaning; colour alone does not carry the verdict.
- Use dots and ranges for comparisons. A reference line makes the denominator visible. Ratio comparisons use logarithmic spacing; equal multiplicative changes occupy equal distances.
- Tessera is drawn above the competitor field. Rings distinguish reference observations such as random-input memory from the worst case. The shared axis includes every plotted value; values outside 1/32×–32× use overflow triangles, with their numeric values retained in the accompanying tables. Selected-input figures also mark off-scale block-range endpoints, even when the median is in range, and print both bounds in the figure and table.
- Comparison and trend matrices group serial and parallel cases vertically under mode headings, with serial first. Each trend group starts with its own geometric mean; case labels reserve tags for qualifiers such as non-default seeds, not the mode.
- Thin rules separate groups. Titles and value columns carry hierarchy; avoid gradients, shadows, 3D effects and per-chart font choices.
- Light and dark surfaces use the same semantic colours and separate neutral tokens. Standalone SVGs carry literal neutral colours for image viewers; ordinary CSS supplies dark mode, following the system preference in a standalone image and the page's `dark` class on `<html>` when inlined, so the site's theme switch applies. Every SVG includes a title and description.

The row interface accepts no style objects or drawing callbacks; the [template](../../bench/plot/rows.ts) owns widths, spacing, drawing order and legends and never mutates its input, so reactive callers can re-render it. Import `renderRows` and `RowChart` from it to get an SVG string; offline callers decide where to write it. VitePress uses [`BenchmarkFigure.vue`](../.vitepress/theme/components/BenchmarkFigure.vue), a reactive image adapter taking a `RowChart`; for inline SVG supply a unique chart `id` so accessibility references stay distinct. Names come from [the catalog](../../bench/catalog.toml), never a second map. The [extension boundary](../design/benchmark-tooling.md#figure-boundary) says where new templates live.

## Add or change a template

1. State the question and choose an existing expression where possible.
2. Keep statistical selection and aggregation outside the renderer.
3. Use the shared theme, layout and SVG surface. Add a token only when it represents a shared visual decision.
4. Test empty or unavailable results, long labels, coincident marks, extreme values and both one and multiple series where applicable.
5. Render a reference example in light and dark appearance and inspect it at its actual display width. Check hierarchy, label fit, value alignment and whether the main finding is visible without reading every number.
6. Keep the plotted-value table or equivalent accessible report alongside published figures.

Automated tests protect report generation, numeric summaries and unavailable builds, not SVG snapshots, wording or coordinates. When changing rendering, generate before-and-after figures from the same recorded dataset and inspect both appearances. Run `bun run tools:test`, `bun run tools:typecheck`, `bun run docs:typecheck` and `bun run docs:build` when the shared figure layer changes.
