// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { backendFeatures } from "./runner.ts";

test("revision comparison enables historical SIMD without changing current builds", () => {
  assert.deepEqual(backendFeatures('[features]\nportable-simd = ["dep:fearless_simd"]\n'), [
    "tessera-sort/portable-simd",
  ]);
  for (const dependency of [
    '"1.2"',
    '{ version = "1.2" }',
    '{ version = "1.2", optional = false }',
  ]) {
    assert.deepEqual(backendFeatures("[dependencies]\nfearless_simd = " + dependency), []);
  }
  for (const manifest of [
    "[dependencies]\n",
    '[dependencies]\nfearless_simd = { version = "1.2", optional = true }',
  ]) {
    assert.throws(() => backendFeatures(manifest), /no supported SIMD backend/);
  }
});
