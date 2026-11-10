// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { availableParallelism, tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { VARIANTS } from "../catalog.ts";
import { pairedRun } from "../continuous/fixtures/paired.ts";
import { loadPlan } from "../plan.ts";
import { comparisonCpus, measureBlock } from "./runner.ts";

test("paired processes select exact API IDs, pin CPUs and never overlap preparations", {
  skip: process.platform !== "linux" || availableParallelism() < 4,
}, async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-process-pair-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const plan = await loadPlan("continuous");
  await writeFile(join(directory, "plan.json"), JSON.stringify(plan));
  const binary = join(directory, "bench");
  const run = pairedRun(VARIANTS[0]);
  run.cases = ["rust/u64/random/49/s1/api", "rust/u64/random/49/s1/par-4"];
  run.blocks = [];
  await writeFile(
    binary,
    String.raw`#!/usr/bin/env node
// SPDX-License-Identifier: BSD-2-Clause
const fs = require("node:fs"), path = require("node:path"), assert = require("node:assert/strict");
const directory = ${JSON.stringify(directory)};
const lock = path.join(directory, "active");
const fd = fs.openSync(lock, "wx");
const ids = process.env.TESSERA_BENCH_IDS.split("\n");
assert.equal(ids.length, 1);
assert.equal(process.env.TESSERA_BENCH_PARALLEL, "4");
const allowed = fs.readFileSync("/proc/self/status", "utf8").match(/^Cpus_allowed_list:\s*(.+)$/m)[1];
const cpus = allowed.split(",").reduce((sum, range) => { const [a, b = a] = range.split("-").map(Number); return sum + b-a+1; }, 0);
assert.equal(cpus, ids[0].endsWith("/par-4") ? 4 : 1);
fs.appendFileSync(path.join(directory, "events"), ids[0] + "\n");
for (const id of ids) {
 const output = path.join(process.env.CRITERION_HOME, id, "new");
 fs.mkdirSync(output, {recursive:true});
 fs.writeFileSync(path.join(output, "benchmark.json"), JSON.stringify({full_id:id}));
 fs.writeFileSync(path.join(output, "estimates.json"), JSON.stringify({mean:{point_estimate:100}}));
}
fs.closeSync(fd); fs.unlinkSync(lock);
`,
    { mode: 0o755 },
  );
  const binaries = { baseline: binary, current: binary };
  const workspace = { directory, tree: directory, target: join(directory, "target") };
  const options = { root: directory, baseline: "HEAD", out: directory, simd: false, plan };
  const block = await measureBlock(run, binaries, workspace, options);
  for (const values of Object.values(block))
    assert.deepEqual(Object.keys(values).sort(), run.cases);
  const events = (await readFile(join(directory, "events"), "utf8")).trim().split("\n");
  assert.deepEqual(events, Array.from({ length: 3 }, () => run.cases).flat());
  // A serial-only shard of a parallel plan must still pin serial calls to one CPU.
  run.cases = run.cases.slice(0, 1);
  await measureBlock(run, binaries, workspace, options);
  await assert.rejects(comparisonCpus(true, "0"), /four available CPUs/);
});
