// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { summarizeRatios } from "./statistics.ts";

test("paired median intervals reject isolated outliers and a practical null", () => {
  for (const count of [20, 30, 40]) {
    for (const ratio of [0.95, 1, 1.05])
      assert.equal(summarizeRatios(Array(count).fill(ratio)).verdict, "within noise");
    assert.equal(summarizeRatios(Array(count).fill(1.1)).verdict, "slower");
    assert.equal(summarizeRatios(Array(count).fill(0.9)).verdict, "faster");
    assert.equal(summarizeRatios([100, ...Array(count - 1).fill(1)]).verdict, "within noise");
  }
  const s = summarizeRatios(Array.from({ length: 30 }, (_, i) => 1 + i / 100));
  assert.equal(s.low, 1.07);
  assert.equal(s.high, 1.22);
  assert.throws(() => summarizeRatios([1]));
  assert.throws(() => summarizeRatios(Array(30).fill(NaN)));
});
