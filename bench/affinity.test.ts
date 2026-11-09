// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { cpuListSchema } from "./affinity.ts";

test("affinity accepts CPU lists and ranges", () => {
  for (const value of ["2", "0,2,4,6,8,10,12,14", "0-7", "0-3,8-11"])
    assert.equal(cpuListSchema.parse(value), value);
  for (const value of ["", "-1", "1,,2", "2-1", "1-", "1:2", "1.5", "9007199254740992"])
    assert.equal(cpuListSchema.safeParse(value).success, false, value);
});
