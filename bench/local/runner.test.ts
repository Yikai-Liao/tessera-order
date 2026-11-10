// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { availableParallelism, tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { criterionId } from "../catalog.ts";
import {
  backendFeatures,
  parallelExecution,
  selectedCases,
  serialBaselineManifest,
  smokeEnvironment,
} from "./runner.ts";

test("parallel affinity is normalized and too few allowed CPUs fail", {
  skip: process.platform !== "linux",
}, async () => {
  const allowed = (await readFile("/proc/self/status", "utf8"))
    .match(/^Cpus_allowed_list:(.+)$/m)![1]
    .trim();
  await assert.rejects(parallelExecution(allowed.split(/[,-]/)[0]), /Four allowed CPUs/);
  if (availableParallelism() >= 4) {
    const observed = await parallelExecution();
    assert.equal(observed.affinity, allowed);
    assert.equal(observed.allowed, availableParallelism());
    assert.equal(observed.threads, 4);
    assert.match(observed.topology, /CPU,Core,Socket,Node/);
  } else await assert.rejects(parallelExecution(), /Four allowed CPUs/);
});

test("parallel selection enables only the owned smoke protocol", () => {
  assert.deepEqual(smokeEnvironment("plan.json", false), { TESSERA_BENCH_PLAN: "plan.json" });
  assert.deepEqual(smokeEnvironment("plan.json", true), {
    TESSERA_BENCH_PLAN: "plan.json",
    TESSERA_BENCH_PARALLEL: "4",
  });
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
    const input = { ty: "u64", distribution: "random", len: 48 };
    const serial = criterionId(input, "api"),
      parallel = criterionId(input, "par-4");
    const binaries = { baseline: join(directory, "baseline"), current: join(directory, "current") };
    const plan = join(directory, "plan.json");
    const source = (withParallel: boolean) => `#!${process.execPath}
if (process.env.TESSERA_BENCH_LIST !== "1" || process.env.TESSERA_BENCH_PLAN !== ${JSON.stringify(plan)}) process.exit(1);
console.log(${JSON.stringify(serial)});
if (${withParallel} && process.env.TESSERA_BENCH_PARALLEL === "4") console.log(${JSON.stringify(parallel)});
`;
    for (const path of Object.values(binaries))
      await writeFile(path, source(true), { mode: 0o755 });
    assert.deepEqual(await selectedCases(binaries, plan, undefined, directory), [serial]);
    assert.deepEqual(await selectedCases(binaries, plan, undefined, directory, true), [
      serial,
      parallel,
    ]);
    assert.deepEqual(await selectedCases(binaries, plan, "/par-4$", directory, true), [parallel]);
    await writeFile(binaries.baseline, source(false));
    await assert.rejects(selectedCases(binaries, plan, undefined, directory, true), /identical/);
    await writeFile(binaries.current, source(false));
    await assert.rejects(
      selectedCases(binaries, plan, undefined, directory, true),
      /four-thread cases/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("revision comparison enables historical SIMD without changing current builds", () => {
  assert.deepEqual(backendFeatures('[features]\nportable-simd = ["dep:fearless_simd"]\n'), [
    "tessera-sort/portable-simd",
  ]);
  for (const dependency of [
    '"1.2"',
    '{ version = "1.2" }',
    '{ version = "1.2", optional = false }',
  ]) {
    assert.deepEqual(backendFeatures("[dependencies]\nfearless_simd = " + dependency), []);
  }
  for (const manifest of [
    "[dependencies]\n",
    '[dependencies]\nfearless_simd = { version = "1.2", optional = true }',
  ]) {
    assert.throws(() => backendFeatures(manifest), /no supported SIMD backend/);
  }
});
