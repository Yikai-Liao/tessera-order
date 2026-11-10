// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { planDigits, sampleKeys } from "./digits.ts";

test("digit planning balances shifted runs and covers sparse masks", () => {
  for (const shift of [0n, 17n, 96n]) {
    const plan = planDigits(0xffff_ffffn << shift, 14, 128);
    assert.equal(plan.length, 3);
    assert.equal(
      plan.reduce((slots, digit) => slots + 2 ** digit.width, 0),
      5120,
    );
  }
  for (let mask = 1n; mask < 256n; mask++) {
    const plan = planDigits(mask, 3, 128);
    let covered = 0n;
    for (const digit of plan) {
      const bits = ((1n << BigInt(digit.width)) - 1n) << BigInt(digit.shift);
      assert.equal(covered & bits, 0n);
      assert.ok(digit.width <= 3);
      covered |= bits;
    }
    assert.equal(covered & mask, mask);
  }
});

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
