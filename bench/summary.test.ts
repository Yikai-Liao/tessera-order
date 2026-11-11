// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { resultSummary } from "./summary.ts";

test("compact results retain counts, denominator-independent evidence and truncation", () => {
  const rows = Array.from({ length: 8 }, (_, i) => ({
    id: String(i),
    ratio: 1.1 + i / 10,
    low: 1.05,
    high: 2,
    verdict: "slower" as const,
    aaLow: 0.98,
    aaHigh: 1.02,
  }));
  const result = resultSummary([
    ...rows,
    { id: "fast", ratio: 0.5, low: 0.4, high: 0.6, verdict: "faster" },
    { id: "quiet", ratio: 1, low: 0.9, high: 1.1, verdict: "within noise" },
  ]);
  assert.deepEqual(
    [result.measured, result.slower, result.faster, result.withinNoise],
    [10, 8, 1, 1],
  );
  assert.equal(result.focus[0].id, "7");
  assert.deepEqual(result.focus[0].aaRange, [0.98, 1.02]);
  assert.equal(result.omittedChanges, 4);
  assert.equal(rows[0].id, "0");
  assert.equal(resultSummary([]).measured, 0);
  const short = resultSummary([
    {
      id: "short",
      ratio: 1.1,
      low: 0.9,
      high: 1.2,
      verdict: "inconclusive",
      aaLow: 0.8,
      aaHigh: 1.2,
    },
  ]);
  assert.equal(short.inconclusive, 1);
  assert.equal(short.withinNoise, 0);
  assert.equal(short.slower, 0);
  assert.equal(short.focus[0].verdict, "inconclusive");
});
