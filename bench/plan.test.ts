// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { decodeConfig } from "./config.ts";
import { loadPlan, parsePlan, planSchema, plansByOperation, selectCaseIds } from "./plan.ts";

const fixture = decodeConfig(
  readFileSync(new URL("./plans/selection.fixture.toml", import.meta.url), "utf8"),
  "selection.fixture.toml",
) as { plan: Record<string, unknown> };

test("presets load through the same validator and overrides replace selection", async () => {
  for (const preset of ["local", "continuous", "smoke", "release", "parallel"])
    assert.ok((await loadPlan(preset)).implementations.includes("tessera"));
  const plan = await loadPlan("local", {
    types: ["u64"],
    distributions: ["random"],
    lengths: [1234],
  });
  assert.deepEqual(plan.include, [{ types: ["u64"], distributions: ["random"], lengths: [1234] }]);
  assert.deepEqual(parsePlan(fixture.plan).implementations, ["tessera", "rayon-stable"]);
  assert.deepEqual(
    (await loadPlan("continuous", { implementations: ["glidesort"] })).implementations,
    ["glidesort"],
  );
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
  assert.deepEqual(parsePlan({ ...plan, implementations: ["tessera"] }).include, plan.include);
});

test("malformed and unsupported plans fail before execution", () => {
  for (const invalid of [
    { ...fixture.plan, schema: 2 },
    { ...fixture.plan, operation: { kind: "argsort" } },
    { ...fixture.plan, harness: "smoke" },
    { ...fixture.plan, unexpected: true },
    { ...fixture.plan, include: [] },
    { ...fixture.plan, include: [{ types: ["u33"] }] },
    { ...fixture.plan, include: [{ lengths: [0] }] },
    { ...fixture.plan, include: [{ modes: ["workspace"] }] },
    { ...fixture.plan, parallel: [{ types: ["u64"] }] },
    { ...fixture.plan, include: [{ executions: ["par-4"] }] },
    { ...fixture.plan, include: [{ executions: ["t0"] }] },
  ])
    assert.throws(() => parsePlan(invalid));
  assert.throws(() => decodeConfig("{", "plan.json"));
  assert.throws(() => decodeConfig("types = [", "plan.toml"));
  const ids = ["rust/sort/u64/random/65536/s1/serial", "rust/sort/u64/d16/65536/s1/serial"];
  assert.deepEqual(selectCaseIds(ids, "/random/"), [ids[0]]);
  assert.throws(() => selectCaseIds(ids, "^never$"), /no supported cases/);
});

test("operation overrides retain exact parameters and reject unrelated flags", async () => {
  const selected = await loadPlan("local", {
    operation: "select_nth",
    index: 99,
    lengths: [100],
  });
  assert.ok(selected.include.length > 0);
  for (const rule of selected.include)
    assert.deepEqual(rule.operation, { kind: "select_nth", index: 99 });
  for (const invalid of [
    { operation: "argsort" as const, index: 1 },
    { operation: "select_nth" as const },
    { operation: "select_nth" as const, index: 100, lengths: [100] },
    { operation: "select_nth" as const, index: 5, lengths: ["tile-next" as const] },
    { index: 5 },
    { operation: "partition-buckets" as const, bucketCount: 0 },
  ])
    await assert.rejects(loadPlan("local", invalid));
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
  for (const rule of parsePlan(JSON.parse(result.stdout)).include)
    assert.deepEqual(rule.operation, { kind: "partition-buckets", bucket_count: 7 });
});

test("plans, the editor schema and the CLI admit only current operations", () => {
  const cli = (...args: string[]) =>
    spawnSync(
      process.execPath,
      [fileURLToPath(new URL("../tools/index.ts", import.meta.url)), "plan", "local", ...args],
      { encoding: "utf8" },
    );
  const schema = JSON.stringify(planSchema());
  const help = cli("--help");
  assert.equal(help.status, 0, help.stderr);
  assert.match(
    help.stdout.replace(/\s+/g, " "),
    /choices: "sort", "argsort", "select_nth", "partition-buckets"/,
  );
  for (const operation of [
    { kind: "partition" },
    { kind: "select", index: 10 },
    { kind: "segments", segment_len: 73 },
    { kind: "top-k", k: 10 },
    { kind: "prefix", prefix_len: 10 },
    { kind: "rank-range", start: 10, end: 20 },
    { kind: "select-many", indices: [99, 0, 99] },
  ]) {
    const rule = { lengths: [100] };
    assert.throws(() =>
      parsePlan({ schema: 3, implementations: ["tessera"], include: [{ ...rule, operation }] }),
    );
    assert.ok(!schema.includes(JSON.stringify(operation.kind)));
    const result = cli("--operation", operation.kind);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Allowed choices/);
  }
  for (const flag of ["--k", "--prefix-len", "--indices", "--segment-len", "--rank-range"]) {
    assert.ok(!help.stdout.includes(`${flag} `));
    assert.match(cli(flag, "1").stderr, /unknown option/);
  }
});

test("plans place operations on rules; an override replaces them", async () => {
  const rule = { types: ["u64"], distributions: ["random"], lengths: [1000] };
  const select = { ...rule, operation: { kind: "select_nth", index: 10 } };
  parsePlan({ schema: 3, implementations: ["tessera"], include: [rule, select] });
  for (const invalid of [
    { schema: 3, implementations: ["tessera"], include: [rule], parallel: [select] },
    { schema: 3, implementations: ["tessera"], include: [{ ...select, lengths: ["tile-next"] }] },
    { schema: 3, implementations: ["tessera"], include: [{ ...select, lengths: [5] }] },
  ])
    assert.throws(() => parsePlan(invalid));
  const continuous = await loadPlan("continuous");
  assert.deepEqual(
    continuous.include.flatMap((entry) => entry.operation ?? []),
    [
      { kind: "argsort" },
      { kind: "partition-buckets", bucket_count: 16 },
      { kind: "select_nth", index: 32768 },
      { kind: "partition-buckets", bucket_count: 65537 },
      { kind: "argsort" },
      { kind: "select_nth", index: 32768 },
      { kind: "select_nth", index: 0 },
      { kind: "select_nth", index: 65535 },
      { kind: "select_nth", index: 0 },
      { kind: "select_nth", index: 65535 },
      { kind: "argsort" },
      { kind: "argsort" },
      { kind: "argsort" },
      { kind: "select_nth", index: 4 },
      { kind: "argsort" },
      { kind: "select_nth", index: 32768 },
    ],
  );
  const sorting = await loadPlan("continuous", { operation: "sort" });
  assert.ok(sorting.include.length > 0 && sorting.include.every((entry) => !entry.operation));
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

test("a plan splits into one session plan per operation instance", async () => {
  const plan = await loadPlan("continuous");
  const parts = plansByOperation(plan);
  // Every rule lands in exactly one part, and each part selects one operation instance.
  assert.equal(
    parts.reduce((total, part) => total + part.include.length, 0),
    plan.include.length,
  );
  const keys = parts.map((part) => {
    const operations = new Set(part.include.map((rule) => JSON.stringify(rule.operation ?? null)));
    assert.equal(operations.size, 1);
    return [...operations][0];
  });
  assert.equal(new Set(keys).size, parts.length);
  // Selection at two ranks is two sessions.
  assert.ok(keys.filter((key) => key.includes("select_nth")).length >= 2);
  assert.equal(keys[0], "null");
});
