# Benchmark figures

Benchmark figures share a visual grammar, not just a palette. A reader should recognize the same typography, spacing and mark meanings in a release report, a PR comment and the documentation site.

<script setup lang="ts">
import BenchmarkFigure from "../.vitepress/theme/components/BenchmarkFigure.vue";
import type { RowChart } from "../../bench/plot/rows.ts";

const example: RowChart = {
  id: "figure-guide",
  title: "A shared template keeps comparisons aligned",
  guide: "Synthetic layout example · relative speed · larger is better",
  columns: ["Speed"],
  groups: [{
    title: "Example series",
    rows: [
      {
        name: { input: "Tessera", sub: "", tag: "", strong: true },
        marks: [{ value: 1.7, tone: "tessera" }],
        cells: ["1.70×"], strong: true,
      },
      {
        name: { input: "Other radix sort", sub: "", tag: "" },
        marks: [{ value: 1.2, tone: "radix" }],
        cells: ["1.20×"],
      },
      {
        name: { input: "Comparison sort", sub: "", tag: "" },
        marks: [{ value: 0.8, tone: "comparison" }],
        cells: ["0.80×"],
      },
    ],
  }],
  scale: {
    low: 0.5, high: 2, origin: 1, clamp: true,
    ticks: [{ value: 0.5, label: "1/2×" }, { value: 1, label: "reference" }, { value: 2, label: "2×" }],
  },
  legend: [[["tessera"], "Tessera"], [["radix"], "other radix sorts"], [["comparison"], "comparison sorts"]],
  axis: ["← slower", "faster →"],
};
</script>

<BenchmarkFigure :chart="example" />

This is a synthetic layout example, not a performance result. The site renders it with the same [row template](../../bench/plot/rows.ts) used by offline reports.

## Choose an expression

| Question | Template | Visual encoding |
|---|---|---|
| How do algorithms rank? | Named rows | One dot per algorithm on a shared ratio axis |
| Where is Tessera among competitors? | Grouped rows with a field of marks | Competitor dots, Tessera emphasized, value and rank columns |
| How much memory is required? | Named range rows | Ring for random input, dot for worst input, input-size reference |
| Which build regressed? | PR comparison matrix | Shared cases, one column per build, stems from zero |
| What changed during a PR? | PR trend matrix | One time series per case and build, reference at the baseline |

A new dataset does not require a new template. A genuinely different question may require one; the [extension boundary](../design/benchmark-tooling.md#extending-figures) explains where that implementation belongs.

## Fixed visual decisions

[`theme.ts`](../../bench/plot/theme.ts) owns the palette, typography and spacing tokens. [`figure.ts`](../../bench/plot/figure.ts) owns the panel, shared stylesheet and accessible SVG envelope. Templates use those foundations, not copied style blocks.

- One figure answers one question. Its title states a measured finding when results support one; the guide names the quantity, reference and aggregation.
- Labels form a left column; numbers form right-aligned value columns. Words do not repeat inside every plotted cell.
- Blue emphasizes Tessera or less run time, orange identifies other radix sorts or more run time, and neutral grey provides context. A chart's legend states its meaning; colour alone does not carry the verdict.
- Use dots and ranges for comparisons. A reference line makes the denominator visible. Ratio comparisons use logarithmic spacing; equal multiplicative changes occupy equal distances.
- A Portable SIMD build is a hollow ring in algorithm-speed figures. Rings are drawn above dots, and Tessera is drawn above the competitor field.
- Thin rules separate groups. Titles and value columns carry hierarchy; avoid gradients, shadows, 3D effects and per-chart font choices.
- Light and dark surfaces use the same semantic colours and separate neutral tokens. Every SVG includes a title and description.

The row interface accepts no arbitrary style objects or drawing callbacks. It owns label measurement, column widths, row spacing, collision lanes, drawing order, legends and escaping. Its input remains unchanged, so reactive callers can render it repeatedly.

## Use from TypeScript or VitePress

Import `renderRows` and `RowChart` from `bench/plot/rows.ts`. The renderer accepts the structure shown by the example above and returns an SVG string. Offline callers decide where to write it; browser callers decide where to display it. Neither the template nor the catalog imports filesystem or GitHub code.

VitePress uses [`BenchmarkFigure.vue`](../.vitepress/theme/components/BenchmarkFigure.vue), a reactive image adapter with responsive sizing and alternative text. Its `chart` prop is a `RowChart`; changing the data regenerates the SVG. When inserting SVG markup inline rather than using the image adapter, supply a unique chart `id` so accessibility references remain distinct.

Benchmark identifiers and human-readable names come from [the catalog](../../bench/catalog.json). A caller should not invent another name map while preparing a figure.

## Add or change a template

1. State the question and choose an existing expression where possible.
2. Keep statistical selection and aggregation outside the renderer.
3. Use the shared theme, layout and SVG surface. Add a token only when it represents a shared visual decision.
4. Test empty or unavailable results, long labels, coincident marks, extreme values and both one and multiple series where applicable.
5. Render a reference example in light and dark appearance and inspect it at its actual display width. Check hierarchy, label fit, value alignment and whether the main finding is visible without reading every number.
6. Keep the plotted-value table or equivalent accessible report alongside published figures.

[`bench/report/fixtures/`](../../bench/report/fixtures/) holds five standard algorithm figures; [the PR fixture](../../bench/continuous/fixtures/chart.svg) covers the comparison matrix. Snapshot tests detect output changes, while semantic tests check values and layout invariants. Update a snapshot only after inspecting the rendered change; an updated snapshot is not itself evidence of a better design.

Run `pnpm bench:test`, `pnpm bench:typecheck`, `pnpm docs:typecheck` and `pnpm docs:build` when the shared figure layer changes.
