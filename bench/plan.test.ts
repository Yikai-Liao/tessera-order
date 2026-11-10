// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { decodeConfig } from "./config.ts";
import { loadPlan, parsePlan, selectCaseIds } from "./plan.ts";

const fixture = decodeConfig(
  readFileSync(new URL("./plans/selection.fixture.toml", import.meta.url), "utf8"),
  "selection.fixture.toml",
) as { plan: Record<string, unknown> };

test("presets load through the same validator and overrides replace selection", async () => {
  for (const preset of ["local", "continuous", "smoke", "release", "parallel"])
    assert.ok((await loadPlan(preset)).algorithms.includes("tessera"));
  const plan = await loadPlan("local", {
    types: ["u64"],
    distributions: ["random"],
    lengths: [1234],
  });
  assert.deepEqual(plan.include, [{ types: ["u64"], distributions: ["random"], lengths: [1234] }]);
  assert.equal(parsePlan(fixture.plan).harness, "smoke");
  assert.deepEqual(parsePlan(decodeConfig(JSON.stringify(plan), "plan.json")), plan);
});

test("malformed and unsupported plans fail before execution", () => {
  for (const invalid of [
    { ...fixture.plan, schema: 2 },
    { ...fixture.plan, unexpected: true },
    { ...fixture.plan, include: [] },
    { ...fixture.plan, include: [{ types: ["u33"] }] },
    { ...fixture.plan, include: [{ lengths: [0] }] },
    { ...fixture.plan, include: [{ modes: ["workspace"] }] },
    { ...fixture.plan, parallel: [{ types: ["typo"] }] },
  ])
    assert.throws(() => parsePlan(invalid));
  assert.throws(() => decodeConfig("{", "plan.json"));
  assert.throws(() => decodeConfig("types = [", "plan.toml"));
  const ids = ["rust/u64/random/65536/s1/api", "rust/u64/d16/65536/s1/api"];
  assert.deepEqual(selectCaseIds(ids, "/random/"), [ids[0]]);
  assert.throws(() => selectCaseIds(ids, "^never$"), /no supported cases/);
});
