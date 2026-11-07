// SPDX-License-Identifier: BSD-2-Clause

import { strict as assert } from "node:assert";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { figures } from "./charts.ts";
import { fixture } from "./fixture.ts";
import { load, validate } from "./load.ts";
import { key, summarize } from "./model.ts";
import { report } from "./index.ts";

async function writeFixture(directory: string) {
  const data = fixture();
  await Promise.all([
    writeFile(join(directory, "host.json"), JSON.stringify(data.host)),
    writeFile(join(directory, "manifest.json"), JSON.stringify(data.manifests.get("default"))),
    writeFile(join(directory, "memory-manifest-default.json"), JSON.stringify(data.manifests.get("default"))),
    writeFile(join(directory, "blocks.jsonl"), [...data.observations, ...data.samples].map(s => JSON.stringify(s) + "\n").join("")),
    writeFile(join(directory, "memory.jsonl"), data.memory.map(s => JSON.stringify(s) + "\n").join("")),
  ]);
  return data;
}

test("paired ratios cancel block drift and require eight agreeing blocks beyond A/A noise", () => {
  const data = fixture(); validate(data);
  const s = summarize(data).get(key("default", "u64/random/1000/s1", "std-stable"))!;
  assert.equal(s.verdict, "slower"); assert.ok(Math.abs(s.ratio - 2.6) < 0.01);
  const aa = summarize(data).get(key("default", s.caseId, "tessera-aa"))!;
  assert.ok(aa.low < 1 && aa.high > 1);
  const rows = data.samples.filter(r => r.case_id === s.caseId && r.variant === "std-stable");
  for (const row of rows) {
    const baseline = data.samples.find(r => r.case_id === s.caseId && r.block === row.block && r.variant === "tessera")!;
    row.elapsed_ns = Math.round(baseline.elapsed_ns * (row.block < 7 ? 0.8 : 1.2));
  }
  assert.equal(summarize(data).get(key("default", s.caseId, "std-stable"))!.verdict, "within noise");
  rows[7].elapsed_ns = Math.round(rows[7].elapsed_ns * 0.5);
  assert.equal(summarize(data).get(key("default", s.caseId, "std-stable"))!.verdict, "faster");
  for (const row of rows) row.elapsed_ns = data.samples.find(r => r.case_id === s.caseId && r.block === row.block && r.variant === "tessera")!.elapsed_ns;
  assert.equal(summarize(data).get(key("default", s.caseId, "std-stable"))!.verdict, "within noise");
});

test("incomplete, duplicate, unverified and mismatched samples cannot produce a report", () => {
  for (const mutation of [
    (d: ReturnType<typeof fixture>) => { d.samples.pop(); },
    (d: ReturnType<typeof fixture>) => { d.samples.push(d.samples[0]); },
    (d: ReturnType<typeof fixture>) => { d.samples[0].verified = false; },
    (d: ReturnType<typeof fixture>) => { d.samples[0].slot = d.samples[1].slot; },
    (d: ReturnType<typeof fixture>) => { d.samples[0].batch = 0; },
    (d: ReturnType<typeof fixture>) => { d.samples[0].case_id = "u64/random/1001/s1"; },
    (d: ReturnType<typeof fixture>) => { d.memory.pop(); },
    (d: ReturnType<typeof fixture>) => { d.observations.pop(); },
  ]) { const data = fixture(); mutation(data); assert.throws(() => validate(data)); }
});

test("five figures retain plotted values when Tessera loses and competitors are close", () => {
  const data = fixture(), charts = figures(data, summarize(data));
  assert.equal(charts.length, 5);
  for (const chart of charts) {
    assert.match(chart.svg, /width="720"/); assert.match(chart.svg, /<title>/);
    assert.doesNotMatch(chart.svg, /NaN|Infinity|undefined/); assert.match(chart.table, /\|/);
  }
  assert.ok(charts[0].table.indexOf("vqsort") < charts[0].table.indexOf("tessera"));
  assert.match(charts[0].svg, /long-stable-variant-name/);
  assert.match(charts[2].table, /Tessera slower/);
});

test("raw directory round trip preserves time units, bounds and optional BMF output", async () => {
  const dir = await mkdtemp(join(tmpdir(), "tessera-report-"));
  try {
    await writeFixture(dir); await report([dir]);
    const bmf = JSON.parse(await readFile(join(dir, "bmf.json"), "utf8"));
    const s = bmf["compare/u64/random/1000/s1/std-stable/default"];
    assert.ok(Math.abs(s.latency.value - 10.816) < 0.01);
    assert.ok(s.ratio.lower_value <= s.ratio.value && s.ratio.upper_value >= s.ratio.value);
    assert.ok(bmf["compare/u64/random/1000/s1/tessera-aa/default"]);
    assert.match(await readFile(join(dir, "report/index.md"), "utf8"), /synthetic fixture/);
    await assert.rejects(load([dir, dir]), /duplicate/);
    const other = await mkdtemp(join(tmpdir(), "tessera-other-host-"));
    try { await writeFixture(other); await writeFile(join(other, "host.json"), '{"cpu":"different"}'); await assert.rejects(load([dir, other]), /fingerprints differ/); }
    finally { await rm(other, { recursive: true }); }
  } finally { await rm(dir, { recursive: true }); }
});


test("Portable SIMD has its own A/A pair and uses the unchanged slice::sort anchor", () => {
  const data = fixture(true); validate(data);
  const charts = figures(data,summarize(data));
  assert.match(charts[0].table,/tessera \+ simd/);
  const values = summarize(data);
  const id = "u64/random/1000/s1";
  const defaultStd=values.get(key("default",id,"std-stable"))!.ratio;
  const simdStd=values.get(key("portable-simd",id,"std-stable"))!.ratio;
  assert.ok(Math.abs(simdStd/defaultStd-1.25)<0.001);
});


test("reviewed SVG fixtures stay in sync with the synthetic comparison", async () => {
  const data=fixture(true);
  for(const chart of figures(data,summarize(data))) {
    assert.equal(chart.svg,await readFile(new URL(`./fixtures/${chart.name}.svg`,import.meta.url),"utf8"));
  }
});
