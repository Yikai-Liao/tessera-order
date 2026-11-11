// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { parseHTML } from "linkedom/worker";
import { modeName } from "../catalog.ts";
import type { TrendSeries, TrendSummary } from "./history.ts";
import { caseParts, formatChange } from "./report.ts";
import { trendSvg } from "./trend-chart.ts";

function trend(series: TrendSeries[], modes = ["par-4", "api"]): TrendSummary {
  return {
    base: "b".repeat(40),
    modes,
    runs: ["first", "latest"].map((uuid) => ({ uuid, hash: uuid })),
    overall: [0, 0],
    focus: [...series],
    series,
  };
}

function labels(summary: TrendSummary) {
  const { document } = parseHTML(trendSvg(summary));
  return [...document.querySelectorAll("g[aria-label='text'] text")].map(
    (node) => node.textContent,
  );
}

function section(text: (string | null)[], mode: string, next?: string) {
  const start = text.indexOf(modeName(mode).toUpperCase());
  assert.notEqual(start, -1, `missing ${mode} heading`);
  const end = next ? text.indexOf(modeName(next).toUpperCase()) : text.length;
  assert.ok(end > start);
  return text.slice(start + 1, end);
}

test("trend cases stay with their mode aggregate and keep swing order within each group", () => {
  const parallel = { name: "rust/u64/random/1000000/s1/par-4", changes: [0, 0.8] };
  const serial = { name: "rust/u64/d256/1000000/s1/api", changes: [0, 0.4] };
  const parallelSmall = { name: "rust/u64/reverse/1000000/s2/par-4", changes: [0, 0.2] };
  const serialSmall = { name: "rust/u64/sorted/1000000/s1/api", changes: [0, 0.1] };
  const summary = trend([parallel, serialSmall, parallelSmall, serial]);
  // A quiet, unselected case still contributes to its mode's geometric mean.
  summary.series.push({ name: "rust/u64/equal/1000000/s1/api", changes: [0, 0] });
  const text = labels(summary);
  const serialLabels = section(text, "api", "par-4");
  const parallelLabels = section(text, "par-4");
  for (const [group, cases, mean] of [
    [serialLabels, [serial, serialSmall], (1.4 * 1.1) ** (1 / 3) - 1],
    [parallelLabels, [parallel, parallelSmall], Math.sqrt(1.8 * 1.2) - 1],
  ] as const) {
    assert.ok(group.includes(formatChange(mean)));
    const positions = cases.map((entry) => group.indexOf(caseParts(entry.name).input));
    assert.ok(positions[0] > group.indexOf(formatChange(mean)) && positions[1] > positions[0]);
    for (const entry of cases) assert.ok(group.includes(formatChange(entry.changes.at(-1)!)));
  }
  assert.ok(parallelLabels.includes("seed 2"));
  assert.ok(!serialLabels.includes(caseParts(parallel.name).input));
  assert.ok(!parallelLabels.includes(caseParts(serial.name).input));
  assert.ok(!text.includes(modeName("par-4")), "mode belongs in the heading, not row tags");
});

test("a quiet single-mode history still has its titled aggregate", () => {
  for (const mode of ["api", "par-4"]) {
    const summary = trend(
      [{ name: `rust/u64/random/1000000/s1/${mode}`, changes: [0, 0.03] }],
      [mode],
    );
    summary.focus = [];
    summary.overall = [0, 0.03];
    const group = section(labels(summary), mode);
    assert.ok(group.includes(formatChange(0.03)));
    assert.ok(!group.includes("random"));
  }
});

test("a current mode without shared history retains an unavailable aggregate", () => {
  const summary = trend([{ name: "rust/u64/random/1000000/s1/api", changes: [0, 0.2] }]);
  const text = labels(summary);
  const group = section(text, "par-4");
  assert.ok(group.includes("—"));
  assert.ok(!group.includes("random"));
});
