// SPDX-License-Identifier: BSD-2-Clause

import { strict as assert } from "node:assert";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { renderFigures } from "./charts.ts";
import { createFixture } from "./fixture.ts";
import { collectWarnings, writeReport } from "./index.ts";
import { loadRun, validateRun } from "./load.ts";
import { measurementId, summarize } from "./model.ts";

async function writeFixture(directory: string, simd = false) {
  const dataset = createFixture(simd);
  await Promise.all([
    writeFile(join(directory, "host.json"), JSON.stringify(dataset.host)),
    ...[...dataset.manifests].flatMap(([build, manifest]) =>
      [
        build === "default" ? "manifest.json" : `manifest-${build}.json`,
        `memory-manifest-${build}.json`,
      ].map((file) => writeFile(join(directory, file), JSON.stringify(manifest))),
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

test("figures preserve plotted values", () => {
  const dataset = createFixture(),
    charts = renderFigures(dataset, summarize(dataset));
  assert.equal(charts.length, 6);
  for (const chart of charts) {
    assert.match(chart.svg, /width="\d+"/);
    assert.match(chart.svg, /<title id="t">/);
    assert.doesNotMatch(chart.svg, /NaN|Infinity|undefined/);
    assert.match(chart.table, /\|/);
  }
  assert.deepEqual(
    charts.map((c) => c.name),
    ["ranking", "inputs", "radix", "short", "types", "memory"],
  );
  assert.ok(charts[0].table.indexOf("vqsort") < charts[0].table.indexOf("Tessera"));
  assert.match(charts[0].svg, /long-stable-variant-name/);
  assert.match(charts[2].table, /Tessera slower/);
});

test("raw results round trip", async () => {
  const dir = await mkdtemp(join(tmpdir(), "tessera-report-"));
  try {
    await writeFixture(dir);
    await writeReport([dir]);
    const bmf = JSON.parse(await readFile(join(dir, "bmf.json"), "utf8"));
    const s = bmf["compare/u64/random/1000/s1/std-stable/default"];
    assert.ok(Math.abs(s.latency.value - 10.816) < 0.01);
    assert.ok(s.ratio.lower_value <= s.ratio.value && s.ratio.upper_value >= s.ratio.value);
    assert.ok(bmf["compare/u64/random/1000/s1/tessera-aa/default"]);
    assert.match(await readFile(join(dir, "report/index.md"), "utf8"), /synthetic fixture/);
    await assert.rejects(loadRun([dir, dir]), /duplicate/);
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

test("optimization and debug settings must agree across timing, memory and SIMD", async () => {
  const dir = await mkdtemp(join(tmpdir(), "tessera-build-config-"));
  try {
    const dataset = await writeFixture(dir, true);
    await loadRun([dir]);
    for (const field of ["opt_level", "debug"]) {
      const different = field === "opt_level" ? "1" : "true";
      for (const [build, manifest] of dataset.manifests) {
        const memoryPath = join(dir, `memory-manifest-${build}.json`);
        await writeFile(memoryPath, JSON.stringify({ ...manifest, [field]: different }));
        await assert.rejects(
          loadRun([dir]),
          new RegExp(`memory provenance differs: ${build}/${field}`),
        );
        await writeFile(memoryPath, JSON.stringify(manifest));
      }
      const simd = dataset.manifests.get("portable-simd")!;
      const simdPaths = ["manifest-portable-simd.json", "memory-manifest-portable-simd.json"];
      for (const path of simdPaths)
        await writeFile(join(dir, path), JSON.stringify({ ...simd, [field]: different }));
      await assert.rejects(loadRun([dir]), new RegExp(`compiler configuration: ${field}`));
      for (const path of simdPaths) await writeFile(join(dir, path), JSON.stringify(simd));

      const incomplete: Record<string, unknown> = { ...dataset.manifests.get("default")! };
      delete incomplete[field];
      for (const path of ["manifest.json", "memory-manifest-default.json"])
        await writeFile(join(dir, path), JSON.stringify(incomplete));
      await assert.rejects(loadRun([dir]), /expected a nonempty string/);
      for (const path of ["manifest.json", "memory-manifest-default.json"])
        await writeFile(join(dir, path), JSON.stringify(dataset.manifests.get("default")));
    }
  } finally {
    await rm(dir, { recursive: true });
  }
});

test("native compiler versions match", async () => {
  const dir = await mkdtemp(join(tmpdir(), "tessera-native-compilers-"));
  try {
    const dataset = await writeFixture(dir, true);
    const native = {
      ...dataset.manifests.get("default")!,
      cc: "gcc 12",
      cxx: "g++ 12",
      native_build: { cc: "cc", cxx: "c++" },
    };
    const timingPath = join(dir, "manifest.json");
    const memoryPath = join(dir, "memory-manifest-default.json");
    await writeFile(timingPath, JSON.stringify(native));
    await writeFile(memoryPath, JSON.stringify(native));
    await loadRun([dir]);
    for (const [field, version] of [
      ["cc", "gcc 14"],
      ["cxx", "g++ 14"],
    ]) {
      await writeFile(memoryPath, JSON.stringify({ ...native, [field]: version }));
      await assert.rejects(
        loadRun([dir]),
        new RegExp(`memory provenance differs: default/${field}`),
      );
      const incomplete: Record<string, unknown> = { ...native };
      delete incomplete[field];
      await writeFile(timingPath, JSON.stringify(incomplete));
      await writeFile(memoryPath, JSON.stringify(incomplete));
      await assert.rejects(loadRun([dir]), /expected a nonempty string/);
      await writeFile(timingPath, JSON.stringify(native));
      await writeFile(memoryPath, JSON.stringify(native));
    }
  } finally {
    await rm(dir, { recursive: true });
  }
});

test("SIMD shares the anchor", () => {
  const dataset = createFixture(true);
  validateRun(dataset);
  const charts = renderFigures(dataset, summarize(dataset));
  assert.match(charts[0].table, /Tessera, Portable SIMD/);
  const values = summarize(dataset);
  const id = "u64/random/1000/s1";
  const defaultStd = values.get(measurementId("default", id, "std-stable"))!.ratio;
  const simdStd = values.get(measurementId("portable-simd", id, "std-stable"))!.ratio;
  assert.ok(Math.abs(simdStd / defaultStd - 1.25) < 0.001);
});

test("SVG fixtures stay synchronized", async () => {
  const dataset = createFixture(true);
  for (const chart of renderFigures(dataset, summarize(dataset))) {
    assert.equal(
      chart.svg,
      await readFile(new URL(`./fixtures/${chart.name}.svg`, import.meta.url), "utf8"),
    );
  }
});

test("partial builds omit anchors", () => {
  const dataset = createFixture(true);
  dataset.partial = true;
  const selected = (build: string, group: string) =>
    group === (build === "default" ? "u64/random" : "u32/random");
  dataset.samples = dataset.samples.filter((sample) => selected(sample.build, sample.group));
  dataset.observations = dataset.observations.filter((observation) =>
    selected(observation.build, observation.group),
  );
  validateRun(dataset);
  assert.doesNotThrow(() => collectWarnings(dataset));
});
