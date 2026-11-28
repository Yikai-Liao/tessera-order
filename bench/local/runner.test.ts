// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { availableParallelism, tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { catalog, criterionId, SORT } from "../catalog.ts";
import { pairedRun } from "../continuous/fixtures/paired.ts";
import { loadPlan, planFeatures } from "../plan.ts";
import { simdFeatures } from "../workspace.ts";
import { revisionThreads, summarizeLocal } from "./model.ts";
import { renderLocalReport } from "./report.ts";
import { comparisonCpus, inspectGoPair, selectedCases, serialBaselineManifest } from "./runner.ts";
import { measureSamples, serialLanes } from "./samples.ts";

test("retained budgets survive catalog changes without accepting incomplete samples", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-retained-budget-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const run = pairedRun();
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
    delete (run as Partial<typeof run>).sampleCounts;
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
      ? await comparisonCpus(4)
      : undefined;
  const binaries = { baseline: join(directory, "a"), current: join(directory, "b") };
  for (const [role, file] of Object.entries(binaries))
    await writeFile(
      file,
      String.raw`#!/usr/bin/env bun
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
 require("node:assert/strict").equal(count, fs.existsSync(dir + "/serial-only") ? 2 : id.endsWith("/t4") ? 4 : 1);
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
  const run = pairedRun();
  run.cases = [
    "rust/sort/u64/random/48/s1/serial",
    ...(mask ? ["rust/sort/u64/random/49/s1/t4"] : []),
  ];
  await measureSamples(run, binaries, "unused", mask, async () => {});
  if (mask) await assert.rejects(comparisonCpus(4, "0"), /require 4 available CPUs/);
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

test("historical manifest completion never fabricates parallel support", () => {
  const baseline = "[features]\n[dependencies]\n";
  const current =
    '[features]\nrayon = ["dep:rayon"]\n[dependencies]\nrayon = { version = "1.12", optional = true }\n';
  assert.match(serialBaselineManifest(baseline, current), /rayon/);
  assert.equal(serialBaselineManifest(current, current, true), current);
  assert.throws(() => serialBaselineManifest(baseline, current, true), /Rayon-capable baseline/);
});

test("revision selection checks parallel IDs from both binaries", {
  skip: process.platform === "win32",
}, async () => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-selection-"));
  try {
    const input = { ty: "u64", distribution: "random", len: 48, operation: SORT };
    const serial = criterionId(input, "serial"),
      parallel = criterionId(input, "t4");
    const binaries = { baseline: join(directory, "baseline"), current: join(directory, "current") };
    const plan = join(directory, "plan.json");
    const source = (withParallel: boolean) => `#!${process.execPath}
if (process.env.TESSERA_BENCH_LIST !== "1" || process.env.TESSERA_BENCH_PLAN !== ${JSON.stringify(plan)}) process.exit(1);
const supported = [${JSON.stringify(serial)}];
if (${withParallel} && JSON.parse(require("node:fs").readFileSync(process.env.TESSERA_BENCH_PLAN)).include?.some((rule) => rule.executions?.includes("t4"))) supported.push(${JSON.stringify(parallel)});
console.log(supported.join("\\n"));
`;
    for (const path of Object.values(binaries))
      await writeFile(path, source(true), { mode: 0o755 });
    await writeFile(plan, JSON.stringify({}));
    assert.deepEqual(await selectedCases(binaries, plan, undefined, directory), [serial]);
    await writeFile(plan, JSON.stringify({ include: [{ executions: ["serial", "t4"] }] }));
    assert.deepEqual(await selectedCases(binaries, plan, undefined, directory, ["t4"]), [
      serial,
      parallel,
    ]);
    assert.deepEqual(await selectedCases(binaries, plan, "/t4$", directory, ["t4"]), [parallel]);
    await writeFile(binaries.baseline, source(false));
    await assert.rejects(selectedCases(binaries, plan, undefined, directory, ["t4"]), /identical/);
    await writeFile(binaries.current, source(false));
    await assert.rejects(
      selectedCases(binaries, plan, undefined, directory, ["t4"]),
      /did not list any t4 cases/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("revision scheduling reserves the largest selected worker count", async () => {
  const local = await loadPlan("local");
  assert.equal(revisionThreads(local), 1);
  assert.equal(revisionThreads(await loadPlan("continuous")), 4);
  assert.equal(revisionThreads(await loadPlan("local", { executions: ["serial", "t2"] })), 2);
  assert.equal(
    revisionThreads({
      ...local,
      include: [{ operation: { kind: "argsort" }, executions: ["t4"] }],
    }),
    4,
  );
  // Mixed serial-operation and parallel-sort plans — including plans where parallel
  // argsort is excluded — must not require `parallel-operations`, so comparisons
  // against pre-par_argsort revisions continue to build.
  assert.deepEqual(
    planFeatures({
      ...local,
      include: [{ operation: { kind: "argsort" }, executions: ["serial"] }, { executions: ["t4"] }],
    }),
    ["rayon", "operations"],
  );
  assert.deepEqual(
    planFeatures({
      ...local,
      include: [
        { operation: { kind: "argsort" }, executions: ["serial", "t4"] },
        { executions: ["t4"] },
      ],
      exclude: [{ operation: { kind: "argsort" }, executions: ["t4"] }],
    }),
    ["rayon", "operations"],
  );
  assert.deepEqual(
    planFeatures({
      ...local,
      include: [{ operation: { kind: "argsort" }, executions: ["t4"] }],
    }),
    ["rayon", "operations", "parallel-operations"],
  );
  assert.throws(
    () =>
      revisionThreads({
        ...local,
        include: [{ operation: { kind: "select_nth", index: 0 }, executions: ["t4"] }],
      }),
    /only serial/,
  );
  for (const implementations of [["tessera", "std-stable"], ["std-stable"], ["rayon-stable"]])
    assert.throws(() => revisionThreads({ ...local, implementations }), /only tessera/);
});

test("revision scheduling admits only the Tessera implementation for its language", async () => {
  const rust = await loadPlan("continuous");
  const go = { ...rust, implementations: ["go-tessera"] };
  assert.equal(revisionThreads(go, "go"), 4);
  assert.throws(() => revisionThreads(rust, "go"), /only go-tessera/);
  assert.throws(() => revisionThreads(go, "rust"), /only tessera/);
  assert.throws(
    () => revisionThreads({ ...go, implementations: ["go-tessera", "go-slices"] }, "go"),
    /only go-tessera/,
  );
});

test("revision comparison enables historical SIMD without changing current builds", () => {
  assert.deepEqual(simdFeatures('[features]\nportable-simd = ["dep:fearless_simd"]\n'), [
    "portable-simd",
  ]);
  for (const dependency of [
    '"1.2"',
    '{ version = "1.2" }',
    '{ version = "1.2", optional = false }',
  ]) {
    assert.deepEqual(simdFeatures("[dependencies]\nfearless_simd = " + dependency), []);
  }
  for (const manifest of [
    "[dependencies]\n",
    '[dependencies]\nfearless_simd = { version = "1.2", optional = true }',
  ]) {
    assert.throws(() => simdFeatures(manifest), /no supported SIMD backend/);
  }
});

test("serial lanes take one CPU per physical core", async () => {
  const cores: Record<string, string> = { "0": "0:0", "1": "0:0", "2": "0:1", "3": "0:1" };
  assert.deepEqual(await serialLanes("0,1,2,3", async (cpu) => cores[cpu]), ["0", "2"]);
  assert.deepEqual(await serialLanes("0,1,2,3", async (cpu) => `0:${cpu}`), ["0", "1"]);
  assert.deepEqual(await serialLanes("0,1", async () => "0:0"), ["0"]);
  const unreadable = async () => {
    throw new Error("no topology");
  };
  assert.deepEqual(await serialLanes("4,5,6,7", unreadable), ["4"]);
});

test("Go preparation checks the full inventory, digests and provenance before sampling", {
  skip: process.platform === "win32",
}, async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-go-pair-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const id = criterionId(
    { ty: "u64", distribution: "random", len: 48, operation: SORT },
    "serial",
    "go",
  );
  const requests = {
    [id]: {
      id,
      case: { len: 48 },
      record_bytes: 8,
      batch: 1,
      implementation: "go-tessera",
      threads: 1,
      parallel: false,
      samples: 1,
      warm_ns: 1,
      window_ns: 1,
    },
  };
  const binaries = { baseline: join(directory, "baseline"), current: join(directory, "current") };
  const writeWorker = (path: string, override: object = {}) =>
    writeFile(
      path,
      "#!" +
        process.execPath +
        "\n" +
        "const override = " +
        JSON.stringify(override) +
        ";\n" +
        'for await (const line of require("node:readline").createInterface({input:process.stdin})) {\n' +
        "  const {action,id} = JSON.parse(line);\n" +
        '  const replies = {list:{id,route:"sort"},digest:{id,digest:"0123456789abcdef"},info:{GoVersion:"go1.27.4",Settings:[{Key:"GOEXPERIMENT",Value:"simd"}],Deps:[]}};\n' +
        "  const change = !override.only || id === override.only ? override[action] : {};\n" +
        "  console.log(JSON.stringify({...replies[action],...change}));\n" +
        "}\n",
      { mode: 0o755 },
    );
  await writeWorker(binaries.baseline);
  await writeWorker(binaries.current);
  const select = () =>
    selectedCases(binaries, "unused", undefined, directory, ["serial"], requests);
  assert.deepEqual(await select(), [id]);
  await inspectGoPair(binaries, requests, [id]);
  for (const [name, override, message] of [
    ["missing digest", { digest: { digest: null } }, /Missing Go input digest/],
    ["differing digest", { digest: { digest: "1123456789abcdef" } }, /input digests differ/],
    ["altered provenance", { info: { GoVersion: "go1.27.5" } }, /build settings differ/],
  ] as const)
    await t.test(name, async () => {
      await writeWorker(binaries.current, override);
      await assert.rejects(inspectGoPair(binaries, requests, [id]), message);
    });
  await t.test("missing planned ID", async () => {
    const second = id.replace("/48/", "/49/");
    const complete = { ...requests, [second]: { ...requests[id], id: second, case: { len: 49 } } };
    const decline = { only: second, list: { route: null, decline: "unsupported" } };
    await writeWorker(binaries.baseline, decline);
    await writeWorker(binaries.current, decline);
    await assert.rejects(
      selectedCases(binaries, "unused", undefined, directory, ["serial"], complete),
      /inventory does not cover the prepared plan/,
    );
  });
});

test("Go preparation returns absent-module baseline before building", {
  skip: process.platform === "win32",
}, async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-no-go-baseline-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await writeFile(
    join(directory, "git"),
    '#!/bin/sh\ncase "$1" in rev-parse) echo "$4";; ls-tree) exit 0;; *) exit 2;; esac\n',
    { mode: 0o755 },
  );
  const out = join(directory, "out");
  const processResult = spawnSync(
    process.execPath,
    ["tools/index.ts", "continuous", "prepare", "base", "--language", "go", "--out", out],
    {
      env: { ...process.env, PATH: directory + ":" + process.env.PATH },
    },
  );
  assert.equal(processResult.status, 0, processResult.stderr.toString());
  assert.deepEqual(JSON.parse(processResult.stdout.toString()).result, {
    kind: "no-baseline",
    baseline: "base^{commit}",
    head: "HEAD^{commit}",
  });
  await assert.rejects(readFile(join(out, "prepared.json")), { code: "ENOENT" });
});
