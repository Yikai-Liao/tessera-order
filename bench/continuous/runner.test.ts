// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { benchmarkGroups } from "../../.github/scripts/ci_changes.ts";
import { operationKinds, parseCriterionId, revisionSampling } from "../catalog.ts";
import { revisionFeatures, shardCases } from "../local/model.ts";
import { loadPlan } from "../plan.ts";
import { PAIRED_SHARDS, writePaired } from "./fixtures/paired.ts";
import { baseHash, hash } from "./fixtures/report.ts";
import { continuousShards, pairedBmf, readPaired } from "./paired.ts";
import { selectContinuous } from "./selection.ts";

const source = { hash, baseHash };

test("entry scope selects complete groups and small serial evidence publishes", async (t) => {
  const full = await loadPlan("continuous");
  const entryGroups = benchmarkGroups(
    ["algorithms", "cached", "partition"].map((name) => `rust/tessera-order/src/${name}.rs`),
  );
  assert.deepEqual(entryGroups, operationKinds.filter((kind) => kind !== "sort").toSorted());
  const selection = benchmarkGroups(["rust/tessera-order/src/cached.rs"]);
  const plan = selectContinuous(full, (selection as string[]).join(","));
  assert.deepEqual(plan.algorithms, ["tessera"]);
  assert.ok(plan.include.every((rule) => rule.operation?.kind === "partition-buckets"));
  assert.deepEqual(revisionFeatures(plan), ["operations"]);
  const parallel = selectContinuous(full, "parallel");
  assert.deepEqual(parallel.algorithms, ["tessera-par-4"]);
  assert.deepEqual(parallel.include, full.parallel);
  assert.deepEqual(revisionFeatures(parallel), ["rayon"]);
  const mixed = selectContinuous(full, "argsort,parallel");
  assert.deepEqual(mixed.parallel, full.parallel);
  assert.deepEqual(revisionFeatures(mixed), ["operations", "rayon"]);
  assert.equal(selectContinuous(full, "all"), full);
  for (const groups of ["", "missing", "argsort,argsort"])
    assert.throws(() => selectContinuous(full, groups));
  const out = await mkdtemp(join(tmpdir(), "tessera-small-scope-"));
  t.after(() => rm(out, { recursive: true, force: true }));
  const ids = ["u64", "row_u64"]
    .map((ty) => `rust/${ty}/random/65536/s1/partition-buckets/16/api`)
    .sort();
  await writePaired(out, 1.2, plan, ids);
  const runs = await readPaired(out, source, join(out, "prepared"));
  assert.equal(runs.length, 1);
  assert.deepEqual(Object.keys(pairedBmf(runs)).sort(), ids);
});

test("hosted matrix follows the frozen inventory", async (t) => {
  const out = await mkdtemp(join(tmpdir(), "tessera-matrix-"));
  t.after(() => rm(out, { recursive: true, force: true }));
  await writePaired(out);
  const result = spawnSync(
    process.execPath,
    ["tools/index.ts", "continuous", "matrix", join(out, "prepared")],
    {
      encoding: "utf8",
    },
  );
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).include.length, PAIRED_SHARDS);
  assert.equal(continuousShards(["rust/u64/random/65536/s1/argsort/api"]), 1);
});

test("all shards cover the same input inventory once, including four-thread modes", async (t) => {
  const out = await mkdtemp(join(tmpdir(), "tessera-shards-"));
  t.after(() => rm(out, { recursive: true, force: true }));
  const runs = await writePaired(out);
  const selected = runs.flatMap((run) => run.cases).sort();
  assert.deepEqual(selected, runs[0].shard!.allCases);
  // Actions names bind a shard to its prepared artifact; JSON owns its index.
  for (const run of runs) {
    const name = `paired-${run.shard!.index}`;
    await rename(join(out, name), join(out, `${name}-12345`));
  }
  const loaded = await readPaired(out, source, join(out, "prepared"));
  const bmf = pairedBmf(loaded);
  assert.equal(Object.keys(bmf).length, selected.length);
  assert.equal(Object.values(bmf)[0]["paired-time-ratio"].value, 1.2);
  assert.deepEqual(shardCases([...selected].reverse(), 1, PAIRED_SHARDS), runs[0].cases);
});

