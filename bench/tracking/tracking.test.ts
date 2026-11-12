// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { renderHistory } from "../plot/history.ts";
import { appendRun } from "./publish.ts";
import {
  indexSchema,
  PREVIEW_CASE,
  parseRuns,
  summarizeReport,
  type TrackingRun,
} from "./result.ts";

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
  plan_hash: "c".repeat(64),
  executable_hash: "d".repeat(64),
  lock_hash: "e".repeat(64),
};

test("only complete matching absolute reports enter history", () => {
  assert.deepEqual(run.values[PREVIEW_CASE], [10, 9, 11]);
  const mutations = [
    (r: ReturnType<typeof report>) => {
      r.branch.head.version.hash = "0".repeat(40);
    },
    (r: ReturnType<typeof report>) => {
      r.project.slug = "other";
    },
    (r: ReturnType<typeof report>) => {
      r.testbed.name = "ci";
    },
    (r: ReturnType<typeof report>) => {
      r.results = [[]];
    },
    (r: ReturnType<typeof report>) => {
      r.results[0].push(r.results[0][0]);
    },
    (r: ReturnType<typeof report>) => {
      r.results[0][0].measures[0].measure.slug = "paired-time-ratio";
    },
    (r: ReturnType<typeof report>) => {
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
  assert.deepEqual(
    index.months.map((m) => m.month),
    ["2026-10", "2026-11"],
  );
  assert.equal(index.cases[0].id, PREVIEW_CASE);
  const all = [];
  for (const { file } of index.months) {
    const text = await readFile(join(dir, file), "utf8");
    assert.ok(file.includes(createHash("sha256").update(text).digest("hex")));
    all.push(...parseRuns(text));
  }
  assert.deepEqual(
    all.map((r) => r.time),
    [run.time, next.time],
  );
  assert.notEqual(await readFile(join(dir, "preview.svg"), "utf8"), first);
  assert.equal(renderHistory([]), "");
});
