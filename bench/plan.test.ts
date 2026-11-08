// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { loadPlan, parsePlan, planSchema, selectCaseIds } from "./plan.ts";
import fixture from "./plans/selection.fixture.json" with { type: "json" };

test("the native selection fixture validates", () => {
  assert.equal(parsePlan(fixture.plan).harness, "smoke");
});

test("presets validate and overrides replace axes", async () => {
  for (const preset of ["local", "continuous", "smoke", "release"])
    assert.ok((await loadPlan(preset)).algorithms.includes("tessera"));
  const plan = await loadPlan("local", {
    types: ["u64", "row_u64"],
    distributions: ["random", "zipf2"],
    lengths: [1000],
  });
  assert.deepEqual(plan.include, [
    {
      types: ["u64", "row_u64"],
      distributions: ["random", "zipf2"],
      lengths: [1000],
      modes: ["api"],
    },
  ]);
  assert.deepEqual(
    parsePlan({
      ...plan,
      harness: "comparison",
      algorithms: ["glidesort"],
    }).algorithms,
    ["tessera", "std-stable", "glidesort"],
  );
});

test("invalid plans fail before execution", () => {
  const plan = fixture.plan;
  for (const invalid of [
    { ...plan, schema: 2 },
    { ...plan, includes: [] },
    { ...plan, include: [] },
    { ...plan, include: [{ type: ["u64"] }] },
    { ...plan, include: [{ types: ["u33"] }] },
    { ...plan, include: [{ distributions: ["typo"] }] },
    { ...plan, include: [{ types: [] }] },
    { ...plan, include: [{ lengths: [0] }] },
    { ...plan, include: [{ lengths: [1.5] }] },
    { ...plan, include: [{ lengths: [1, 1] }] },
    { ...plan, algorithms: ["glidesort"] },
    { ...plan, harness: "comparison", include: [{ modes: ["workspace"] }] },
  ])
    assert.throws(() => parsePlan(invalid));
});

test("selection uses actual IDs and rejects empty experiments", async () => {
  const ids = [
    "rust/u64/random/65536/s1/api",
    "rust/u64/random/65536/s1/workspace",
    "rust/u64/random/1000/s1/api",
  ];
  assert.deepEqual(selectCaseIds(ids, "/api$"), [ids[0], ids[2]]);
  assert.throws(() => selectCaseIds(ids, "^never$"), /no supported cases/);
  assert.throws(() => selectCaseIds([]), /no supported cases/);
  assert.equal(planSchema().type, "object");
});
