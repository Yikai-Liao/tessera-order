// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { pairedBudget, summarizePaired } from "./statistics.ts";

test("short screens require unanimous direction outside paired A/A", () => {
  for (const [values, verdict] of [
    [[99, 99, 99], "faster"],
    [[121, 121, 121], "slower"],
    [[109, 109, 109], "inconclusive"],
    [[99, 99, 121], "inconclusive"],
    [[121, 121, 99], "inconclusive"],
  ] as const) {
    const blocks = values.map((value) => ({ first: 100, second: 120, value }));
    const result = summarizePaired(blocks, 3);
    assert.equal(result.verdict, verdict);
    assert.equal(result.aaLow, 100 / 110);
    assert.equal(result.aaHigh, 120 / 110);
    assert.deepEqual(
      summarizePaired(
        blocks.map(({ first, second, value }) => ({ first: second, second: first, value })),
        3,
      ),
      result,
    );
    assert.throws(() => summarizePaired(blocks), /9 complete blocks/);
    assert.throws(() => summarizePaired(blocks.slice(1), 3), /3 complete blocks/);
  }
  for (const count of [0, 1, 2, 4, 5, 8, 10, NaN]) assert.throws(() => pairedBudget(count));
});
