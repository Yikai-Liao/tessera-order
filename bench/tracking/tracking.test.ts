// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdir, mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { parseHTML } from "linkedom/worker";
import { z } from "zod";
import {
  columnId,
  type Execution,
  executionSection,
  executionThreads,
  parseCriterionId,
  SORT,
} from "../catalog.ts";
import { loadPlan } from "../plan.ts";
import type { RowChart } from "../plot/rows.ts";
import { columnPlan, trackingMemory } from "./build.ts";
import {
  comparisonData,
  GROWTH_FILE,
  latestRuns,
  trackingScaling,
  writeComparison,
} from "./comparison.ts";
import { explainFailure } from "./failure.ts";
import { growthPeaks } from "./growth.ts";
import {
  comparableRuns,
  formatTimestamp,
  historyTimestamp,
  latestValues,
  renderComparison,
  renderMainOverview,
  renderMainTrend,
} from "./history.ts";
import {
  columnSlug,
  trackingBranch,
  trackingColumn,
  trackingColumns,
  trackingImplementation,
  trackingImplementations,
  trackingSelection,
} from "./implementations.ts";
import { remoteJobs } from "./jobs.ts";
import { appendRun, trackingCases, trackingFacets } from "./publish.ts";
import {
  combineReports,
  type GrowthRecord,
  growthSchema,
  indexSchema,
  parseRuns,
  runSchema,
  summarizeReport,
  TRACKING_SPEC,
  type TrackingRun,
} from "./result.ts";

const PREVIEW_CASE = "rust/sort/u64/random/65536/s1/serial";

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
  ...summarizeReport(report(), source, "example", [PREVIEW_CASE], "main"),
  commit_time: "2026-10-31T00:30:00.000Z",
  rustc: "rustc test",
  rustflags: "",
  image: "registry.bencher.dev/example@sha256:" + "b".repeat(64),
  workflow: "https://github.com/test/repo/actions/runs/1",
  execution: "serial",
};

test("tracking excludes revision controls and operation-only references", () => {
  assert.ok(trackingImplementations.some((entry) => entry.id === "tessera"));
  assert.ok(
    trackingImplementations.every((entry) => !["revision", "reference"].includes(entry.adapter)),
  );
  for (const id of ["tessera-full", "tessera-copy", "tessera-baseline"]) {
    assert.throws(() => trackingImplementation(id), /Unknown tracking implementation/);
    assert.throws(() => trackingBranch(id), /Unknown tracking implementation/);
  }
});

test("each implementation's reports must come from its own Cloud branch", () => {
  const raw = report();
  raw.branch.name = "implementation-glidesort";
  assert.throws(() => summarizeReport(raw, source, "example", [PREVIEW_CASE], "main"));
  assert.ok(summarizeReport(raw, source, "example", [PREVIEW_CASE], "implementation-glidesort"));
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
    assert.throws(() => summarizeReport(r, source, "example", [PREVIEW_CASE], "main"));
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
      group: "sort",
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
  commit_time: `2026-11-${String(i + 1).padStart(2, "0")}T00:00:00Z`,
  hash: i.toString(16).padStart(40, "0"),
  spec,
  values: Object.fromEntries(Object.entries(values).map(([id, v]) => [id, [v, v, v]])),
});

test("cases follow execution, length, record type and catalog input order", () => {
  const ids = [
    "rust/sort/u64/random/1000000/s1/t4",
    "rust/sort/u64/sorted/65536/s1/serial",
    "rust/sort/u32/random/1000000/s1/serial",
    "rust/sort/u64/random/1000000/s1/t2",
    "rust/sort/u64/random/65536/s1/serial",
    "rust/sort/u64/random/32/s1/serial",
  ];
  const cases = trackingCases(ids);
  assert.deepEqual(
    cases.map((entry) => entry.id),
    [
      "rust/sort/u64/random/32/s1/serial",
      "rust/sort/u64/random/65536/s1/serial",
      "rust/sort/u64/sorted/65536/s1/serial",
      "rust/sort/u32/random/1000000/s1/serial",
      "rust/sort/u64/random/1000000/s1/t2",
      "rust/sort/u64/random/1000000/s1/t4",
    ],
  );
  const groups = ["serial", "t2", "t4"].map(
    (execution) => executionSection(SORT, execution as Execution).title,
  );
  assert.deepEqual(
    cases.map((entry) => entry.group),
    [groups[0], groups[0], groups[0], groups[0], groups[1], groups[2]],
  );
  // Filter values follow the catalog, not the order cases first appear in.
  const facets = trackingFacets(cases);
  assert.deepEqual(facets, {
    type: ["u32", "u64"],
    input: ["random", "sorted"],
    length: ["32", "65,536", "1,000,000"],
    group: groups,
  });
  assert.equal(indexSchema.parse({ months: [], cases, facets }).cases.length, ids.length);
});

