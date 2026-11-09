// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { fixture } from "./fixtures/report.ts";
import { summarizeVolatility } from "./volatility.ts";

test("main volatility pairs adjacent observations, filters environments and retains reruns", () => {
  const make = (index: number, value: number) => {
    const report = fixture();
    report.uuid = `run-${index}`;
    report.branch.name = "main";
    report.start_time = new Date(Date.UTC(2026, 10, 1, index)).toISOString();
    report.end_time = report.start_time;
    report.results = [[structuredClone(report.results[0][0])]];
    report.results[0][0].measures[0].metric.value = value;
    return report;
  };
  const first = make(1, 100),
    middle = make(2, 120),
    last = make(3, 114);
  last.branch.head.version = { hash: "other-commit" };
  const differentSpec = make(2, 999);
  differentSpec.uuid = "different-spec";
  differentSpec.testbed.spec = { uuid: "other" };
  const incomplete = make(2, 888);
  incomplete.uuid = "incomplete";
  incomplete.end_time = undefined;
  const result = summarizeVolatility(
    [last, first, middle, first, make(4, 999), differentSpec, incomplete],
    last,
  );
  assert.equal(result.runs.length, 3);
  assert.equal(result.cases.length, 1);
  assert.equal(result.cases[0].pairs, 2);
  assert.equal(result.cases[0].outsideFivePercent, 1);
  assert.equal(result.cases[0].sameCommitChanges.length, 1);
  assert.ok(Math.abs(result.cases[0].maxAbsoluteChange - 0.2) < 1e-12);
  const missing = make(2, 120);
  missing.results = [[]];
  assert.equal(summarizeVolatility([first, missing, last], last).cases[0].pairs, 1);
});
