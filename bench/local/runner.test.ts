// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { availableParallelism, tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { catalog, VARIANTS } from "../catalog.ts";
import { pairedRun } from "../continuous/fixtures/paired.ts";
import { summarizeLocal } from "./model.ts";
import { renderLocalReport } from "./report.ts";
import { comparisonCpus } from "./runner.ts";
import { measureSamples } from "./samples.ts";

test("retained budgets survive catalog changes without accepting incomplete samples", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-retained-budget-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const run = pairedRun(VARIANTS[0]);
  for (const id of run.cases) {
    run.sampleCounts![id]++;
    run.samples![id].push(run.samples![id][0]);
  }
  await writeFile(join(directory, "local.json"), JSON.stringify(run));
  const budgets = catalog.revisionSampling.map((s) => s.samples);
  try {
    for (const settings of catalog.revisionSampling) settings.samples += 2;
    await renderLocalReport(directory);
    assert.equal(summarizeLocal(run).length, run.cases.length);
    // Missing or incomplete budgets cannot be regenerated as complete evidence.
    for (const id of run.cases) run.samples![id].pop();
    assert.throws(() => summarizeLocal(run), /budget/);
    delete run.sampleCounts;
    assert.throws(() => summarizeLocal(run), /budget/);
  } finally {
    catalog.revisionSampling.forEach((s, i) => {
      s.samples = budgets[i];
    });
  }
});

test("revision workers alternate adjacent samples, retain evidence and reap failures", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-samples-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const mask =
    process.platform === "linux" && availableParallelism() >= 4
      ? await comparisonCpus(true)
      : undefined;
  const binaries = { baseline: join(directory, "a"), current: join(directory, "b") };
  for (const [role, file] of Object.entries(binaries))
    await writeFile(
      file,
      String.raw`#!/usr/bin/env node
const fs = require("node:fs"), readline = require("node:readline");
const role = ${JSON.stringify(role)}, dir = ${JSON.stringify(directory)};
fs.writeFileSync(dir + "/" + role + ".pid", String(process.pid));
fs.appendFileSync(dir + "/pids", process.pid + "\n");
const id = process.env.TESSERA_BENCH_IDS;
if (${JSON.stringify(process.platform)} === "linux") {
 const personality = parseInt(fs.readFileSync("/proc/self/personality", "utf8"),16);
 require("node:assert/strict").ok(personality & 0x40000, "ASLR must be disabled");
}
if (${Boolean(mask)}) {
 const allowed = fs.readFileSync("/proc/self/status", "utf8").match(/^Cpus_allowed_list:\s*(.+)$/m)[1];
 const count = allowed.split(",").reduce((n,r) => {const [a,b=a]=r.split("-").map(Number);return n+b-a+1},0);
 require("node:assert/strict").equal(count, fs.existsSync(dir + "/serial-only") ? 2 : id.endsWith("/par-4") ? 4 : 1);
}
const rl = readline.createInterface({input:process.stdin});
rl.on("line", request => {
 const lock = fs.openSync(dir + "/active", "wx");
 fs.appendFileSync(dir + "/events", role + ":" + request + "\n");
 fs.closeSync(lock); fs.unlinkSync(dir + "/active");
 if (fs.existsSync(dir + "/fail") && role === "current") process.exit(2);
 if (request === "start") console.log(JSON.stringify({id}));
 if (request === "warm") console.log(JSON.stringify({id,samples:30}));
 if (request === "sample") console.log(JSON.stringify({elapsed_ns:role === "baseline" ? 1000 : 1200,sorts:10}));
 if (request === "stop") {console.log("{}");process.exit(0)}
});
`,
      { mode: 0o755 },
    );
  const run = pairedRun(VARIANTS[0]);
  run.cases = ["rust/u64/random/48/s1/api", ...(mask ? ["rust/u64/random/49/s1/par-4"] : [])];
  await measureSamples(run, binaries, "unused", mask, async () => {});
  if (mask) await assert.rejects(comparisonCpus(true, "0"), /four available CPUs/);
  const events = (await readFile(join(directory, "events"), "utf8")).trim().split("\n");
  assert.deepEqual(
    events.filter((s) => s.endsWith(":sample")),
    Array.from({ length: 30 * run.cases.length }, (_, i) =>
      i % 2 ? ["current:sample", "baseline:sample"] : ["baseline:sample", "current:sample"],
    ).flat(),
  );
  const pids = (await readFile(join(directory, "pids"), "utf8")).trim().split("\n");
  assert.equal(new Set(pids).size, 2 * 30 * run.cases.length);
  assert.equal(summarizeLocal(run)[0].verdict, "slower");
  await writeFile(join(directory, "local.json"), JSON.stringify(run));
  const result = await renderLocalReport(directory);
  assert.equal(result.kind, "revision-comparison");
  assert.equal(result.ratio, "current / baseline");
  assert.ok("slower" in result);
  assert.equal(result.slower, run.cases.length);
  assert.match(await readFile(join(directory, "index.md"), "utf8"), /Adjacent A\/B/);
  Object.assign(run, { protocol: "paired-samples-v1" });
  assert.throws(() => summarizeLocal(run), /Unsupported/);
  run.protocol = "paired-samples-v2";
  await writeFile(join(directory, "fail"), "");
  await assert.rejects(measureSamples(run, binaries, "unused", mask, async () => {}));
  for (const role of ["baseline", "current"]) {
    const pid = Number(await readFile(join(directory, `${role}.pid`), "utf8"));
    assert.throws(() => process.kill(pid, 0), { code: "ESRCH" });
  }
  if (mask) {
    await rm(join(directory, "fail"));
    await writeFile(join(directory, "serial-only"), "");
    run.cases = [run.cases[0]];
    delete run.shard;
    const serialMask = mask.split(",").slice(0, 2).join(",");
    await measureSamples(run, binaries, "unused", serialMask, async () => {});
  }
});
