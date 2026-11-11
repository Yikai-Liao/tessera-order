// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { VARIANTS } from "../catalog.ts";
import { pairedRun } from "./fixtures/paired.ts";
import { fixture } from "./fixtures/report.ts";
import { summarizePairedTrend } from "./history.ts";
import { pairedBmf, pairedSummary } from "./paired.ts";

function measured(ratio: number, uuid: string, time: string, head = "same-baseline") {
  const run = pairedRun(VARIANTS[0], 1, ratio);
  const report = fixture();
  report.uuid = uuid;
  report.start_time = time;
  report.branch.head.uuid = head;
  report.branch.head.start_point = null;
  report.testbed.name = VARIANTS[0].relativeTestbed;
  report.results = [
    Object.entries(pairedBmf([run])).map(([name, values]) => ({
      benchmark: { uuid: name, name },
      measures: [
        {
          measure: { uuid: "ratio", name: "paired-time-ratio" },
          metric: { uuid: name, value: values["paired-time-ratio"].value },
          boundary: null,
        },
      ],
    })),
  ];
  return { report, summary: pairedSummary([run], VARIANTS[0], report) };
}

test("paired history retains focus after a regression is fixed", () => {
  const first = measured(1.3, "first", "2026-11-11T00:00:00Z");
  const current = measured(0.9, "current", "2026-11-11T01:00:00Z");
  const trend = summarizePairedTrend(current.summary, "rust", current.report, [first.report])!;
  assert.equal(trend.runs.length, 2);
  assert.equal(trend.focus.length, current.summary.rows.length);
  assert.ok(Math.abs(trend.overall[0] - 0.3) < 1e-10);
  assert.ok(Math.abs(trend.overall[1] + 0.1) < 1e-10);
  assert.equal(trend.base, current.summary.baseHash);
});

test("paired history excludes other baseline Heads and fixes the shared cohort", () => {
  const other = measured(3, "other", "2026-11-11T00:00:00Z", "different-baseline");
  const first = measured(1.1, "first", "2026-11-11T01:00:00Z");
  first.report.results[0].pop();
  const current = measured(1, "current", "2026-11-11T02:00:00Z");
  const trend = summarizePairedTrend(current.summary, "rust", current.report, [
    other.report,
    first.report,
  ])!;
  assert.deepEqual(
    trend.runs.map((run) => run.uuid),
    ["first", "current"],
  );
  assert.equal(trend.sharedCases, current.summary.rows.length - 1);
  assert.ok(Math.abs(trend.overall[0] - 0.1) < 1e-10);
  assert.equal(trend.overall[1], 0);
});

test("unavailable Cloud still produces the current measured trend point", () => {
  const { summary } = measured(1.2, "current", "2026-11-11T00:00:00Z");
  const trend = summarizePairedTrend(summary, "rust")!;
  assert.equal(trend.runs.length, 1);
  assert.equal(trend.runs[0].hash, summary.hash);
  assert.ok(Math.abs(trend.overall[0] - 0.2) < 1e-10);
});
