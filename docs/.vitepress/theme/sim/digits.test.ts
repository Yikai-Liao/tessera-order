// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { sampleKeys } from "./digits.ts";

test("bounded sampling covers short inputs and does not alias periodic keys", () => {
  for (const len of [1, 7, 8, 9, 63, 127, 128, 129, 1024]) {
    const records = Array.from({ length: len }, (_, index) => BigInt(index));
    const sample = sampleKeys(records);
    assert.equal(sample.length, Math.min(len, 128));
    assert.equal(new Set(sample).size, sample.length);
    if (len <= 128) assert.deepEqual(sample, records);
  }
  const periodic = Array.from({ length: 1024 }, (_, index) => BigInt(index % 8));
  assert.equal(new Set(sampleKeys(periodic)).size, 8);
});
