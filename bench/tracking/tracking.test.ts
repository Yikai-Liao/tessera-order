// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtemp, readdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { parseHTML } from "linkedom/worker";
import { z } from "zod";
import { latestValues, renderCaseHistory, renderMainOverview, renderMainTrend } from "./history.ts";
import { appendRun, trackingCases } from "./publish.ts";
import { indexSchema, parseRuns, summarizeReport, type TrackingRun } from "./result.ts";

const PREVIEW_CASE = "rust/u64/random/65536/s1/api";

const uuid = "01a1cdb1-9445-76dd-89c4-e4220d49e6f7";
const source = "a".repeat(40);
const report = () => ({
  uuid,
  start_time: "2026-10-31T01:00:00Z",
  adapter: "rust_criterion",
  project: { slug: "example" },
  branch: { name: "main", head: { version: { hash: source } } },
  testbed: { name: "baremetal-intel-v1-absolute-v1", spec: { uuid, slug: "intel-v1" } },
  results: [
    [
      {
        benchmark: { name: PREVIEW_CASE },
        measures: [
          {
            measure: { slug: "latency", units: "nanoseconds (ns)" },
            metric: { value: 10, lower_value: 9, upper_value: 11 },
          },
        ],
      },
    ],
  ],
});
const run: TrackingRun = {
  ...summarizeReport(report(), source, "example", [PREVIEW_CASE]),
  protocol: "criterion-main-v1",
  rustc: "rustc test",
  rustflags: "",
  image: "registry.bencher.dev/example@sha256:" + "b".repeat(64),
  workflow: "https://github.com/test/repo/actions/runs/1",
};

test("only complete matching absolute reports enter history", () => {
  assert.deepEqual(run.values[PREVIEW_CASE], [10, 9, 11]);
  const mutations: ((r: ReturnType<typeof report>) => void)[] = [
    (r) => {
      r.branch.head.version.hash = "0".repeat(40);
    },
    (r) => {
      r.project.slug = "other";
    },
    (r) => {
      r.testbed.name = "ci";
    },
    (r) => {
      r.results = [[]];
    },
    (r) => {
      r.results[0].push(r.results[0][0]);
    },
    (r) => {
      r.results[0][0].measures[0].measure.slug = "paired-time-ratio";
    },
    (r) => {
      r.results[0][0].measures[0].metric.lower_value = 12;
    },
  ];
  for (const mutate of mutations) {
    const r = report();
    mutate(r);
    assert.throws(() => summarizeReport(r, source, "example", [PREVIEW_CASE]));
  }
  assert.throws(() => parseRuns(JSON.stringify(run)), /Truncated/);
});

