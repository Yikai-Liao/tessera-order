// SPDX-License-Identifier: BSD-2-Clause

import { strict as assert } from "node:assert";
import { mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { parseHTML } from "linkedom/worker";
import { caseGroup, caseId, columnId, type Operation, referenceColumn } from "../catalog.ts";
import type { CurveChart } from "../plot/curves.ts";
import { renderFigures, renderGrowth } from "./charts.ts";
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
  const s = summarize(dataset).get(
    measurementId("default", "sort/u64/random/1000/s1", "std-stable/serial"),
  )!;
  assert.equal(s.verdict, "slower");
  assert.ok(Math.abs(s.ratio - 2.6) < 0.01);
  const aa = summarize(dataset).get(measurementId("default", s.caseId, "tessera-aa"))!;
  assert.ok(aa.low < 1 && aa.high > 1);
  const rows = dataset.samples.filter(
    (r) => r.case_id === s.caseId && r.column === "std-stable/serial",
  );
  for (const row of rows) {
    const baseline = dataset.samples.find(
      (r) => r.case_id === s.caseId && r.block === row.block && r.column === "tessera/serial",
    )!;
    row.elapsed_ns = Math.round(baseline.elapsed_ns * (row.block < 7 ? 0.8 : 1.2));
  }
  assert.equal(
    summarize(dataset).get(measurementId("default", s.caseId, "std-stable/serial"))!.verdict,
    "within noise",
  );
  rows[7].elapsed_ns = Math.round(rows[7].elapsed_ns * 0.5);
  assert.equal(
    summarize(dataset).get(measurementId("default", s.caseId, "std-stable/serial"))!.verdict,
    "faster",
  );
  for (const row of rows)
    row.elapsed_ns = dataset.samples.find(
      (r) => r.case_id === s.caseId && r.block === row.block && r.column === "tessera/serial",
    )!.elapsed_ns;
  assert.equal(
    summarize(dataset).get(measurementId("default", s.caseId, "std-stable/serial"))!.verdict,
    "within noise",
  );
});

test("a partial release selection shows each case rather than partial release panels", () => {
  const full = createFixture();
  assert.ok(renderFigures(full, summarize(full)).some((figure) => figure.name === "lengths"));
  // A shared f64 case and u64 random lengths would fill a pattern panel and the length figure.
  const kept = [
    "sort/f64/random/1000000/s1",
    "sort/u64/random/1000/s1",
    "sort/u64/random/1000000/s1",
  ];
  const dataset = createFixture();
  dataset.manifest.cases = dataset.manifest.cases.filter((input) => kept.includes(caseId(input)));
  dataset.samples = dataset.samples.filter((sample) => kept.includes(sample.case_id));
  dataset.memory = dataset.memory.filter((sample) => kept.includes(sample.case_id));
  validateRun(dataset);
  const figures = renderFigures(dataset, summarize(dataset));
  assert.equal(figures.length, kept.length);
  assert.ok(figures.every((figure) => figure.table.includes("A/A range")));
});

test("revision placements retain their own measured verdict", () => {
  const dataset = createFixture();
  dataset.manifest.columns.push({
    id: "tessera-baseline/serial",
    implementation: "tessera-baseline",
    execution: "serial",
    stable: true,
    adapter: "revision",
  });
  for (const row of dataset.samples)
    if (row.column === "std-stable/serial") row.column = "tessera-baseline/serial";
  const result = summarize(dataset).get(
    measurementId("default", "sort/u64/random/1000/s1", "tessera-baseline/serial"),
  )!;
  assert.equal(result.verdict, "slower");
  assert.equal(result.stable, true);
});

