// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { parseHTML } from "linkedom/worker";
import { type Execution, executionSection, SORT } from "../catalog.ts";
import type { TrendSeries, TrendSummary } from "./history.ts";
import { caseParts, formatChange } from "./report.ts";
import { trendSvg } from "./trend-chart.ts";

const heading = (execution: Execution) => executionSection(SORT, execution).title;

function trend(series: TrendSeries[], executions: Execution[] = ["serial", "t4"]): TrendSummary {
  return {
    base: "b".repeat(40),
    executions,
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

function section(text: (string | null)[], execution: Execution, next?: Execution) {
  const start = text.indexOf(heading(execution));
  assert.notEqual(start, -1, `missing ${execution} heading`);
  const end = next ? text.indexOf(heading(next)) : text.length;
  assert.ok(end > start);
  return text.slice(start + 1, end);
}

test("trend cases stay with their execution aggregate and keep swing order within each group", () => {
  const parallel = { name: "rust/sort/u64/random/1000000/s1/t4", changes: [0, 0.8] };
  const serial = { name: "rust/sort/u64/d256/1000000/s1/serial", changes: [0, 0.4] };
  const parallelSmall = { name: "rust/sort/u64/reverse/1000000/s1/t4", changes: [0, 0.2] };
  const serialSmall = { name: "rust/sort/u64/sorted/1000000/s1/serial", changes: [0, 0.1] };
  const summary = trend([parallel, serialSmall, parallelSmall, serial]);
  // A quiet, unselected case still contributes to its execution's geometric mean.
  summary.series.push({ name: "rust/sort/u64/equal/1000000/s1/serial", changes: [0, 0] });
  const text = labels(summary);
  const serialLabels = section(text, "serial", "t4");
  const parallelLabels = section(text, "t4");
  for (const [group, cases, mean] of [
    [serialLabels, [serial, serialSmall], (1.4 * 1.1) ** (1 / 3) - 1],
    [parallelLabels, [parallel, parallelSmall], Math.sqrt(1.8 * 1.2) - 1],
  ] as const) {
    assert.ok(group.includes(formatChange(mean)));
    const positions = cases.map((entry) => group.indexOf(caseParts(entry.name).input));
    assert.ok(positions[0] > group.indexOf(formatChange(mean)) && positions[1] > positions[0]);
    for (const entry of cases) assert.ok(group.includes(formatChange(entry.changes.at(-1)!)));
  }
  assert.ok(!serialLabels.includes(caseParts(parallel.name).input));
  assert.ok(!parallelLabels.includes(caseParts(serial.name).input));
  assert.equal(
    text.filter((label) => label === heading("t4")).length,
    1,
    "the execution belongs in the heading, not row tags",
  );
});

test("a quiet single-execution history still has its titled aggregate", () => {
  for (const execution of ["serial", "t4"] as const) {
    const summary = trend(
      [{ name: `rust/sort/u64/random/1000000/s1/${execution}`, changes: [0, 0.03] }],
      [execution],
    );
    summary.focus = [];
    summary.overall = [0, 0.03];
    const group = section(labels(summary), execution);
    assert.ok(group.includes(formatChange(0.03)));
    assert.ok(!group.includes("random"));
  }
});

test("a current execution without shared history retains an unavailable aggregate", () => {
  const summary = trend([{ name: "rust/sort/u64/random/1000000/s1/serial", changes: [0, 0.2] }]);
  const text = labels(summary);
  const group = section(text, "t4");
  assert.ok(group.includes("—"));
  assert.ok(!group.includes("random"));
});