test("rejects a self-consistent but shortened inventory across every shard", async (t) => {
  const out = await mkdtemp(join(tmpdir(), "tessera-shard-truncated-"));
  t.after(() => rm(out, { recursive: true, force: true }));
  const runs = await writePaired(out);
  const trusted = runs[0].shard!.allCases;
  const omitted = trusted.find((id) => id.endsWith("/par-4"));
  const shortened = trusted.filter((id) => id !== omitted);
  for (const run of runs) {
    run.shard!.allCases = shortened;
    run.cases = shardCases(shortened, run.shard!.index, PAIRED_SHARDS);
    run.sampleCounts = Object.fromEntries(
      run.cases.map((id) => [id, revisionSampling(parseCriterionId(id).len).samples]),
    );
    run.samples = Object.fromEntries(
      run.cases.map((id) => [
        id,
        Array.from({ length: revisionSampling(parseCriterionId(id).len).samples }, () => ({
          baseline: 100,
          current: 120,
        })),
      ]),
    );
    await writeFile(join(out, `paired-${run.shard!.index}`, "local.json"), JSON.stringify(run));
  }
  // Agreement between shards cannot replace this run’s prepared inventory.
  await assert.rejects(readPaired(out, source, join(out, "prepared")), /Incompatible/);
});

for (const damage of [
  "missing",
  "duplicate",
  "head",
  "baseline",
  "plan",
  "blocks",
  "selection",
  "settings",
  "features",
  "invalid-time",
  "provenance",
  "different-build",
  "aslr",
  "budget",
  "missing-budget",
] as const) {
  test(`rejects ${damage} evidence before upload`, async (t) => {
    const out = await mkdtemp(join(tmpdir(), "tessera-shard-invalid-"));
    t.after(() => rm(out, { recursive: true, force: true }));
    const runs = await writePaired(out);
    const run = runs[0];
    const path = join(out, `paired-1`, "local.json");
    if (damage === "missing") await rm(path);
    else if (damage === "plan") await writeFile(join(out, `paired-1`, "plan.json"), "{}");
    else {
      if (damage === "duplicate") run.shard!.index = 2;
      if (damage === "aslr") delete run.host.aslr;
      if (damage === "head") run.head = "c".repeat(40);
      if (damage === "baseline") run.baseline = hash;
      if (damage === "blocks") run.samples![run.cases[0]].pop();
      if (damage === "missing-budget") delete (run as Partial<typeof run>).sampleCounts;
      if (damage === "budget") {
        run.sampleCounts![run.cases[0]]--;
        run.samples![run.cases[0]].pop();
      }
      if (damage === "selection") run.cases.pop();
      if (damage === "settings") run.builds!.current.opt_level = "1";
      if (damage === "features") run.builds!.current.features = [];
      if (damage === "invalid-time") run.samples![run.cases[0]][0].current = -1;
      if (damage === "provenance") delete (run.source as Partial<typeof run.source>).current;
      if (damage === "different-build") run.binaries.current = "1".repeat(64);
      await writeFile(path, JSON.stringify(run));
    }
    await assert.rejects(readPaired(out, source, join(out, "prepared")));
    assert.ok(await readFile(join(out, `paired-2`, "local.json"), "utf8"));
  });
}

test("report uses the prepared plan from its own run instead of a publisher preset", async (t) => {
  const out = await mkdtemp(join(tmpdir(), "tessera-branch-plan-"));
  t.after(() => rm(out, { recursive: true, force: true }));
  await writePaired(out);
  const prepared = join(out, "prepared");
  const pair = JSON.parse(await readFile(join(prepared, "prepared.json"), "utf8"));
  const plan = pair.plan;
  plan.include = [
    { types: ["u64"], distributions: ["random"], lengths: [48, 49, 1000, 65536, 1000000] },
    ...plan.include.filter((rule: { operation?: unknown }) => rule.operation),
  ];
  await writeFile(join(prepared, "prepared.json"), JSON.stringify(pair));
  for (const dir of [...Array.from({ length: PAIRED_SHARDS }, (_, i) => `paired-${i + 1}`)])
    await writeFile(join(out, dir, "plan.json"), JSON.stringify(plan));
  assert.equal((await readPaired(out, source, prepared)).length, PAIRED_SHARDS);
});