test("parallel Tessera is an opponent, not an A/A control", () => {
  const dataset = createFixture();
  const id = "sort/u64/random/1000/s1";
  dataset.manifest.columns.push({
    id: "tessera/t8",
    implementation: "tessera",
    execution: "t8",
    stable: true,
    adapter: "common",
  });
  for (const sample of dataset.samples) {
    if (sample.column !== "std-stable/serial") continue;
    sample.column = "tessera/t8";
    const baseline = dataset.samples.find(
      (s) =>
        s.column === "tessera/serial" && s.case_id === sample.case_id && s.block === sample.block,
    )!;
    sample.elapsed_ns = Math.round(baseline.elapsed_ns / 4);
    sample.batch = baseline.batch;
  }
  const summary = summarize(dataset).get(measurementId("default", id, "tessera/t8"))!;
  assert.equal(summary.stable, true);
  assert.equal(summary.verdict, "faster");
  dataset.host.affinity = "0,2,4,6,8,10,12,14";
  const warnings = collectWarnings(dataset);
  assert.ok(warnings.some((warning) => warning.includes("requested physical cores")));
  assert.ok(warnings.every((warning) => !warning.includes("recommended one logical CPU")));
});

test("A/A verdicts use the paired mean and survive swapping control labels", () => {
  const dataset = createFixture();
  const id = "sort/u64/random/1000/s1";
  dataset.samples = dataset.samples.filter((s) => s.case_id === id);
  const key = measurementId("default", id, "std-stable/serial");
  for (const [opponent, verdict] of [
    [109, "within noise"],
    [99, "faster"],
    [100, "within noise"],
    [110, "within noise"],
    [120, "within noise"],
    [121, "slower"],
  ] as const) {
    for (const s of dataset.samples) {
      const time = s.column === "tessera/serial" ? 100 : s.column === "tessera-aa" ? 120 : opponent;
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
      if (s.column === "tessera/serial") s.column = "tessera-aa";
      else if (s.column === "tessera-aa") s.column = "tessera/serial";
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
      d.samples = d.samples.filter((sample) => sample.case_id !== "sort/u64/random/1000/s1");
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
      d.samples[0].case_id = "sort/u64/random/1001/s1";
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
    assert.equal(summary.kind, "implementation-comparison");
    assert.equal(summary.ratio, "implementation / paired Tessera mean");
    assert.equal(summary.focus.length, 5);
    assert.ok(summary.omittedChanges > 0);
    const bmf = JSON.parse(await readFile(join(dir, "bmf.json"), "utf8"));
    const s = bmf["compare/sort/u64/random/1000/s1/std-stable/serial/default"];
    assert.ok(Math.abs(s.latency.value - 10.816) < 0.01);
    assert.ok(s.ratio.lower_value <= s.ratio.value && s.ratio.upper_value >= s.ratio.value);
    assert.ok(bmf["compare/sort/u64/random/1000/s1/tessera-aa/default"]);
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
  const payloadRanks = !["argsort", "partition-buckets"].includes(operation.kind);
  const ty = payloadRanks ? "row_u64" : "u64";
  const source = dataset.manifest.cases.find(
    (c) => c.ty === ty && c.distribution === "random" && c.len === 1000,
  )!;
  const oldId = caseId(source),
    oldGroup = caseGroup(source);
  const columns = payloadRanks
    ? ["tessera/serial", "std-stable/serial", "std-unstable/serial"]
    : ["tessera/serial", "std-stable/serial"];
  const implementations = [...columns.map((id) => id.split("/")[0]), "rdst"];
  const c = {
    ...source,
    operation,
    columns,
    routes: Object.fromEntries(columns.map((id) => [id, "synthetic complete operation"])),
    omitted: [
      { column: "rdst/serial", reason: "unsupported operation/input/execution combination" },
    ],
  };
  const id = caseId(c),
    group = caseGroup(c);
  Object.assign(dataset.manifest, {
    index_bytes: 8,
    cases: [c],
    columns: dataset.manifest.columns.filter((column) => columns.includes(column.id)),
    plan: {
      schema: 3,
      implementations,
      include: [{ types: [c.ty], distributions: [c.distribution], lengths: [c.len], operation }],
      exclude: [],
    },
  });
  dataset.samples = dataset.samples
    .filter((s) => s.case_id === oldId && [...columns, "tessera-aa"].includes(s.column))
    .map((s) => ({
      ...s,
      case_id: id,
      group,
      slot: [...columns, "tessera-aa"].indexOf(s.column),
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
    .filter((m) => m.case_id === oldId && columns.includes(m.column))
    .map((m) => ({
      ...m,
      case_id: id,
      peak_extra_bytes: m.peak_extra_bytes + output,
      output_bytes: output,
    }));
  return dataset;
}

test("operation runs regenerate with exact IDs and output-aware memory", async (t) => {
  for (const operation of [
    { kind: "argsort" },
    { kind: "partition-buckets", bucket_count: 7 },
    { kind: "select_nth", index: 499 },
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
      assert.ok(
        report.includes("| std-unstable/serial | synthetic complete operation | row_u64 |"),
      );
      assert.match(report, /partial routes.*payload records/);
    }
    assert.ok(report.includes(JSON.stringify(operation)));
    const reference = columnId(referenceColumn(operation, "serial")!);
    assert.ok(report.includes(`The reference is \`${reference}\``));
    // Speeds divide by the reference's time, not Tessera's.
    const summaries = summarize(dataset);
    const id = caseId(dataset.manifest.cases[0]);
    const ns = (column: string) => summaries.get(measurementId("default", id, column))!.ns;
    assert.notEqual(ns(reference), ns("tessera/serial"));
    assert.ok(report.includes(`${(ns(reference) / ns("tessera/serial")).toFixed(3)}×`));
    assert.ok((await readdir(join(directory, "report"))).every((name) => !name.endsWith(".svg")));
    const regenerated = await loadRun([directory]);
    assert.equal(regenerated.samples[0].case_id, caseId(dataset.manifest.cases[0]));
    assert.equal(
      regenerated.memory[0].output_bytes,
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
      (d.manifest.cases[0] as { operation?: Operation }).operation = undefined;
    },
    (d: ReturnType<typeof operationFixture>) => {
      d.manifest.cases[0].operation = { kind: "argsort" };
    },
    (d: ReturnType<typeof operationFixture>) => {
      d.manifest.cases[0].routes = {};
    },
    (d: ReturnType<typeof operationFixture>) => {
      // A planned execution must account for every implementation.
      d.manifest.cases[0].omitted.push({ column: "tessera/t4", reason: "unsupported" });
    },
  ]) {
    const dataset = operationFixture({ kind: "select_nth", index: 99 });
    validateRun(structuredClone(dataset));
    mutation(dataset);
    assert.throws(() => validateRun(dataset));
  }
  // Selection's reference is the stable full sort; select_nth_unstable is an optional competitor.
  const omit = (column: string) => {
    const d = operationFixture({ kind: "select_nth", index: 99 });
    const c = d.manifest.cases[0];
    c.columns = c.columns.filter((id) => id !== column);
    delete c.routes[column];
    c.omitted.push({ column, reason: "unsupported" });
    const slots = [...c.columns, "tessera-aa"];
    d.samples = d.samples
      .filter((s) => s.column !== column)
      .map((s) => ({ ...s, slot: slots.indexOf(s.column) }));
    d.memory = d.memory.filter((m) => m.column !== column);
    return d;
  };
  validateRun(omit("std-unstable/serial"));
  assert.throws(() => validateRun(omit("std-stable/serial")), /invalid case columns/);
});

test("every case retains each measured column's route", () => {
  const dataset = createFixture(3);
  validateRun(structuredClone(dataset));
  delete (dataset.manifest.cases[0] as { routes?: unknown }).routes;
  assert.throws(() => validateRun(dataset), /routes|object/);
});

test("measurements from an earlier schema must be regenerated by their own revision", () => {
  for (const mutation of [
    (d: ReturnType<typeof createFixture>) => {
      Object.assign(d.manifest, { schema: 1, plan_version: 1 });
    },
    (d: ReturnType<typeof createFixture>) => {
      Object.assign(d.manifest, { plan: { ...d.manifest.plan, schema: 2 } });
    },
  ]) {
    const dataset = createFixture(3);
    mutation(dataset);
    assert.throws(() => validateRun(dataset), /regenerate/);
  }
});

test("the memory figure names each line's growth and keys the slopes in a clear corner", () => {
  const lengths = [1, 2, 3, 4, 5, 6].map((k) => 1024 * 4 ** k);
  const figure = (constant: number) =>
    renderGrowth({
      operation: { kind: "sort" },
      ty: "row_u32",
      distribution: "random",
      lengths,
      peaks: [
        { name: "tessera", values: lengths.map((n) => 64 * Math.sqrt(n)) },
        { name: "std-stable", values: lengths.map((n) => 8 * n) },
        { name: "ips4o", values: lengths.map(() => constant) },
        { name: "std-unstable", values: lengths.map((n) => (n === lengths.at(-1) ? 1000 : null)) },
      ],
    })!;
  const clear = figure(5e5);
  const chart = clear.chart as CurveChart;
  assert.equal(clear.title.startsWith("Grows as √n"), true);
  assert.match(clear.summary, /the key in the corner draws those slopes/);
  assert.deepEqual(
    chart.curves.map((curve) => curve.growth),
    ["∝ √n", "∝ n", "constant", undefined],
  );
  const { document } = parseHTML(clear.svg);
  const image = document.querySelector('svg[role="img"]')!;
  const description = document.getElementById(image.getAttribute("aria-describedby")!)!;
  assert.equal(description.localName, "desc");
  assert.match(description.textContent!, /slice::sort \(∝ n\):/);
  assert.match(description.textContent!, /ips4o \(constant\):/);
  assert.match(description.textContent!, /slice::sort_unstable: 1000/);
  assert.deepEqual(
    chart.references.map((reference) => reference.label),
    ["constant", "∝ √n", "∝ n"],
  );
  // Every key line lies inside the plot.
  for (const [index, reference] of chart.references.entries()) {
    for (const [n, y] of reference.points)
      assert.ok(n >= chart.x.low && n <= chart.x.high && y >= chart.y.low && y <= chart.y.high);
    const [[x1, y1], [x2, y2]] = reference.points;
    const xRatio = x2 / x1;
    const expected = [1, Math.sqrt(xRatio), xRatio][index];
    assert.ok(Math.abs(y2 / y1 / expected - 1) < 1e-12, reference.label);
  }
  // A line close below the key's corner raises the axis instead, keeping the key above it.
  const raised = figure(5e6).chart as CurveChart;
  assert.ok(raised.y.high > chart.y.high);
  const [constant] = raised.references;
  assert.ok(constant.points[0][1] >= 4 * 5e6);
});

test("the memory figure omits the slope key when adjacent lengths crowd its labels", () => {
  const lengths = [65_536, 65_537];
  const figure = renderGrowth({
    operation: { kind: "sort" },
    ty: "row_u32",
    distribution: "random",
    lengths,
    peaks: [{ name: "tessera", values: [8192, 8192] }],
  })!;
  const chart = figure.chart as CurveChart;
  assert.deepEqual(
    chart.curves[0].points.map(([n]) => n),
    lengths,
  );
  assert.deepEqual(chart.references, []);
  assert.doesNotMatch(figure.summary, /\bkey\b/);
});

test("the memory key clears segments spanning unmeasured lengths", () => {
  const lengths = [1024, 4096, 16_384, 65_536];
  for (const firstPeak of [0, 8192, 65_536]) {
    const lastPeak = 524_288;
    const figure = renderGrowth({
      operation: { kind: "sort" },
      ty: "row_u32",
      distribution: "random",
      lengths,
      peaks: [
        { name: "tessera", values: lengths.map(() => 1000) },
        { name: "std-stable", values: [firstPeak, null, null, lastPeak] },
      ],
    })!;
    const chart = figure.chart as CurveChart;
    assert.deepEqual(chart.curves[1].points, [
      [lengths[0], firstPeak],
      [lengths[3], lastPeak],
    ]);
    const [constant] = chart.references;
    assert.equal(constant?.label, "constant");
    const [[, keyBase], [keyEnd]] = constant.points;
    const progress = Math.log(keyEnd / lengths[0]) / Math.log(lengths[3] / lengths[0]);
    const start = Math.max(firstPeak, chart.y.low);
    const segmentPeak = start * (lastPeak / start) ** progress;
    assert.ok(keyBase / segmentPeak >= 4 - 1e-12, `clearance for ${firstPeak} B starting peak`);
    assert.match(figure.table, /\| slice::sort .*?\| [^|]+ \| – \| – \| [^|]+ \|/);
    if (firstPeak === 0) assert.match(figure.table, /\| slice::sort .*?\| 0 B \| – \| – \|/);
  }
});
