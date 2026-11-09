// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { stringify } from "smol-toml";
import { decodeConfig } from "./config.ts";
import { loadPlan, parsePlan, planSchema, selectCaseIds } from "./plan.ts";

const fixture = decodeConfig(
  readFileSync(new URL("./plans/selection.fixture.toml", import.meta.url), "utf8"),
  "selection.fixture.toml",
) as { plan: Record<string, unknown> };

test("parallel iteration defaults to eight workers only", async () => {
  assert.deepEqual((await loadPlan("parallel")).algorithms, [
    "tessera",
    "std-stable",
    "tessera-par-8",
    "rayon-stable-8",
  ]);
});

test("continuous parallel coverage is explicit and independent of serial coverage", async () => {
  const plan = await loadPlan("continuous");
  assert.deepEqual(plan.parallel, [
    {
      types: ["u32", "row_u32", "u64", "row_u64"],
      distributions: ["random"],
      lengths: [10000000],
    },
    { types: ["u64"], distributions: ["sorted", "reverse", "equal"], lengths: [10000000] },
    { types: ["row_u32"], distributions: ["skew99"], lengths: [10000000] },
  ]);
  assert.equal(plan.algorithms.length, 1);
  assert.throws(() => parsePlan({ ...plan, harness: "comparison" }), /smoke harness/);
  assert.throws(() => parsePlan({ ...plan, parallel: [{ types: ["typo"] }] }));
});

test("the native selection fixture validates", () => {
  assert.equal(parsePlan(fixture.plan).harness, "smoke");
});

test("file formats share validation", async () => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-plan-"));
  try {
    for (const preset of ["local", "continuous", "smoke", "release", "parallel"]) {
      const expected = await loadPlan(preset);
      for (const [extension, encode] of [
        ["json", (value: unknown) => JSON.stringify(value)],
        ["toml", stringify],
      ] as const) {
        const path = join(directory, `${preset}.${extension}`);
        await writeFile(path, encode(expected));
        assert.deepEqual(await loadPlan(path), expected);
        const override = { lengths: [1234] };
        assert.deepEqual(await loadPlan(path, override), await loadPlan(preset, override));
        await writeFile(path, encode({ ...expected, include: [{ types: ["typo"] }] }));
        await assert.rejects(loadPlan(path), /types/);
        await writeFile(path, encode({ ...expected, unexpected: true }));
        await assert.rejects(loadPlan(path), /unexpected/);
      }
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("presets validate and overrides replace axes", async () => {
  for (const preset of ["local", "continuous", "smoke", "release", "parallel"])
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
    { ...plan, include: [{ modes: ["api"] }] },
    { ...plan, include: [{ modes: ["workspace"] }] },
    { ...plan, harness: "comparison", include: [{ modes: ["unstable"] }] },
  ])
    assert.throws(() => parsePlan(invalid));
});

test("selection uses actual IDs and rejects empty experiments", async () => {
  const ids = [
    "rust/u64/random/65536/s1/api",
    "rust/u64/d16/65536/s1/api",
    "rust/u64/random/1000/s1/api",
  ];
  assert.deepEqual(selectCaseIds(ids, "/random/"), [ids[0], ids[2]]);
  assert.throws(() => selectCaseIds(ids, "^never$"), /no supported cases/);
  assert.throws(() => selectCaseIds([]), /no supported cases/);
  assert.equal(planSchema().type, "object");
});
