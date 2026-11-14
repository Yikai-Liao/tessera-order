// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
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
  assert.deepEqual(parsePlan(fixture.plan).algorithms, ["tessera"]);
  assert.deepEqual((await loadPlan("continuous", { algorithms: ["glidesort"] })).algorithms, [
    "glidesort",
  ]);
  assert.deepEqual(parsePlan(decodeConfig(JSON.stringify(plan), "plan.json")), plan);
});

test("Reverse direct keys are opt-in comparison inputs", async () => {
  const types = ["reverse_u64", "reverse_f64"];
  const plan = await loadPlan("smoke", {
    types,
    distributions: ["sorted", "sorted99", "far_swaps", "random"],
    lengths: [63, 64, 1000],
  });
  assert.ok(plan.include.every((rule) => rule.types?.join() === types.join()));
  for (const preset of ["smoke", "release", "parallel"]) {
    const defaults = await loadPlan(preset);
    assert.ok(
      defaults.include.every((rule) => rule.types && !rule.types.some((id) => types.includes(id))),
    );
  }
  assert.deepEqual(parsePlan({ ...plan, algorithms: ["tessera"] }).include, plan.include);
});

test("malformed and unsupported plans fail before execution", () => {
  for (const invalid of [
    { ...fixture.plan, schema: 2 },
    { ...fixture.plan, harness: "smoke" },
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

test("operation overrides retain exact raw requests and reject unrelated flags", async () => {
  const selected = await loadPlan("local", {
    operation: "select-many",
    indices: [99, 0, 99],
    lengths: [100],
  });
  assert.equal(selected.schema, 2);
  assert.deepEqual(selected.operation, { kind: "select-many", indices: [99, 0, 99] });
  for (const invalid of [
    { operation: "argsort" as const, prefixLen: 1 },
    { operation: "top-k" as const },
    { operation: "top-k" as const, k: 101, lengths: [100] },
    { operation: "prefix" as const, prefixLen: 5, lengths: ["tile-next" as const] },
    { prefixLen: 5 },
  ])
    await assert.rejects(loadPlan("local", invalid));
  assert.deepEqual(
    (await loadPlan("local", { operation: "partition-buckets", bucketCount: 7 })).operation,
    { kind: "partition-buckets", bucket_count: 7 },
  );
  await assert.rejects(loadPlan("local", { operation: "partition-buckets", bucketCount: 0 }));
  await assert.rejects(loadPlan("local", { operation: "partition", bucketCount: 2 }));
  const result = spawnSync(
    process.execPath,
    [
      fileURLToPath(new URL("../tools/index.ts", import.meta.url)),
      "plan",
      "local",
      "--operation",
      "partition-buckets",
      "--bucket-count",
      "7",
      "--lengths",
      "100",
    ],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(parsePlan(JSON.parse(result.stdout)).operation, {
    kind: "partition-buckets",
    bucket_count: 7,
  });
});

test("plan CLI defaults to JSON and retains explicit TOML export", async () => {
  const expected = await loadPlan("local");
  for (const format of ["json", "toml"]) {
    const result = spawnSync(
      process.execPath,
      [
        fileURLToPath(new URL("../tools/index.ts", import.meta.url)),
        "plan",
        "local",
        ...(format === "toml" ? ["--format", format] : []),
      ],
      { encoding: "utf8" },
    );
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(parsePlan(decodeConfig(result.stdout, `plan.${format}`)), expected);
  }
});
