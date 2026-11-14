// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { titleErrors } from "./check_pr_title.ts";

test("titles check type, form and length but accept any lowercase scope", () => {
  assert.deepEqual(titleErrors("chore(release): prepare tessera-order 0.1.0"), []);
  assert.deepEqual(titleErrors("feat(engine)!: add a partition"), []);
  assert.deepEqual(titleErrors("docs: explain blocks"), []);
  assert.deepEqual(titleErrors("ci(rust-release): attach the crate"), []);
  assert.equal(titleErrors("feature: add a partition").length, 1);
  assert.equal(titleErrors("feat(Engine): add a partition").length, 1);
  assert.equal(titleErrors("docs: explain blocks.").length, 1);
  assert.equal(titleErrors(`docs: ${"x".repeat(60)}`).length, 1);
});
