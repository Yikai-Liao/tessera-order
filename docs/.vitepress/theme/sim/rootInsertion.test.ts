// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { rootInsertion } from "./rootInsertion.ts";

for (const records of [47, 48]) {
  assert.equal(rootInsertion(records, 8), "finish");
  assert.equal(rootInsertion(records, 16), "finish");
  assert.equal(rootInsertion(records, 2048), "budget");
  assert.equal(rootInsertion(records, 8, 128), "budget");
}
assert.equal(rootInsertion(49, 8), "budget");
assert.equal(rootInsertion(48, 8, 384), "finish");
assert.equal(rootInsertion(48, 8, 383), "budget");
