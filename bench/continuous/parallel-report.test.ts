// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { chartSvg } from "./chart.ts";
import { fixture } from "./fixtures/report.ts";
import { summarizeTrend } from "./history.ts";
import { renderCombined } from "./markdown.ts";
import { summarize } from "./report.ts";
import { trendSvg } from "./trend-chart.ts";

/** Synthetic values test presentation, not sorting performance. */
function mixed(index: number, parallel: number, baseline: number | null = 100) {
  const report = fixture();
  report.uuid = `mixed-${index}`;
  report.start_time = new Date(Date.UTC(2026, 10, 7, 0, index)).toISOString();
  const original = report.results[0][0];
  report.results = [
    ["api", "par-4"].map((mode) => ({
      benchmark: { uuid: mode, name: `rust/u32/random/10000000/s1/${mode}` },
      measures: [
        {
          ...structuredClone(original.measures[0]),
          metric: { uuid: `${index}-${mode}`, value: mode === "api" ? 100 : parallel },
          boundary:
            mode === "par-4" && baseline === null
              ? null
              : { baseline: 100, lower_limit: 95, upper_limit: 105 },
        },
      ],
    })),
  ];
  report.alerts =
    parallel > 105 && baseline !== null
      ? [{ metric: { uuid: `${index}-par-4` }, limit: "upper" }]
      : [];
  return report;
}

test("new parallel baselines are explicit and do not hide a failed SIMD build", () => {
  const summary = summarize(mixed(1, 100, null));
  const body = renderCombined(
    [
      { name: "Default", summary, outcome: "success" },
      { name: "Portable SIMD", summary: null, outcome: "failure" },
    ],
    { language: "rust", hash: "a".repeat(40), runUrl: "https://example.test/run" },
  );
  assert.match(body, /parallel · 4 threads \| — · 1 new \| failed/);
  assert.match(body, /1 new/);
  assert.match(body, /Portable SIMD failed/);
  assert.match(body, /par-4/);
});

test("comparison has API row groups but only the original build columns", () => {
  const current = mixed(1, 130);
  // Include an independent serial alert to exercise both row groups.
  current.results[0][0].measures[0].metric.value = 110;
  current.alerts.push({ metric: { uuid: "1-api" }, limit: "upper" });
  const svg = chartSvg([
    { name: "Default", summary: summarize(current) },
    { name: "Portable SIMD", summary: null },
  ])!;
  assert.match(svg, />SERIAL</);
  assert.match(svg, />PARALLEL · 4 THREADS</);
  assert.match(svg, />Default</);
  assert.match(svg, />Portable SIMD</);
  assert.match(svg, /measurement failed/);
  assert.doesNotMatch(svg, /NaN|Infinity/);
});

test("history separates serial and four-thread fixed-cohort aggregates", () => {
  const first = mixed(1, 100);
  const current = mixed(2, 80);
  const trend = summarizeTrend([first, current], current, "rust")!;
  const svg = trendSvg([
    { name: "Default", trend },
    { name: "Portable SIMD", trend: null },
  ])!;
  assert.match(svg, />serial</);
  assert.match(svg, />parallel · 4 threads</);
  assert.match(svg, />−20\.0%</);
  assert.match(svg, />\+0\.0%</);
  assert.doesNotMatch(svg, />Overall</);
  const fresh = mixed(2, 100, null);
  const freshTrend = summarizeTrend([first, fresh], fresh, "rust")!;
  const missing = trendSvg([{ name: "Default", trend: freshTrend }])!;
  assert.match(missing, />parallel · 4 threads</);
  assert.match(missing, /no comparable history/);
});