test("publication is idempotent, combines months and keeps the README SVG URL stable", async (t) => {
  const dir = await mkdtemp(join(tmpdir(), "main-history-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  await appendRun(dir, run);
  const first = await readFile(join(dir, "preview.svg"), "utf8");
  assert.match(first, /<svg/);
  const before = await readdir(dir);
  await appendRun(dir, run);
  assert.deepEqual(await readdir(dir), before);
  await assert.rejects(appendRun(dir, { ...run, rustc: "different" }), /different data/);
  const next = {
    ...run,
    id: "01a1cdb1-9445-76dd-89c4-e4220d49e6f8",
    time: "2026-11-01T01:00:00Z",
    values: { [PREVIEW_CASE]: [20, 19, 21] as [number, number, number] },
  };
  await appendRun(dir, next);
  const index = indexSchema.parse(JSON.parse(await readFile(join(dir, "index.json"), "utf8")));
  assert.deepEqual(index.months, ["2026-10", "2026-11"]);
  assert.deepEqual(index.cases, [
    {
      id: PREVIEW_CASE,
      label: "u64 · random · 65,536",
      group: "serial",
      input: "random",
      sub: "u64 · 65,536",
      tag: "",
    },
  ]);
  const all = [];
  for (const month of index.months) {
    const text = await readFile(join(dir, month + ".jsonl"), "utf8");
    all.push(...parseRuns(text));
  }
  assert.deepEqual(
    all.map((r) => r.time),
    [run.time, next.time],
  );
  assert.notEqual(await readFile(join(dir, "preview.svg"), "utf8"), first);
  await appendRun(dir, { ...next, id: "01a1cdb1-9445-76dd-89c4-e4220d49e6f9" });
  assert.equal(parseRuns(await readFile(join(dir, "2026-11.jsonl"), "utf8")).length, 2);
  assert.deepEqual(
    indexSchema.parse(JSON.parse(await readFile(join(dir, "index.json"), "utf8"))),
    index,
  );
  // The first page version reads only `id` and `label`, so the new index stays readable to it.
  const firstReader = z.object({
    cases: z.array(z.object({ id: z.string(), label: z.string() })),
  });
  assert.doesNotThrow(() => firstReader.parse(index));
  // Re-publishing an identical report regenerates derived files without remeasuring.
  await rm(join(dir, "index.json"));
  await appendRun(dir, next);
  assert.deepEqual(
    indexSchema.parse(JSON.parse(await readFile(join(dir, "index.json"), "utf8"))),
    index,
  );
  assert.equal(renderCaseHistory([], index.cases[0]), "");
  assert.equal(renderMainTrend([], index.cases), "");
});

const texts = (svg: string) =>
  [...parseHTML(svg).document.querySelectorAll("text")].map((node) => node.textContent);
const at = (i: number, values: Record<string, number>, spec = uuid): TrackingRun => ({
  ...run,
  id: `01a1cdb1-9445-76dd-89c4-e4220d49e${i.toString(16).padStart(3, "0")}`,
  time: `2026-11-${String(i + 1).padStart(2, "0")}T00:00:00Z`,
  hash: i.toString(16).padStart(40, "0"),
  spec,
  values: Object.fromEntries(Object.entries(values).map(([id, v]) => [id, [v, v, v]])),
});

test("cases follow mode, length, record type and catalog input order", () => {
  const ids = [
    "rust/u64/random/1000000/s1/par-4",
    "rust/u64/sorted/65536/s1/api",
    "rust/u32/random/1000000/s1/api",
    "rust/u64/random/65536/s2/api",
    "rust/u64/random/65536/s1/api",
    "rust/u64/random/32/s1/api",
  ];
  const cases = trackingCases(ids);
  assert.deepEqual(
    cases.map((entry) => entry.id),
    [
      "rust/u64/random/32/s1/api",
      "rust/u64/random/65536/s1/api",
      "rust/u64/random/65536/s2/api",
      "rust/u64/sorted/65536/s1/api",
      "rust/u32/random/1000000/s1/api",
      "rust/u64/random/1000000/s1/par-4",
    ],
  );
  assert.equal(cases[2].tag, "seed 2");
  assert.equal(cases[5].group, "parallel · 4 threads");
  assert.equal(indexSchema.parse({ months: [], cases }).cases.length, ids.length);
});

test("the trend aligns gaps and chain-links aggregates across changing case sets", () => {
  const a = "rust/u64/random/32/s1/api",
    b = "rust/u64/random/65536/s1/api",
    c = "rust/u64/sorted/65536/s1/api";
  const cases = trackingCases([a, b, c]);
  // c enters at the second run, misses the third and is twice as slow as the others.
  const runs = [
    at(0, { [a]: 100, [b]: 100 }),
    at(1, { [a]: 110, [b]: 90, [c]: 400 }),
    at(2, { [a]: 121, [b]: 81 }),
    at(3, { [a]: 121, [b]: 81, [c]: 200 }),
  ];
  const text = texts(renderMainTrend(runs, cases, "t", (id) => "#" + id));
  // Aggregate: sqrt(1.1·0.9) twice, then c returns across its gap: ·cbrt(1·1·0.5).
  const overall = 1.1 * 0.9 * Math.cbrt(0.5) - 1;
  assert.equal(text[text.indexOf("Overall") + 2], `−${(-overall * 100).toFixed(1)}%`);
  assert.ok(text.includes("+21.0%") && text.includes("−19.0%") && text.includes("−50.0%"));
  const svg = parseHTML(renderMainTrend(runs, cases, "t", (id) => "#" + id)).document;
  assert.deepEqual(
    [...svg.querySelectorAll(".trend-row-link a")].map((node) => node.getAttribute("href")),
    [a, b, c].map((id) => "#" + id),
  );
  // Without links, the overview keeps only the mode aggregate.
  const overview = texts(renderMainOverview(runs, cases));
  assert.ok(overview.includes("Serial") && !overview.includes("random"));
  assert.deepEqual(
    latestValues(runs, cases).map((row) => [
      row.entry.id,
      row.value,
      Number(row.change.toFixed(3)),
    ]),
    [
      [a, 121, 0.21],
      [b, 81, -0.19],
      [c, 200, -0.5],
    ],
  );
});

test("only runs on the latest protocol and specification form the trend", () => {
  const cases = trackingCases([PREVIEW_CASE]);
  const other = "01a1cdb1-9445-76dd-89c4-e4220d49e000";
  const runs = [
    at(0, { [PREVIEW_CASE]: 50 }, other),
    at(1, { [PREVIEW_CASE]: 100 }),
    at(2, { [PREVIEW_CASE]: 110 }),
  ];
  assert.ok(texts(renderMainTrend(runs, cases)).includes("+10.0%"));
  // The detail chart keeps every run, with other hardware as a separate line.
  const detail = parseHTML(renderCaseHistory(runs, cases[0])).document;
  assert.equal(detail.querySelectorAll("a[href*='/commit/']").length, 3);
  // Its heading compares within the latest series, as the trend does.
  assert.ok(
    texts(renderCaseHistory(runs, cases[0])).some((text) => text?.startsWith("+10.0% since")),
  );
  assert.deepEqual(
    latestValues(runs, cases).map((row) => row.current),
    [true],
  );
});