test("the trend aligns gaps and chain-links aggregates across changing case sets", () => {
  const a = "rust/sort/u64/random/32/s1/serial",
    b = "rust/sort/u64/random/65536/s1/serial",
    c = "rust/sort/u64/sorted/65536/s1/serial";
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
  // Without links, the overview keeps only the execution aggregate.
  const overview = texts(renderMainOverview(runs, cases));
  assert.ok(overview.includes("sort") && !overview.includes("random"));
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

test("commit times are normalized and survive publication", async (t) => {
  const normalized = new Date("2026-11-01T00:15:30+08:00").toISOString();
  const withCommit = runSchema.parse({ ...run, commit_time: normalized });
  assert.equal(withCommit.commit_time, "2026-10-31T16:15:30.000Z");
  assert.equal(formatTimestamp(withCommit.commit_time), "2026-10-31 16:15:30.000 UTC");
  assert.equal(historyTimestamp(withCommit), Date.parse(normalized));
  assert.throws(() => runSchema.parse({ ...run, commit_time: "not-a-time" }));
  const dir = await mkdtemp(join(tmpdir(), "commit-times-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  await appendRun(dir, withCommit);
  const stored = parseRuns(await readFile(join(dir, "2026-10.jsonl"), "utf8"));
  assert.equal(stored[0].commit_time, normalized);
  const { commit_time: _, ...missing } = withCommit;
  assert.throws(() => runSchema.parse(missing));
});

test("missing compared cases retain shared slots, independent baselines and gaps", () => {
  const b = "rust/sort/u64/sorted/65536/s1/serial",
    c = "rust/sort/u32/random/32/s1/serial";
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

test("the columns of one commit form one link of the aggregate", () => {
  const serial = "rust/sort/u64/random/65536/s1/serial",
    parallel = "rust/sort/u64/random/65536/s1/t4";
  // Each commit records its serial and t4 columns as separate runs.
  const column = (index: number, commit: number, values: Record<string, number>) => ({
    ...at(index, values),
    hash: String(commit).repeat(40),
    commit_time: `2026-11-0${commit}T00:00:00Z`,
  });
  const runs = [
    column(0, 1, { [serial]: 100 }),
    column(1, 1, { [parallel]: 100 }),
    column(2, 2, { [serial]: 110 }),
    column(3, 2, { [parallel]: 110 }),
  ];
  // Both executions slow down by 10%: the aggregate is 10%, not the compounded 21%.
  const headline = (input: TrackingRun[]) =>
    renderMainTrend(input, trackingCases([serial, parallel])).match(/Tessera (\S+) run time/)?.[1];
  assert.equal(headline(runs), "+10.0%");
  // A later commit measuring only t4 moves the aggregate by t4 alone, inventing no serial value.
  assert.equal(headline([...runs, column(4, 3, { [parallel]: 121 })]), "+21.0%");
  // A rerun of the first commit chains like a commit: 100, rerun 110, then 120 is +20%.
  assert.equal(
    headline([
      column(0, 1, { [serial]: 100 }),
      column(1, 1, { [serial]: 110 }),
      column(2, 2, { [serial]: 120 }),
    ]),
    "+20.0%",
  );
});

test("the latest measured environment is selected before commit-time ordering", () => {
  const old = { ...at(0, { [PREVIEW_CASE]: 100 }), commit_time: "2026-12-01T00:00:00Z" };
  const newerEnvironment = {
    ...at(1, { [PREVIEW_CASE]: 200 }, "01a1cdb1-9445-76dd-89c4-e4220d49e000"),
    commit_time: "2026-10-01T00:00:00Z",
  };
  assert.deepEqual(comparableRuns([old, newerEnvironment]), [newerEnvironment]);
});

test("only runs on the latest hardware specification form the trend", () => {
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

test("remote jobs split an inventory by catalog budget, in order and balanced", () => {
  const short = (i: number) => `rust/sort/u64/random/${1000 + i}/s1/serial`;
  const long = (i: number) => `rust/sort/u64/random/${1000000 + i}/s1/t4`;
  // Tessera's continuous inventory: 54 short serial cases and 44 million-record cases.
  const tessera = [
    ...Array.from({ length: 54 }, (_, i) => short(i)),
    ...Array.from({ length: 44 }, (_, i) => long(i)),
  ];
  const jobs = remoteJobs(tessera);
  assert.equal(jobs.length, 3);
  assert.deepEqual(jobs.flat(), tessera);
  // Catalog warm-up plus window: 2 s below a million records, 5 s from there.
  const seconds = (job: string[]) =>
    job.reduce((sum, id) => sum + (parseCriterionId(id).len >= 1_000_000 ? 5 : 2), 0);
  // Midpoint assignment may exceed the 150 s budget by part of one case.
  for (const job of jobs) assert.ok(seconds(job) <= 0.5 * 300 + 5, String(seconds(job)));
  // An opponent measuring only the serial cases of the same inputs fits one job.
  const opponent = [
    ...Array.from({ length: 54 }, (_, i) => short(i)),
    ...Array.from({ length: 8 }, (_, i) => long(i).replace("/t4", "/serial")),
  ];
  assert.deepEqual(remoteJobs(opponent), [opponent]);
});

test("a run combines its remote jobs only for one source and hardware", () => {
  const other = "rust/sort/u64/random/1000/s1/serial";
  const second = report();
  second.uuid = "01a1cdb1-9445-76dd-89c4-e4220d49e6f8";
  second.results[0][0].benchmark.name = other;
  const jobs = [
    summarizeReport(report(), source, "example", [PREVIEW_CASE], "main"),
    summarizeReport(second, source, "example", [other], "main"),
  ];
  const run = combineReports(jobs);
  assert.equal(run.id, uuid);
  assert.deepEqual(Object.keys(run.values).sort(), [other, PREVIEW_CASE].sort());
  assert.throws(() => combineReports([jobs[0], { ...jobs[1], spec: second.uuid }]));
  assert.throws(() => combineReports([]));
});

test("known remote failures are explained once, unknown ones are not guessed", () => {
  assert.match(
    explainFailure(
      "x\nRemote job failed: Firecracker error: Timeout: VM execution timed out after 300s\n",
    )!,
    /longer than Bencher's 300 s limit/,
  );
  assert.match(
    explainFailure("Timed out waiting for remote job after 600 seconds")!,
    /within 600 s of waiting, queueing included/,
  );
  assert.equal(explainFailure("error: linker failed"), null);
});

test("memory rows key timed cases by column and must cover every timed case", () => {
  const row = (case_id: string, column: string, peak_extra_bytes: number) =>
    JSON.stringify({ case_id, column, build: "default", peak_extra_bytes, verified: true });
  const journal = [
    row("sort/u64/random/65536/s1", "tessera/serial", 4096),
    row("sort/u64/random/65536/s1", "tessera/t4", 8192),
    // Inputs and columns the run does not time are dropped.
    row("sort/u64/sorted/1000000/s1", "tessera/serial", 0),
    row("sort/u64/random/65536/s1", "std-stable/serial", 524288),
  ].join("\n");
  const serial = { implementation: "tessera", execution: "serial" as const };
  const parallel = { implementation: "tessera", execution: "t4" as const };
  const ids = ["rust/sort/u64/random/65536/s1/serial"];
  assert.deepEqual(trackingMemory(journal, serial, ids), { [ids[0]]: 4096 });
  assert.deepEqual(trackingMemory(journal, parallel, ["rust/sort/u64/random/65536/s1/t4"]), {
    "rust/sort/u64/random/65536/s1/t4": 8192,
  });
  assert.throws(
    () => trackingMemory(journal, serial, [...ids, "rust/sort/u64/d16/65536/s1/serial"]),
    /Missing memory rows/,
  );
  assert.throws(
    () =>
      trackingMemory(
        journal + "\n" + row("sort/u64/random/65536/s1", "tessera/serial", 1),
        serial,
        ids,
      ),
    /Duplicate/,
  );
});

test("a run's values carry its own execution", () => {
  assert.ok(!runSchema.safeParse({ ...run, execution: "t4" }).success);
  assert.ok(!runSchema.safeParse({ ...run, execution: "api" }).success);
  const { execution: _, ...missing } = run;
  assert.ok(!runSchema.safeParse(missing).success);
});

test("a run's memory covers exactly its timed cases", () => {
  assert.ok(runSchema.safeParse({ ...run, memory: { [PREVIEW_CASE]: 0 } }).success);
  assert.ok(!runSchema.safeParse({ ...run, memory: {} }).success);
  assert.ok(!runSchema.safeParse({ ...run, memory: { [PREVIEW_CASE]: 1, other: 1 } }).success);
  assert.ok(runSchema.safeParse(run).success);
});

test("the comparison ranks latest runs on Tessera's hardware over a common cohort", () => {
  const other = "01a1cdb1-9445-76dd-89c4-e4220d49e6f8";
  const tracked = (
    spec: string,
    time: string,
    ns: Record<string, number>,
    memory?: Record<string, number>,
  ): TrackingRun => {
    const values = Object.fromEntries(
      Object.entries(ns).map(([id, value]) => [`rust/${id}/serial`, [value, value, value]]),
    );
    return runSchema.parse({
      ...run,
      id: crypto.randomUUID(),
      spec,
      time,
      values,
      ...(memory
        ? {
            memory: Object.fromEntries(
              Object.entries(memory).map(([id, bytes]) => [`rust/${id}/serial`, bytes]),
            ),
          }
        : {}),
    });
  };
  const cases = [
    "sort/u64/random/65536/s1",
    "sort/u64/reverse/65536/s1",
    "sort/u64/random/1048576/s1",
    "sort/u64/random/1000/s1",
    "sort/u64/sorted/1000/s1",
  ];
  const at = (values: number[]) => Object.fromEntries(cases.map((id, i) => [id, values[i]]));
  const runs = new Map([
    [
      "tessera",
      [
        tracked(other, "2026-10-01T00:00:00Z", at([1, 1, 1, 1, 1])),
        tracked(uuid, "2026-11-01T00:00:00Z", at([10, 20, 100, 3, 1]), at([100, 100, 1000, 1, 1])),
      ],
    ],
    [
      "std-stable",
      [tracked(uuid, "2026-11-02T00:00:00Z", at([40, 20, 800, 2, 1]), at([1, 2, 3, 1, 1]))],
    ],
    // Latest on another spec: the comparison uses the earlier run on Tessera's spec.
    [
      "glidesort",
      [
        tracked(uuid, "2026-11-03T00:00:00Z", at([20, 20, 400, 2, 2])),
        tracked(other, "2026-11-04T00:00:00Z", at([1, 1, 1, 1, 1])),
      ],
    ],
    // Lacks the reverse input, so it is listed but not ranked as a loss or a win.
    ["radsort", [tracked(uuid, "2026-11-05T00:00:00Z", { [cases[0]]: 50 })]],
  ]);
  assert.equal(latestRuns(new Map())?.spec, undefined);
  const latest = latestRuns(runs)!;
  assert.equal(latest.spec, uuid);
  assert.equal(latest.runs.get("glidesort/serial")?.time, "2026-11-03T00:00:00Z");
  const data = comparisonData(runs)!;
  assert.deepEqual(
    data.operations[0].runs.map((entry) => [entry.implementation, entry.memory]),
    [
      ["tessera", true],
      ["std-stable", true],
      ["radsort", false],
      ["glidesort", false],
    ],
  );
  assert.deepEqual(data.overview, data.operations[0].figures[0]);
  assert.deepEqual(
    [data.operations[0].id, data.operations[0].label, data.operations[0].reference],
    ["sort", "Full sort", "slice::sort"],
  );
  const overview = data.overview!.chart as RowChart;
  assert.match(overview.guide, /the 2 measured input patterns/);
  assert.deepEqual(overview.columns, ["Speed", "Extra memory"]);
  const rows = overview.groups.flatMap((group) => group.rows);
  const tessera = rows.find((row) => row.strong)!;
  // Geometric mean of 40/10 and 20/20; the worst peak over the cohort's cases.
  assert.deepEqual(tessera.cells, ["2×", "100 B"]);
  assert.ok(!JSON.stringify(rows).includes("radsort"));
  // Lengths with a single pattern per type are threshold probes: the length figure and the
  // short-input figure use only lengths where some type has several patterns.
  const chart = (name: string) =>
    data.operations[0].figures.find((figure) => figure.name === name)!;
  const rowsOf = (name: string) =>
    (chart(name).chart as RowChart).groups.map((group) => [
      group.title,
      group.rows.map((row) => row.name.input),
    ]);
  assert.deepEqual(rowsOf("lengths"), [
    ["u64 keys · against any sort", ["1,000 elements", "65,536 elements"]],
  ]);
  assert.equal(chart("lengths").title, "Fastest from 65,536 elements up");
  assert.deepEqual(
    rowsOf("inputs").flatMap(([, rows]) => rows),
    ["random", "reverse"],
  );
  assert.deepEqual(rowsOf("short"), [["1,000 u64 keys", ["random", "sorted"]]]);
  // Without a cohort, the comparison is published empty, replacing any earlier one.
  const alone = comparisonData(new Map([["tessera", runs.get("tessera")!]]));
  assert.deepEqual([alone.spec, alone.overview, alone.operations], [uuid, null, []]);
  assert.equal(comparisonData(new Map()).spec, null);
  // Element types compare over the patterns every type has: here only random.
  const types = comparisonData(
    new Map([
      ...runs,
      [
        "tessera",
        [
          tracked(uuid, "2026-11-06T00:00:00Z", {
            ...at([10, 20, 100, 1, 1]),
            "sort/f64/random/65536/s1": 10,
          }),
        ],
      ],
      [
        "std-stable",
        [
          tracked(uuid, "2026-11-06T00:00:00Z", {
            ...at([40, 20, 800, 2, 1]),
            "sort/f64/random/65536/s1": 30,
          }),
        ],
      ],
    ]),
  ).operations[0].figures.find((figure) => figure.name === "types");
  assert.match(types!.chart.guide, /on the random input$/);
});

test("tracking columns fit the tracking hardware and a selection keeps PR coverage", () => {
  const ids = trackingColumns.map(columnId);
  assert.ok(
    trackingColumns.every(
      (column) => (executionThreads(column.execution) ?? 1) <= TRACKING_SPEC.cpus,
    ),
  );
  assert.ok(ids.includes("tessera/serial") && ids.includes("tessera/t1"));
  // A parallel-only implementation has no serial column.
  assert.ok(ids.includes("rayon-stable/t1") && !ids.includes("rayon-stable/serial"));
  assert.ok(!ids.includes("std-stable/t1"));
  assert.deepEqual(trackingSelection("tessera", ["serial", "t4"]).map(columnId), [
    "tessera/serial",
    "tessera/t4",
  ]);
  assert.deepEqual(trackingSelection("tessera/t2", ["serial"]).map(columnId), ["tessera/t2"]);
  assert.deepEqual(trackingSelection("all", []), trackingColumns);
  assert.throws(() => trackingSelection("rayon-stable", ["serial"]), /no column/);
  assert.throws(() => trackingSelection("tessera/t3", ["serial"]), /Unknown tracking column/);
  assert.equal(columnSlug(trackingColumn("tessera/t4")), "tessera-t4");
  // An implementation's columns share its Cloud branch.
  assert.equal(trackingBranch("rayon-stable"), "implementation-rayon-stable");
});

test("each tracking column measures its own slice of the continuous plan", async () => {
  const plan = await loadPlan("continuous");
  const serial = columnPlan(plan, { implementation: "std-stable", execution: "serial" });
  const parallel = columnPlan(plan, { implementation: "tessera", execution: "t2" });
  assert.deepEqual(serial.implementations, ["std-stable"]);
  for (const [column, execution] of [
    [serial, "serial"],
    [parallel, "t2"],
  ] as const) {
    assert.ok(column.include.length > 0);
    for (const rule of column.include) assert.deepEqual(rule.executions, [execution]);
  }
  // Each column keeps every rule of its execution kind, operation parameters included.
  const slice = (parallel: boolean, execution: string) =>
    plan.include
      .filter((rule) => (rule.executions ?? ["serial"]).some((e) => (e !== "serial") === parallel))
      .map((rule) => ({ ...rule, executions: [execution] }));
  assert.deepEqual(serial.include, slice(false, "serial"));
  assert.deepEqual(parallel.include, slice(true, "t2"));
  const kinds = (column: typeof serial) =>
    new Set(column.include.map((rule) => rule.operation?.kind ?? "sort"));
  assert.deepEqual(kinds(serial), new Set(["sort", "argsort", "partition-buckets", "select_nth"]));
});

test("the comparison ranks 32-bit-key records and adds parallel scaling", () => {
  const tracked = (values: Record<string, number>, execution = "serial") =>
    runSchema.parse({
      ...run,
      id: crypto.randomUUID(),
      execution,
      values: Object.fromEntries(Object.entries(values).map(([k, v]) => [k, [v, v, v]])),
    });
  const serial = (ns: number[]) =>
    tracked({
      "rust/sort/u64/random/65536/s1/serial": ns[0],
      "rust/sort/row_u32/random/49/s1/serial": ns[1],
      "rust/sort/row_u32/random/1000000/s1/serial": ns[2],
    });
  const parallel = (threads: number, ns: number) =>
    tracked({ [`rust/sort/u64/random/1000000/s1/t${threads}`]: ns }, `t${threads}`);
  const runs = new Map([
    ["tessera", [serial([10, 1, 100]), parallel(1, 50), parallel(2, 25)]],
    ["std-stable", [serial([20, 1, 400])]],
    ["rayon-stable", [parallel(1, 200), parallel(2, 100)]],
  ]);
  const data = comparisonData(runs);
  // Equal pattern counts take the longest length.
  assert.match(data.overview!.chart.guide, /^1,000,000 \{u32, u32\} records/);
  assert.equal(data.overview!.subject, "Overall ranking");
  // A faster unstable sort does not displace Tessera among stable record sorts.
  const unstable = comparisonData(new Map([...runs, ["std-unstable", [serial([1, 1, 1])]]]));
  assert.equal(unstable.overview!.title, "Fastest of 2 stable sorts");
  const latest = latestRuns(runs)!.runs;
  const scaling = trackingScaling(latest)!;
  assert.deepEqual([scaling.n, scaling.threads], [1000000, [1, 2]]);
  assert.deepEqual(scaling.inputs, [{ label: "u64 · random", tessera: [4, 8], rayon: [1, 2] }]);
  const figure = data.operations[0].figures.find((entry) => entry.name === "parallel")!;
  // The table names every input in the aggregate.
  assert.match(figure.table, /\| u64 · random \| 4× \| 8× \| 1.00× \| 2× \|/);
  const names = data.operations[0].figures.map((figure) => figure.name);
  assert.ok(names.includes("parallel"));
  // Without Rayon on one thread there is no baseline.
  latest.delete("rayon-stable/t1");
  assert.equal(trackingScaling(latest), null);
});

test("the growth sweep keeps one peak per implementation and length", () => {
  const plan = {
    schema: 3 as const,
    implementations: ["tessera", "glidesort"],
    include: [
      { types: ["row_u32" as const], distributions: ["random" as const], lengths: [1024, 4096] },
    ],
    exclude: [],
  };
  const row = (len: number, implementation: string, peak: number, execution = "serial") =>
    JSON.stringify({
      case_id: `sort/row_u32/random/${len}/s1`,
      column: `${implementation}/${execution}`,
      build: "default",
      peak_extra_bytes: peak,
      verified: true,
    });
  const journal = [
    row(1024, "tessera", 10),
    row(4096, "tessera", 20),
    row(4096, "glidesort", 0),
    // Parallel columns answer a different memory question and stay out of the sweep.
    row(4096, "tessera", 99, "t4"),
  ];
  assert.deepEqual(growthPeaks(journal.join("\n"), plan), {
    operation: "sort",
    type: "row_u32",
    distribution: "random",
    lengths: [1024, 4096],
    peaks: { tessera: [10, 20], glidesort: [null, 0] },
  });
  assert.throws(() => growthPeaks([...journal, journal[0]].join("\n"), plan), /Duplicate/);
  assert.throws(() => growthPeaks(journal.slice(1).join("\n"), plan), /every length/);
  assert.throws(() => growthPeaks(row(1024, "radsort", 1), plan), /Unplanned implementation/);
});

test("the growth sweep replaces the memory figure with curves over length", async (t) => {
  const tracked = (values: Record<string, number>) =>
    runSchema.parse({
      ...run,
      id: crypto.randomUUID(),
      values: Object.fromEntries(Object.entries(values).map(([k, v]) => [k, [v, v, v]])),
    });
  const runs = new Map([
    ["tessera", [tracked({ "rust/sort/u64/random/1000000/s1/serial": 100 })]],
    ["std-stable", [tracked({ "rust/sort/u64/random/1000000/s1/serial": 400 })]],
  ]);
  const growth: GrowthRecord = growthSchema.parse({
    time: run.time,
    hash: source,
    workflow: run.workflow,
    rustc: "rustc",
    native: { cc: "clang", cxx: "clang++", build: {} },
    operation: "sort",
    type: "row_u32",
    distribution: "random",
    lengths: [1 << 20, 1 << 22, 1 << 24],
    // Tessera doubles when the input quadruples: √n.
    peaks: { tessera: [2e6, 4e6, 8e6], "std-stable": [8e6, 32e6, 128e6], crumsort: [0, 0, 0] },
  });
  assert.equal(
    comparisonData(runs).operations[0].figures.find((figure) => figure.name === "memory")?.kind,
    undefined,
  );
  const figure = comparisonData(runs, growth).operations[0].figures.find(
    (entry) => entry.name === "memory",
  )!;
  assert.equal(figure.kind, "curves");
  assert.equal(figure.subject, "Extra memory by input length");
  assert.equal(figure.title, "Grows as √n, to 8 MB at 16M records");
  assert.match(figure.chart.guide, /Tessera at aaaaaaa/);
  // A sort that never reaches the axis is named in the note and keeps its values in the table.
  assert.match(JSON.stringify(figure.chart), /Below 1 kB at every length: crumsort \(0 B\)/);
  assert.match(figure.table, /\| crumsort \| 0 B \| 0 B \| 0 B \|/);
  assert.match(figure.table, /\| Tessera \| 2 MB \| 4 MB \| 8 MB \|/);
  // A sweep of another operation stays out of the sort section.
  assert.equal(
    comparisonData(runs, { ...growth, operation: "argsort" }).operations[0].figures.find(
      (entry) => entry.name === "memory",
    )?.kind,
    undefined,
  );

  // Publication reads the sweep from the data root.
  const root = await mkdtemp(join(tmpdir(), "tessera-growth-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  for (const [id, list] of runs) {
    await mkdir(join(root, id));
    await writeFile(join(root, id, "2026-11.jsonl"), `${JSON.stringify(list[0])}\n`);
  }
  await writeFile(join(root, GROWTH_FILE), JSON.stringify(growth));
  const written = await writeComparison(root);
  assert.equal(
    written.operations[0].figures.find((entry) => entry.name === "memory")?.kind,
    "curves",
  );
});
