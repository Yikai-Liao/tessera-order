// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { chooseTileLeaf, planDigits, sampleKeys } from "./digits.ts";

test("short flat plans balance shifted runs and sparse bits", () => {
  for (const shift of [0n, 17n, 96n]) {
    const plan = planDigits(0xfffffn << shift, 14, 128);
    assert.deepEqual(
      plan.map((digit) => digit.width),
      [10, 10],
    );
    assert.equal(chooseTileLeaf(8192, 0xfffffn << shift, 14, 128), "lsd");
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
    if (chooseTileLeaf(1024, mask, 3, 128) === "lsd") assert.equal(covered & mask, mask);
    else assert.notEqual(covered & mask, 0n);
  }
  const sparse = (1n << 127n) | 1n;
  assert.deepEqual(planDigits(sparse, 14, 128), [
    { shift: 127, width: 1 },
    { shift: 0, width: 1 },
  ]);
});

test("wide tiles partition without sampling and short tiles retain insertion", () => {
  const varying = (1n << 64n) - 1n;
  assert.equal(chooseTileLeaf(1024, varying, 12, 64), "msd");
  assert.equal(chooseTileLeaf(8192, 0n, 14, 64), "done");
  assert.equal(chooseTileLeaf(48, varying, 6, 64), "insertion");
  assert.deepEqual(planDigits(varying, 12, 64), [{ shift: 52, width: 12 }]);
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
  // Locally ordered input still yields one independent observation per stratum.
  const ordered = Array.from({ length: 65536 }, (_, index) => BigInt(index));
  assert.equal(new Set(sampleKeys(ordered).map((key) => key >> 9n)).size, 128);
});
