// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { parseCriterionId, revisionSampling, VARIANTS } from "../catalog.ts";
import { shardCases } from "../local/model.ts";
import { writePaired } from "./fixtures/paired.ts";
import { baseHash, hash } from "./fixtures/report.ts";
import { CONTINUOUS_SHARDS, pairedBmf, readPaired } from "./paired.ts";

const source = { hash, baseHash };

test("hosted matrix uses catalog configurations and the shared shard count", () => {
  const result = spawnSync(process.execPath, ["tools/index.ts", "continuous", "matrix"], {
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  const matrix = JSON.parse(result.stdout).include;
  assert.equal(matrix.length, VARIANTS.length * CONTINUOUS_SHARDS);
  for (const variant of VARIANTS)
    assert.equal(
      matrix.filter((entry: { configuration: string }) => entry.configuration === variant.id)
        .length,
      CONTINUOUS_SHARDS,
    );
});

test("all shards cover the same input inventory once, including four-thread modes", async (t) => {
  const out = await mkdtemp(join(tmpdir(), "tessera-shards-"));
  t.after(() => rm(out, { recursive: true, force: true }));
  const runs = await writePaired(out, VARIANTS[0]);
  const selected = runs.flatMap((run) => run.cases).sort();
  assert.deepEqual(selected, runs[0].shard!.allCases);
  const loaded = await readPaired(out, VARIANTS[0], source, join(out, "prepared"));
  const bmf = pairedBmf(loaded);
  assert.equal(Object.keys(bmf).length, selected.length);
  assert.equal(Object.values(bmf)[0]["paired-time-ratio"].value, 1.2);
  assert.deepEqual(shardCases([...selected].reverse(), 1, CONTINUOUS_SHARDS), runs[0].cases);
});

test("rejects a self-consistent but shortened inventory across every shard", async (t) => {
  const out = await mkdtemp(join(tmpdir(), "tessera-shard-truncated-"));
  t.after(() => rm(out, { recursive: true, force: true }));
  const runs = await writePaired(out, VARIANTS[0]);
  const trusted = runs[0].shard!.allCases;
  const shortened = trusted.filter((id) => !id.endsWith("/par-4"));
  for (const run of runs) {
    run.shard!.allCases = shortened;
    run.cases = shardCases(shortened, run.shard!.index, CONTINUOUS_SHARDS);
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
    await writeFile(
      join(out, `paired-${VARIANTS[0].id}-${run.shard!.index}`, "local.json"),
      JSON.stringify(run),
    );
  }
  // Agreement between shards cannot replace this run’s prepared inventory.
  await assert.rejects(readPaired(out, VARIANTS[0], source, join(out, "prepared")), /Incompatible/);
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
  "aslr",
  "budget",
  "missing-budget",
] as const) {
  test(`rejects ${damage} evidence before upload`, async (t) => {
    const out = await mkdtemp(join(tmpdir(), "tessera-shard-invalid-"));
    t.after(() => rm(out, { recursive: true, force: true }));
    const runs = await writePaired(out, VARIANTS[0]);
    const run = runs[0];
    const path = join(out, `paired-${VARIANTS[0].id}-1`, "local.json");
    if (damage === "missing") await rm(path);
    else if (damage === "plan")
      await writeFile(join(out, `paired-${VARIANTS[0].id}-1`, "plan.json"), "{}");
    else {
      if (damage === "duplicate") run.shard!.index = 2;
      if (damage === "aslr") delete run.host.aslr;
      if (damage === "head") run.head = "c".repeat(40);
      if (damage === "baseline") run.baseline = hash;
      if (damage === "blocks") run.samples![run.cases[0]].pop();
      if (damage === "missing-budget") delete run.sampleCounts;
      if (damage === "budget") {
        run.sampleCounts![run.cases[0]]--;
        run.samples![run.cases[0]].pop();
      }
      if (damage === "selection") run.cases.pop();
      if (damage === "settings") run.builds!.current.opt_level = "1";
      if (damage === "features") run.builds!.current.features = [];
      if (damage === "invalid-time") run.samples![run.cases[0]][0].current = -1;
      if (damage === "provenance") delete (run.source as Partial<typeof run.source>).current;
      await writeFile(path, JSON.stringify(run));
    }
    await assert.rejects(readPaired(out, VARIANTS[0], source, join(out, "prepared")));
    assert.ok(await readFile(join(out, `paired-${VARIANTS[0].id}-2`, "local.json"), "utf8"));
  });
}

test("report uses the prepared plan from its own run instead of a publisher preset", async (t) => {
  const out = await mkdtemp(join(tmpdir(), "tessera-branch-plan-"));
  t.after(() => rm(out, { recursive: true, force: true }));
  await writePaired(out, VARIANTS[0]);
  const prepared = join(out, "prepared");
  const plan = JSON.parse(await readFile(join(prepared, "plan.json"), "utf8"));
  plan.include = [
    { types: ["u64"], distributions: ["random"], lengths: [48, 49, 1000, 65536, 1000000] },
  ];
  for (const dir of [
    "prepared",
    ...Array.from({ length: CONTINUOUS_SHARDS }, (_, i) => `paired-${VARIANTS[0].id}-${i + 1}`),
  ])
    await writeFile(join(out, dir, "plan.json"), JSON.stringify(plan));
  assert.equal((await readPaired(out, VARIANTS[0], source, prepared)).length, CONTINUOUS_SHARDS);
});
