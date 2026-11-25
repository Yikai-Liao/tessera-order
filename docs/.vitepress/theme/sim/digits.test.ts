// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { chooseTileLeaf, planDigits, planTile, sampleKeys } from "./digits.ts";

function optimalDigitCost(bits: bigint, width: number): { passes: number; slots: number } {
  if (bits === 0n) return { passes: 0, slots: 0 };
  const high = bits.toString(2).length;
  let best = { passes: Number.POSITIVE_INFINITY, slots: Number.POSITIVE_INFINITY };
  for (let span = 1; span <= Math.min(width, high); span++) {
    const remaining = bits & ~(((1n << BigInt(span)) - 1n) << BigInt(high - span));
    const sub = optimalDigitCost(remaining, width);
    const passes = sub.passes + 1;
    const slots = sub.slots + (1 << span);
    if (passes < best.passes || (passes === best.passes && slots < best.slots)) {
      best = { passes, slots };
    }
  }
  return best;
}

test("short flat plans balance shifted runs and sparse bits", () => {
  for (const shift of [0n, 17n, 96n]) {
    const plan = planDigits(0xfffffn << shift, 14, 128);
    assert.deepEqual(
      plan.map((digit) => digit.width),
      [10, 10],
    );
    assert.equal(chooseTileLeaf(8192, 0xfffffn << shift, 14, 128), "lsd");
  }
  // Compare against an independent exhaustive oracle so exact-fit lower digits
  // cannot be silently dropped into MSD.
  for (let mask = 1n; mask < 256n; mask++) {
    const optimum = optimalDigitCost(mask, 3);
    const decision = planTile(1024, mask, 3, 128, 16, false);
    const plan = planDigits(mask, 3, 128);
    assert.deepEqual(plan, decision.digits);
    let covered = 0n;
    let slots = 0;
    for (const digit of plan) {
      const bits = ((1n << BigInt(digit.width)) - 1n) << BigInt(digit.shift);
      assert.equal(covered & bits, 0n);
      assert.ok(digit.width <= 3);
      covered |= bits;
      slots += 1 << digit.width;
    }
    if (optimum.passes <= 2) {
      assert.equal(decision.leaf, "lsd");
      assert.equal(plan.length, optimum.passes);
      assert.equal(slots, optimum.slots);
      assert.equal(covered & mask, mask);
    } else {
      assert.equal(decision.leaf, "msd");
      assert.equal(plan.length, 1);
      assert.notEqual(covered & mask, 0n);
    }
  }
  // Exact-fit lower digit (width === tableWidth) must stay a two-pass LSD plan.
  assert.deepEqual(planTile(1024, (1n << 11n) | (1n << 5n) | 1n, 6, 16, 2, false), {
    leaf: "lsd",
    digits: [
      { shift: 11, width: 1 },
      { shift: 0, width: 6 },
    ],
    simdMsdOverride: false,
  });
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
  assert.equal(chooseTileLeaf(24, varying, 6, 64), "insertion");
  assert.equal(chooseTileLeaf(24, varying, 6, 64, 128), "msd");
  assert.deepEqual(planDigits(varying, 12, 64), [{ shift: 52, width: 12 }]);

  // Direct-key SIMD MSD overrides a two-pass LSD split only when one top digit
  // provides at least one bucket per record with fewer table slots.
  const bits25 = (1n << 25n) - 1n;
  assert.deepEqual(planTile(8192, bits25, 13, 32, 4, false), {
    leaf: "lsd",
    digits: [
      { shift: 12, width: 13 },
      { shift: 0, width: 12 },
    ],
    simdMsdOverride: false,
  });
  assert.deepEqual(planTile(8192, bits25, 13, 32, 4, true), {
    leaf: "msd",
    digits: [{ shift: 12, width: 13 }],
    simdMsdOverride: true,
  });
  assert.equal(chooseTileLeaf(8192, (0xffn << 24n) | 0xffn, 13, 32, 4, true), "lsd");
  assert.equal(chooseTileLeaf(8192, (1n << 16n) - 1n, 13, 32, 4, true), "lsd");
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
