// SPDX-License-Identifier: BSD-2-Clause

import { strict as assert } from "node:assert";
import { mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { comparisonGroup, comparisonId, type Operation } from "../catalog.ts";
import { createFixture } from "./fixture.ts";
import { collectWarnings, writeReport } from "./index.ts";
import { loadRun, validateRun } from "./load.ts";
import { measurementId, summarize } from "./model.ts";

async function writeFixture(directory: string, dataset = createFixture()) {
  await Promise.all([
    writeFile(join(directory, "host.json"), JSON.stringify(dataset.host)),
    ...["manifest.json", "memory-manifest-default.json"].map((file) =>
      writeFile(join(directory, file), JSON.stringify(dataset.manifest)),
    ),
    writeFile(
      join(directory, "blocks.jsonl"),
      [...dataset.observations, ...dataset.samples].map((s) => `${JSON.stringify(s)}\n`).join(""),
    ),
    writeFile(
      join(directory, "memory.jsonl"),
      dataset.memory.map((s) => `${JSON.stringify(s)}\n`).join(""),
    ),
  ]);
  return dataset;
}

test("paired ratios respect noise", () => {
  const dataset = createFixture();
  validateRun(dataset);
  const s = summarize(dataset).get(measurementId("default", "u64/random/1000/s1", "std-stable"))!;
  assert.equal(s.verdict, "slower");
  assert.ok(Math.abs(s.ratio - 2.6) < 0.01);
  const aa = summarize(dataset).get(measurementId("default", s.caseId, "tessera-aa"))!;
  assert.ok(aa.low < 1 && aa.high > 1);
  const rows = dataset.samples.filter((r) => r.case_id === s.caseId && r.variant === "std-stable");
  for (const row of rows) {
    const baseline = dataset.samples.find(
      (r) => r.case_id === s.caseId && r.block === row.block && r.variant === "tessera",
    )!;
    row.elapsed_ns = Math.round(baseline.elapsed_ns * (row.block < 7 ? 0.8 : 1.2));
  }
  assert.equal(
    summarize(dataset).get(measurementId("default", s.caseId, "std-stable"))!.verdict,
    "within noise",
  );
  rows[7].elapsed_ns = Math.round(rows[7].elapsed_ns * 0.5);
  assert.equal(
    summarize(dataset).get(measurementId("default", s.caseId, "std-stable"))!.verdict,
    "faster",
  );
  for (const row of rows)
    row.elapsed_ns = dataset.samples.find(
      (r) => r.case_id === s.caseId && r.block === row.block && r.variant === "tessera",
    )!.elapsed_ns;
  assert.equal(
    summarize(dataset).get(measurementId("default", s.caseId, "std-stable"))!.verdict,
    "within noise",
  );
});

test("revision placements retain their own measured verdict", () => {
  const dataset = createFixture();
  dataset.manifest.variants.push({ name: "tessera-baseline", stable: true, adapter: "revision" });
  for (const row of dataset.samples)
    if (row.variant === "std-stable") row.variant = "tessera-baseline";
  const result = summarize(dataset).get(
    measurementId("default", "u64/random/1000/s1", "tessera-baseline"),
  )!;
  assert.equal(result.verdict, "slower");
  assert.equal(result.stable, true);
});

test("parallel Tessera is an opponent, not an A/A control", () => {
  const dataset = createFixture();
  const id = "u64/random/1000/s1";
  dataset.manifest.variants.push({ name: "tessera-par-8", stable: true, threads: 8 });
  for (const sample of dataset.samples) {
    if (sample.variant !== "std-stable") continue;
    sample.variant = "tessera-par-8";
    const baseline = dataset.samples.find(
      (s) => s.variant === "tessera" && s.case_id === sample.case_id && s.block === sample.block,
    )!;
    sample.elapsed_ns = Math.round(baseline.elapsed_ns / 4);
    sample.batch = baseline.batch;
  }
  const summary = summarize(dataset).get(measurementId("default", id, "tessera-par-8"))!;
  assert.equal(summary.stable, true);
  assert.equal(summary.verdict, "faster");
  dataset.host.affinity = "0,2,4,6,8,10,12,14";
  const warnings = collectWarnings(dataset);
  assert.ok(warnings.some((warning) => warning.includes("requested physical cores")));
  assert.ok(warnings.every((warning) => !warning.includes("recommended one logical CPU")));
});

test("A/A verdicts use the paired mean and survive swapping control labels", () => {
  const dataset = createFixture();
  const id = "u64/random/1000/s1";
  dataset.samples = dataset.samples.filter((s) => s.case_id === id);
  const key = measurementId("default", id, "std-stable");
  for (const [opponent, verdict] of [
    [109, "within noise"],
    [99, "faster"],
    [100, "within noise"],
    [110, "within noise"],
    [120, "within noise"],
    [121, "slower"],
  ] as const) {
    for (const s of dataset.samples) {
      const time = s.variant === "tessera" ? 100 : s.variant === "tessera-aa" ? 120 : opponent;
      s.elapsed_ns = s.batch * time;
    }
    const summaries = summarize(dataset);
    const summary = summaries.get(key)!;
    assert.equal(summary.verdict, verdict, `opponent time ${opponent}`);
    assert.equal(summary.ratio, opponent / 110);
    assert.equal(summary.aaLow, 100 / 110);
    assert.equal(summary.aaHigh, 120 / 110);
    assert.equal(summaries.get(measurementId("default", id, "tessera-aa"))!.ratio, 120 / 100);
    for (const s of dataset.samples) {
      if (s.variant === "tessera") s.variant = "tessera-aa";
      else if (s.variant === "tessera-aa") s.variant = "tessera";
    }
    const swapped = summarize(dataset);
    assert.deepEqual(swapped.get(key), summary);
    assert.equal(swapped.get(measurementId("default", id, "tessera-aa"))!.ratio, 100 / 120);
  }
});

test("invalid samples fail validation", () => {
  for (const mutation of [
    (d: ReturnType<typeof createFixture>) => {
      d.samples.pop();
    },
    (d: ReturnType<typeof createFixture>) => {
      d.partial = true;
      d.samples = d.samples.filter((sample) => sample.case_id !== "u64/random/1000/s1");
    },
    (d: ReturnType<typeof createFixture>) => {
      d.samples.push(d.samples[0]);
    },
    (d: ReturnType<typeof createFixture>) => {
      d.samples[0].verified = false;
    },
    (d: ReturnType<typeof createFixture>) => {
      d.samples[0].slot = d.samples[1].slot;
    },
    (d: ReturnType<typeof createFixture>) => {
      d.samples[0].batch = 0;
    },
    (d: ReturnType<typeof createFixture>) => {
      d.samples[0].case_id = "u64/random/1001/s1";
    },
    (d: ReturnType<typeof createFixture>) => {
      d.memory.pop();
    },
    (d: ReturnType<typeof createFixture>) => {
      d.observations.pop();
    },
  ]) {
    const dataset = createFixture();
    mutation(dataset);
    assert.throws(() => validateRun(dataset));
  }
});

test("short reports retain their declared budget and reject missing or extra blocks", async (t) => {
  const directory = await mkdtemp(join(tmpdir(), "tessera-short-report-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const dataset = createFixture(3);
  await writeFixture(directory, dataset);
  const loaded = await loadRun([directory]);
  assert.equal(loaded.manifest.blocks, 3);
  await writeReport([directory]);
  assert.match(
    await readFile(join(directory, "report/index.md"), "utf8"),
    /3 blocks per case.*short screen/,
  );
  loaded.samples[0].block = 3;
  assert.throws(() => validateRun(loaded), /invalid or unverified/);
  loaded.samples[0].block = 0;
  loaded.samples.pop();
  assert.throws(() => validateRun(loaded), /missing blocks/);
  dataset.manifest.blocks = 9;
  await writeFixture(directory, dataset);
  await assert.rejects(loadRun([directory]), /missing blocks/);
  dataset.manifest.blocks = 2;
  await writeFixture(directory, dataset);
  await assert.rejects(loadRun([directory]), /budget/);
});

test("raw results round trip", async () => {
  const dir = await mkdtemp(join(tmpdir(), "tessera-report-"));
  try {
    await writeFixture(dir);
    const summary = await writeReport([dir]);
    assert.equal(summary.kind, "algorithm-comparison");
    assert.equal(summary.ratio, "algorithm / paired Tessera mean");
    assert.equal(summary.focus.length, 5);
    assert.ok(summary.omittedChanges > 0);
    const bmf = JSON.parse(await readFile(join(dir, "bmf.json"), "utf8"));
    const s = bmf["compare/u64/random/1000/s1/std-stable/default"];
    assert.ok(Math.abs(s.latency.value - 10.816) < 0.01);
    assert.ok(s.ratio.lower_value <= s.ratio.value && s.ratio.upper_value >= s.ratio.value);
    assert.ok(bmf["compare/u64/random/1000/s1/tessera-aa/default"]);
    assert.match(await readFile(join(dir, "report/index.md"), "utf8"), /synthetic fixture/);
    await assert.rejects(loadRun([dir, dir]), /duplicate/);
    await writeFile(join(dir, "manifest-portable-simd.json"), "{}");
    await assert.rejects(loadRun([dir]), /Multiple-build/);
    await rm(join(dir, "manifest-portable-simd.json"));
    const other = await mkdtemp(join(tmpdir(), "tessera-other-host-"));
    try {
      await writeFixture(other);
      await writeFile(join(other, "host.json"), '{"cpu":"different"}');
      await assert.rejects(loadRun([dir, other]), /fingerprints differ/);
    } finally {
      await rm(other, { recursive: true });
    }
  } finally {
    await rm(dir, { recursive: true });
  }
});

test("optimization and debug settings must agree across timing and memory", async () => {
  const dir = await mkdtemp(join(tmpdir(), "tessera-build-config-"));
  try {
    const dataset = await writeFixture(dir);
    await loadRun([dir]);
    for (const field of ["opt_level", "debug", "blocks"]) {
      const different = field === "blocks" ? 3 : field === "opt_level" ? "1" : "true";
      {
        const manifest = dataset.manifest;
        const build = "default";
        const memoryPath = join(dir, `memory-manifest-${build}.json`);
        await writeFile(memoryPath, JSON.stringify({ ...manifest, [field]: different }));
        await assert.rejects(
          loadRun([dir]),
          new RegExp(`memory provenance differs: ${build}/${field}`),
        );
        await writeFile(memoryPath, JSON.stringify(manifest));
      }
    }
  } finally {
    await rm(dir, { recursive: true });
  }
});

/** Small boundary fixture, derived from the existing paired-run fixture. */
function operationFixture(operation: Operation) {
  const dataset = createFixture(3);
  const payloadRanks = !["argsort", "segments", "partition", "partition-buckets"].includes(
    operation.kind,
  );
  const ty = payloadRanks ? "row_u64" : "u64";
  const source = dataset.manifest.cases.find(
    (c) => c.ty === ty && c.distribution === "random" && c.len === 1000,
  )!;
  const oldId = comparisonId(source),
    oldGroup = comparisonGroup(source);
  const variants = payloadRanks
    ? ["tessera", "std-stable", "std-unstable"]
    : ["tessera", "std-stable"];
  const c = {
    ...source,
    operation,
    variants,
    implementations: Object.fromEntries(
      variants.map((name) => [name, "synthetic complete operation"]),
    ),
    omitted: [{ variant: "rdst", reason: "unsupported operation/input combination" }],
  };
  const id = comparisonId(c),
    group = comparisonGroup(c);
  Object.assign(dataset.manifest, {
    suite_version: 2,
    index_bytes: 8,
    cases: [c],
    variants: [
      ...dataset.manifest.variants.filter((v) => variants.includes(v.name)),
      { name: "rdst", stable: false },
    ],
    plan: {
      schema: 2,
      operation,
      algorithms: [...variants, "rdst"],
      include: [{ types: [c.ty], distributions: [c.distribution], lengths: [c.len] }],
    },
  });
  dataset.samples = dataset.samples
    .filter((s) => s.case_id === oldId && [...variants, "tessera-aa"].includes(s.variant))
    .map((s) => ({
      ...s,
      case_id: id,
      group,
      slot: [...variants, "tessera-aa"].indexOf(s.variant),
    }));
  dataset.observations = dataset.observations
    .filter((o) => o.group === oldGroup)
    .map((o) => ({ ...o, group }));
  const output =
    operation.kind === "argsort"
      ? c.len * 8
      : operation.kind === "partition-buckets"
        ? (operation.bucket_count + 1) * 8
        : 0;
  dataset.memory = dataset.memory
    .filter((m) => m.case_id === oldId && variants.includes(m.variant))
    .map((m) => ({
      ...m,
      case_id: id,
      peak_extra_bytes: m.peak_extra_bytes + output,
      output_bytes: output,
    }));
  return dataset;
}

test("operation reports retain exact parameters and output-aware memory without sorting figures", async (t) => {
  for (const operation of [
    { kind: "argsort" },
    { kind: "partition" },
    { kind: "partition-buckets", bucket_count: 7 },
    { kind: "segments", segment_len: 73 },
    { kind: "top-k", k: 99 },
    { kind: "prefix", prefix_len: 99 },
    { kind: "select", index: 499 },
    { kind: "rank-range", start: 99, end: 199 },
    { kind: "select-many", indices: [999, 0, 999] },
  ] satisfies Operation[]) {
    const directory = await mkdtemp(join(tmpdir(), "tessera-operation-report-"));
    t.after(() => rm(directory, { recursive: true, force: true }));
    const dataset = operationFixture(operation);
    validateRun(dataset);
    await writeFixture(directory, dataset);
    await writeReport([directory]);
    const report = await readFile(join(directory, "report/index.md"), "utf8");
    assert.match(report, /synthetic complete operation/);
    if (dataset.manifest.cases[0].ty === "row_u64") {
      assert.ok(report.includes("| row_u64 | std-unstable | synthetic complete operation |"));
      assert.match(report, /partial routes.*payload records/);
      assert.doesNotMatch(report, /selection remains scalar-only/);
    }
    assert.ok(report.includes(JSON.stringify(operation)));
    assert.ok((await readdir(join(directory, "report"))).every((name) => !name.endsWith(".svg")));
    assert.equal(
      (await loadRun([directory])).memory[0].output_bytes,
      operation.kind === "argsort"
        ? 8000
        : operation.kind === "partition-buckets"
          ? (operation.bucket_count + 1) * 8
          : 0,
    );
    const invalid = structuredClone(dataset);
    invalid.memory[0].output_bytes = (invalid.memory[0].output_bytes ?? 0) + 1;
    assert.throws(() => validateRun(invalid), /output allocation/);
    const memoryPath = join(directory, "memory-manifest-default.json");
    await writeFile(memoryPath, JSON.stringify({ ...dataset.manifest, index_bytes: 4 }));
    await assert.rejects(loadRun([directory]), /index_bytes/);
  }
  for (const mutation of [
    (d: ReturnType<typeof operationFixture>) => {
      delete d.manifest.cases[0].operation;
    },
    (d: ReturnType<typeof operationFixture>) => {
      d.manifest.cases[0].operation = { kind: "argsort" };
    },
    (d: ReturnType<typeof operationFixture>) => {
      d.manifest.cases[0].implementations = {};
    },
    (d: ReturnType<typeof operationFixture>) => {
      d.manifest.cases[0].omitted = [];
    },
  ]) {
    const dataset = operationFixture({ kind: "top-k", k: 99 });
    mutation(dataset);
    assert.throws(() => validateRun(dataset));
  }
});
