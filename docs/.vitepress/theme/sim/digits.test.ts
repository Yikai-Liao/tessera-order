// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { chooseTileLeaf, partitionWork, planDigits, sampleKeys } from "./digits.ts";

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

test("conditional child work preserves terminal groups without a fanout cutoff", () => {
  const varying = (1n << 64n) - 1n;
  const digit = { shift: 52, width: 12 };
  for (const groups of [1, 2, 3, 4, 8, 16, 32, 64]) {
    const records = Array.from({ length: 1024 }, (_, index) => BigInt(index % groups) << 56n);
    const sample = sampleKeys(records);
    assert.equal(partitionWork(1024, varying, digit, sample, 8, 64), 2 * sample.length);
    assert.equal(chooseTileLeaf(1024, varying, 12, sample, 64), "msd");
  }
  const sample = sampleKeys(
    Array.from(
      { length: 1024 },
      (_, index) => (BigInt(index) * 0x9e3779b97f4a7c15n) & ((1n << 52n) - 1n),
    ),
  );
  assert.ok(partitionWork(1024, varying, digit, sample, 8, 64) > 2 * sample.length);
  assert.equal(chooseTileLeaf(1024, varying, 12, sample, 64), "msd");
  const mask = ((1n << 12n) - 1n) << 52n;
  assert.equal(partitionWork(1024, mask, digit, sample, 8, 64), 2 * sample.length);
  assert.ok(partitionWork(8192, varying, { shift: 63, width: 1 }, [0n], 8, 64) > 2);
  assert.equal(chooseTileLeaf(8192, 0xfffffn, 14, [], 64), "lsd");
  assert.equal(chooseTileLeaf(8192, 0n, 14, [], 64), "done");
  assert.equal(chooseTileLeaf(48, varying, 6, [], 64), "insertion");
});

test("sampled child continuation retains recursive partition opportunities", () => {
  for (const groups of [2n, 4n, 8n]) {
    const records = Array.from({ length: 1024 }, (_, i) => {
      const index = BigInt(i);
      return ((index % groups) << 60n) | ((index * 0x9e3779b97f4a7c15n) & ((1n << 40n) - 1n));
    });
    const varying = records.reduce((mask, key) => mask | (key ^ records[0]), 0n);
    assert.equal(chooseTileLeaf(records.length, varying, 12, sampleKeys(records), 64), "msd");
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
