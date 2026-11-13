// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtemp, readdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { parseHTML } from "linkedom/worker";
import { z } from "zod";
import {
  comparableRuns,
  formatTimestamp,
  historyTimestamp,
  latestValues,
  renderComparison,
  renderMainOverview,
  renderMainTrend,
} from "./history.ts";
import { appendRun, publishRun, trackingCases, trackingFacets } from "./publish.ts";
import {
  indexSchema,
  mergeReports,
  parseRuns,
  runSchema,
  summarizeReport,
  type TrackingRun,
} from "./result.ts";

const PREVIEW_CASE = "rust/u64/random/65536/s1/api";

const uuid = "01a1cdb1-9445-76dd-89c4-e4220d49e6f7";
const source = "a".repeat(40);
const report = () => ({
  uuid,
  start_time: "2026-10-31T01:00:00Z",
  adapter: "rust_criterion",
  project: { slug: "example" },
  branch: { name: "main", head: { version: { hash: source } } },
  testbed: { name: "main", spec: { uuid, slug: "intel-v1" } },
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
  algorithm: "tessera",
  ...summarizeReport(report(), source, "example", [PREVIEW_CASE]),
  protocol: "criterion-main-v1",
  rustc: "rustc test",
  rustflags: "",
  image: "registry.bencher.dev/example@sha256:" + "b".repeat(64),
  workflow: "https://github.com/test/repo/actions/runs/1",
};

test("algorithm reports require their own Cloud branch and complete disjoint shards", () => {
  const raw = report();
  raw.branch.name = "algorithm-rust-std-stable";
  assert.throws(() => summarizeReport(raw, source, "example", [PREVIEW_CASE]));
  const a = summarizeReport(raw, source, "example", [PREVIEW_CASE], raw.branch.name);
  const other = PREVIEW_CASE.replace("random", "sorted");
  const b = {
    ...a,
    id: "01a1cdb1-9445-76dd-89c4-e4220d49e6f8",
    values: { [other]: a.values[PREVIEW_CASE] },
  };
  assert.deepEqual(Object.keys(mergeReports([a, b], [PREVIEW_CASE, other]).values), [
    PREVIEW_CASE,
    other,
  ]);
  for (const reports of [
    [],
    [a],
    [a, a],
    [a, { ...b, values: a.values }],
    [a, { ...b, spec: b.id }],
  ])
    assert.throws(() => mergeReports(reports, [PREVIEW_CASE, other]));
});

test("root publication isolates algorithm namespaces and is idempotent", async (t) => {
  const dir = await mkdtemp(join(tmpdir(), "algorithm-history-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  await publishRun(dir, run);
  const before = await readFile(join(dir, "tessera/2026-10.jsonl"), "utf8");
  const opponent: TrackingRun = {
    ...run,
    algorithm: "rust-std-stable",
    id: "01a1cdb1-9445-76dd-89c4-e4220d49e6f8",
  };
  await publishRun(dir, opponent);
  assert.equal(await readFile(join(dir, "tessera/2026-10.jsonl"), "utf8"), before);
  assert.equal(
    parseRuns(await readFile(join(dir, "rust-std-stable/2026-10.jsonl"), "utf8"))[0].algorithm,
    "rust-std-stable",
  );
  const index = JSON.parse(await readFile(join(dir, "index.json"), "utf8"));
  assert.deepEqual(
    index.algorithms.map((entry: { path: string }) => entry.path),
    ["tessera/", "rust-std-stable/"],
  );
  await publishRun(dir, opponent);
  const next = { ...run, id: "01a1cdb1-9445-76dd-89c4-e4220d49e6f9" };
  await publishRun(dir, next);
  assert.equal(parseRuns(await readFile(join(dir, "tessera/2026-10.jsonl"), "utf8")).length, 2);
  assert.deepEqual((await readdir(dir)).toSorted(), ["index.json", "rust-std-stable", "tessera"]);
  await assert.rejects(publishRun(dir, { ...opponent, rustc: "conflicting" }), /different data/);
  await assert.rejects(publishRun(dir, { ...opponent, algorithm: "../main" }));
  assert.deepEqual(
    comparableRuns([run, opponent]).map((entry) => entry.algorithm),
    ["rust-std-stable"],
  );
});

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
      type: "u64",
      length: "65,536",
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
  assert.equal(renderComparison([], [{ entry: index.cases[0], color: "violet" }]), null);
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
  // Filter values follow the catalog, not the order cases first appear in.
  const facets = trackingFacets(cases);
  assert.deepEqual(facets, {
    type: ["u32", "u64"],
    input: ["random", "sorted"],
    length: ["32", "65,536", "1,000,000"],
    group: ["serial", "parallel · 4 threads"],
  });
  assert.equal(indexSchema.parse({ months: [], cases, facets }).cases.length, ids.length);
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
  // Compared cases keep their own first measurement and leave unmeasured runs empty.
  const comparison = renderComparison(runs, [
    { entry: cases[0], color: "violet" },
    { entry: cases[2], color: "magenta" },
  ])!;
  assert.deepEqual(
    comparison.runs.map((row) =>
      row.values.map((value) => value && Number(value.change.toFixed(3))),
    ),
    [
      [0, null],
      [0.1, 0],
      [0.21, null],
      [0.21, -0.5],
    ],
  );
  assert.ok(comparison.svg.includes("series-magenta"));
  // The default time axis labels absolute times; the change axis labels percentages.
  const axisLabels = (svg: string) =>
    [...parseHTML(svg).document.querySelectorAll(".y-tick text")].map(
      (node) => node.textContent ?? "",
    );
  assert.ok(axisLabels(comparison.svg).length >= 3);
  assert.ok(axisLabels(comparison.svg).every((text) => text.endsWith("ns")));
  const changes = renderComparison(runs, [{ entry: cases[0], color: "violet" }], "change")!;
  assert.ok(axisLabels(changes.svg).every((text) => text.endsWith("%")));
  assert.equal(comparison.frame.start, Date.parse(runs[0].time));
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

test("host width expands the histories without changing their data or vertical layout", () => {
  const cases = trackingCases([PREVIEW_CASE]);
  const runs = [at(0, { [PREVIEW_CASE]: 100 }), at(1, { [PREVIEW_CASE]: 110 })];
  const compared = [{ entry: cases[0], color: "violet" as const }];
  const normal = renderComparison(runs, compared)!;
  const wide = renderComparison(runs, compared, "time", "wide", 1200)!;
  assert.equal(wide.frame.width, 1200);
  assert.equal(wide.frame.height, normal.frame.height);
  assert.deepEqual(wide.runs, normal.runs);
  assert.deepEqual(texts(wide.svg), texts(normal.svg));
  assert.equal(
    renderComparison(runs, compared, "time", "narrow", 320)!.frame.width,
    normal.frame.width,
  );

  const normalTrend = renderMainTrend(runs, cases);
  const wideTrend = renderMainTrend(runs, cases, "wide", undefined, 1200);
  const svg = (source: string) => parseHTML(source).document.querySelector("svg")!;
  assert.equal(Number(svg(wideTrend).getAttribute("width")), 1200);
  assert.equal(svg(wideTrend).getAttribute("height"), svg(normalTrend).getAttribute("height"));
  assert.deepEqual(texts(wideTrend), texts(normalTrend));
});

test("commit timestamps order every view independently of measurement order and axis spacing", () => {
  const cases = trackingCases([PREVIEW_CASE]);
  const compared = [{ entry: cases[0], color: "violet" as const }];
  // A slow/late rerun of the oldest commit must not become the newest commit.
  const early = { ...at(2, { [PREVIEW_CASE]: 100 }), commit_time: "2026-10-01T01:00:00.000Z" };
  const later = { ...at(0, { [PREVIEW_CASE]: 120 }), commit_time: "2026-10-01T04:30:00.000Z" };
  const rerun = {
    ...at(3, { [PREVIEW_CASE]: 110 }),
    hash: early.hash,
    commit_time: early.commit_time,
  };
  const input = [later, rerun, early];
  const ordered = comparableRuns(input);
  assert.deepEqual(
    ordered.map((r) => r.id),
    [early.id, rerun.id, later.id],
  );
  assert.equal(latestValues(input, cases)[0].value, 120);
  assert.ok(texts(renderMainTrend(input, cases)).includes("+20.0%"));
  for (const axis of ["time", "change"] as const) {
    const commits = renderComparison(input, compared, axis, "commits", 0, "commit")!;
    const time = renderComparison(input, compared, axis, "time", 0, "time")!;
    assert.deepEqual(
      commits.runs.map((r) => r.position),
      [0, 1, 2],
    );
    assert.deepEqual(
      time.runs.map((r) => r.position),
      [Date.parse(early.commit_time), Date.parse(early.commit_time), Date.parse(later.commit_time)],
    );
    assert.deepEqual(commits.baselines, time.baselines);
    assert.equal(commits.baselines[0]?.hash, early.hash);
    assert.equal(commits.baselines[0]?.time, early.time);
    assert.deepEqual(
      commits.runs.map((r) => r.values),
      time.runs.map((r) => r.values),
    );
    assert.deepEqual(
      commits.runs.map((r) => Number(r.values[0]!.change.toFixed(2))),
      [0, 0.1, 0.2],
    );
    assert.equal(commits.runs.filter((r) => r.hash === early.hash).length, 2);
  }
  // Zoom uses report identities, not a measured date interval that could include later.
  const zoom = comparableRuns(input, { first: early.id, last: rerun.id });
  assert.deepEqual(
    zoom.map((r) => r.id),
    [early.id, rerun.id],
  );
  assert.equal(renderComparison(zoom, compared)!.baselines[0]?.value, 100);
  assert.equal(
    renderComparison(comparableRuns(input, { first: rerun.id, last: later.id }), compared)!
      .baselines[0]?.value,
    110,
  );
  assert.deepEqual(comparableRuns(input, { first: "missing", last: later.id }), []);
  assert.deepEqual(comparableRuns(input, { first: later.id, last: early.id }), []);
});

test("legacy UTC times remain readable and optional commit times survive publication", async (t) => {
  assert.equal(historyTimestamp(run), Date.parse(run.time));
  const normalized = new Date("2026-11-01T00:15:30+08:00").toISOString();
  const withCommit = runSchema.parse({ ...run, commit_time: normalized });
  assert.equal(withCommit.commit_time, "2026-10-31T16:15:30.000Z");
  assert.equal(formatTimestamp(withCommit.commit_time!), "2026-10-31 16:15:30.000 UTC");
  assert.equal(historyTimestamp(withCommit), Date.parse(normalized));
  assert.throws(() => runSchema.parse({ ...run, commit_time: "not-a-time" }));
  const dir = await mkdtemp(join(tmpdir(), "commit-times-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  await appendRun(dir, withCommit);
  const stored = parseRuns(await readFile(join(dir, "2026-10.jsonl"), "utf8"));
  assert.equal(stored[0].commit_time, normalized);
  // The format remains readable to old readers which ignore new fields.
  const { commit_time: _, ...legacy } = withCommit;
  assert.deepEqual(runSchema.parse(legacy), run);
});

test("missing compared cases retain shared slots, independent baselines and gaps", () => {
  const b = "rust/u64/sorted/65536/s1/api",
    c = "rust/u32/random/32/s1/api";
  const cases = trackingCases([PREVIEW_CASE, b, c]);
  const entry = (id: string) => cases.find((c) => c.id === id)!;
  const input = [
    at(0, { [PREVIEW_CASE]: 100 }),
    at(1, { [c]: 50 }),
    at(2, { [PREVIEW_CASE]: 110, [b]: 200 }),
    at(3, { [c]: 60 }),
  ];
  const compared = [
    { entry: entry(PREVIEW_CASE), color: "violet" as const },
    { entry: entry(b), color: "magenta" as const },
  ];
  const result = renderComparison(input, compared, "change", "gaps", 0, "commit")!;
  assert.equal(result.runs.length, input.length);
  assert.deepEqual(result.runs[1].values, [null, null]);
  assert.equal(result.baselines[0]?.hash, input[0].hash);
  assert.equal(result.baselines[1]?.hash, input[2].hash);
  assert.equal(result.baselines[1]?.value, 200);
  const absent = { entry: entry(b), color: "magenta" as const };
  assert.equal(renderComparison([input[0]], [absent]), null);
  const partial = renderComparison([input[0]], [compared[0], absent])!;
  assert.equal(partial.baselines[1], null);
  assert.deepEqual(partial.runs[0].values[1], null);
  assert.ok(partial.svg.includes("no measurements in this range"));
  // A trailing unmeasured run must not reset the trend headline to zero.
  assert.ok(texts(renderMainTrend(input, [entry(PREVIEW_CASE)])).includes("+10.0%"));
  assert.ok(!result.svg.includes("NaN"));
});

test("the latest measured environment is selected before commit-time ordering", () => {
  const old = { ...at(0, { [PREVIEW_CASE]: 100 }), commit_time: "2026-12-01T00:00:00Z" };
  const newerEnvironment = {
    ...at(1, { [PREVIEW_CASE]: 200 }, "01a1cdb1-9445-76dd-89c4-e4220d49e000"),
    commit_time: "2026-10-01T00:00:00Z",
  };
  assert.deepEqual(comparableRuns([old, newerEnvironment]), [newerEnvironment]);
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
  // The comparison uses the same runs: two commits, changes against the first.
  const compared = renderComparison(runs, [{ entry: cases[0], color: "violet" }])!;
  assert.equal(parseHTML(compared.svg).document.querySelectorAll("a[href*='/commit/']").length, 2);
  assert.deepEqual(
    compared.runs.map((row) => Number(row.values[0]?.change.toFixed(3))),
    [0, 0.1],
  );
  assert.deepEqual(
    latestValues(runs, cases).map((row) => row.current),
    [true],
  );
});
