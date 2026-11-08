// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { type LocalBlock, summarizeLocal } from "./model.ts";

const id = "rust/u64/random/65536/s1/api";
const fixture = (current: number) => ({
  cases: [id],
  blocks: Array.from(
    { length: 9 },
    (): LocalBlock => ({
      baseline: { [id]: 100 },
      control: { [id]: 120 },
      current: { [id]: current },
    }),
  ),
});

test("paired controls bound local verdicts", () => {
  for (const [time, verdict] of [
    [99, "faster"],
    [100, "within noise"],
    [109, "within noise"],
    [120, "within noise"],
    [121, "slower"],
  ] as const) {
    const run = fixture(time);
    const [summary] = summarizeLocal(run);
    assert.equal(summary.verdict, verdict);
    assert.equal(summary.aaLow, 100 / 110);
    assert.equal(summary.aaHigh, 120 / 110);
    for (const block of run.blocks)
      [block.baseline, block.control] = [block.control, block.baseline];
    assert.deepEqual(summarizeLocal(run), [summary]);
  }
});

test("local verdicts require eight blocks", () => {
  const run = fixture(90);
  run.blocks[0].current[id] = 130;
  assert.equal(summarizeLocal(run)[0].verdict, "faster");
  run.blocks[1].current[id] = 130;
  assert.equal(summarizeLocal(run)[0].verdict, "within noise");
});

test("incomplete local blocks are rejected", () => {
  const run = fixture(90);
  run.blocks.pop();
  assert.throws(() => summarizeLocal(run), /nine complete/);
  for (const value of [0, -1, Infinity, NaN]) {
    const invalid = fixture(value);
    assert.throws(() => summarizeLocal(invalid), /selected cases/);
  }
  const missing = fixture(90);
  missing.blocks[0].current = {};
  assert.throws(() => summarizeLocal(missing), /selected cases/);
});
