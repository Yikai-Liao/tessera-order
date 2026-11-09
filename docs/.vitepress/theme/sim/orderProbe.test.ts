// SPDX-License-Identifier: BSD-2-Clause
import assert from "node:assert/strict";
import { orderConfirmationPlan, orderProbePlan } from "./orderProbe.ts";

for (const records of [2, 49, 129, 8192, 65536]) {
  assert.deepEqual(orderConfirmationPlan(records, "ascending"), [
    { direction: "ascending", pairs: records - 1 },
  ]);
  for (const leadingTies of [0, Math.floor(records / 2) - 1]) {
    assert.deepEqual(orderConfirmationPlan(records, "descending", leadingTies), [
      { direction: "ascending", pairs: leadingTies + 1 },
      { direction: "descending", pairs: records - 1 },
    ]);
  }
}

for (const records of [2, 49, 129, 1000, 4096, 8191]) {
  assert.deepEqual(orderProbePlan(records), [{ kind: "confirm", pairs: records - 1 }]);
}
for (const records of [8192, 65536, 1_000_000]) {
  const stride = Math.floor(records / 4096);
  assert.deepEqual(orderProbePlan(records), [
    { kind: "sample", pairs: Math.floor((records - 1) / stride) },
    { kind: "confirm", pairs: records - 1 },
  ]);
}
