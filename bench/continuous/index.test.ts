// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type TestContext, test } from "node:test";
import { continuous } from "../catalog.ts";
import { mockGitHub } from "./fixtures/github.ts";
import { writePaired } from "./fixtures/paired.ts";
import { baseHash, fixture, hash } from "./fixtures/report.ts";
import { publishReport } from "./index.ts";
import { pairedBmf } from "./paired.ts";

async function publication(t: TestContext, cloud = true, moved = false, ratio?: number) {
  const dir = await mkdtemp(join(tmpdir(), "tessera-paired-report-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const runs = await writePaired(dir, ratio ?? 1.2);
  if (cloud) {
    const report = fixture();
    report.testbed.name = continuous.testbed;
    report.branch.name = "pr-7";
    report.results = [
      Object.entries(pairedBmf(runs)).map(([name, metric]) => ({
        benchmark: { name, uuid: name },
        measures: [
          {
            measure: { name: "paired-time-ratio", uuid: "ratio" },
            metric: { uuid: name, value: metric["paired-time-ratio"].value },
          },
        ],
      })),
    ];
    await writeFile(join(dir, "bencher.json"), JSON.stringify(report));
  }
  const summaries: string[] = [];
  let id = 40;
  const mock = mockGitHub((request) => {
    if (request.path.endsWith("/check-runs") && request.method === "POST") return { id: ++id };
    if (request.path.includes("/git/")) {
      if (request.path.includes("/matching-refs/")) return [];
      if (request.path.endsWith("/trees")) return { sha: "image-tree" };
      if (request.path.endsWith("/commits")) return { sha: "image-commit" };
    }
    if (request.path.endsWith("/pulls/7"))
      return { state: "open", head: { sha: moved ? "c".repeat(40) : hash } };
    if (request.method === "GET" && request.path.endsWith("/issues/7/comments")) return [];
    return {};
  });
  return {
    ...mock,
    fetchHistory: async () =>
      new Response("[]", { headers: { "content-type": "application/json" } }),
    summaries,
    core: {
      notice: () => {},
      setOutput: () => {},
      summary: {
        addRaw(text: string) {
          summaries.push(text);
          return this;
        },
        async write() {},
      },
    },
    context: {
      repo: { owner: "o", repo: "r" },
      payload: {},
      serverUrl: "https://github.com",
      runId: 1,
    },
    inputs: {
      hash,
      baseHash,
      number: "7",
      baseRef: "main",
      artifacts: dir,
      prepared: join(dir, "prepared"),
      reports: dir,
    },
  };
}

test("a precise sub-threshold change is no alert, not statistical equivalence", async (t) => {
  const action = await publication(t, false, false, 1.04);
  await publishReport(action);
  assert.doesNotMatch(action.summaries[0], /no significant change/);
  assert.match(action.summaries[0], /no regression alert/);
  assert.match(action.summaries[0], /Overall run time \+4\.0%.*`main`.*10 cases/);
  assert.match(action.summaries[0], /No regression or improvement alerts/);
  const check = action.requests.find((request) => request.method === "PATCH")!;
  assert.match(
    (check.body.output as { summary: string }).summary,
    /Overall run time \+4\.0%.*10 cases/,
  );
  assert.match(action.summaries[0], /!\[Changes against main\]/);
  assert.match(action.summaries[0], /!\[PR benchmark history\]/);
  assert.match(action.summaries[0], /\| serial \|/);
  assert.match(action.summaries[0], /\| parallel · 4 threads \|/);
  assert.match(action.summaries[0], /`ci`/);
});

test("improvements publish successful configured checks", async (t) => {
  const action = await publication(t, true, false, 0.8);
  await publishReport(action);
  assert.deepEqual(
    action.requests
      .filter((request) => request.method === "PATCH")
      .map((request) => request.body.conclusion),
    ["success"],
  );
  assert.match(action.summaries[0], /0.8000×/);
  assert.match(action.summaries[0], /Overall run time −20\.0%/);
  assert.match(action.summaries[0], /\[!TIP\]/);
  assert.doesNotMatch(action.summaries[0], /No regression or improvement alerts/);
  assert.match(action.summaries[0], /\[Report\]\(/);
  assert.match(action.summaries[0], /\[JSON\]\(/);
  assert.match(action.summaries[0], /<details><summary>10 changed cases/);
});

test("raw paired samples drive configured checks and the existing shared chart", async (t) => {
  const action = await publication(t);
  await publishReport(action);
  const checks = action.requests.filter(
    (request) => request.method === "PATCH" && request.path.includes("/check-runs/"),
  );
  assert.deepEqual(
    checks.map((request) => request.body.conclusion),
    ["failure"],
  );
  const body = action.summaries[0];
  assert.match(body, /Overall run time \+20\.0%/);
  assert.match(body, /\[!CAUTION\]/);
  assert.doesNotMatch(body, /No regression or improvement alerts/);
  assert.match(body, /1.2000×/);
  assert.ok(body.includes("Median interval"));
  assert.doesNotMatch(body, /1.20 ns|0.800 ns/);
  assert.match(body, /serial/);
  assert.match(body, /4|Parallel/);
  const images = action.requests
    .filter((request) => request.path.endsWith("/git/trees"))
    .flatMap((request) => request.body.tree as { content: string }[]);
  assert.equal(images.length, 2);
  assert.match(images[0].content, /20/);
  assert.doesNotMatch(body, /Portable SIMD|Adjacent A\/B|Cloud archival runs separately/);
  assert.equal(
    action.requests.filter(
      (request) => request.method === "POST" && request.path.endsWith("/issues/7/comments"),
    ).length,
    1,
  );
});

test("missing shards fail the configured build", async (t) => {
  const action = await publication(t);
  await rm(join(action.inputs.artifacts, "paired-1"), { recursive: true });
  await assert.rejects(publishReport(action), /could not be validated/);
  const checks = action.requests.filter(
    (request) => request.method === "PATCH" && request.path.includes("/check-runs/"),
  );
  assert.deepEqual(
    checks.map((request) => request.body.conclusion),
    ["failure"],
  );
  assert.match(action.summaries[0], /Missing measurement shards/);
});

test("Cloud failure retains ratio tables and does not change performance checks", async (t) => {
  const action = await publication(t, false);
  await publishReport(action);
  assert.match(action.summaries[0], /1.2000×/);
  assert.deepEqual(
    action.requests
      .filter((request) => request.method === "PATCH")
      .map((request) => request.body.conclusion),
    ["failure"],
  );
});

test("Cloud contents cannot override performance evidence", async (t) => {
  const action = await publication(t);
  const path = join(action.inputs.artifacts, "bencher.json");
  const report = fixture();
  report.testbed.name = continuous.testbed;
  report.branch.name = "pr-7";
  report.results = [[]];
  await writeFile(path, JSON.stringify(report));
  await publishReport(action);
  assert.match(action.summaries[0], /1.2000×/);
});

test("superseded PR results stay in checks and summary, not a new comment", async (t) => {
  const action = await publication(t, true, true);
  await publishReport(action);
  assert.equal(
    action.requests.filter(
      (request) => request.method === "POST" && request.path.endsWith("/issues/7/comments"),
    ).length,
    0,
  );
  assert.equal(action.summaries.length, 1);
});

test("fork reports retain the summary without writes or credentials", async (t) => {
  const action = await publication(t, false);
  await assert.rejects(
    publishReport({ ...action, inputs: { ...action.inputs, publish: false } }),
    /regression alerts/,
  );
  assert.match(action.summaries[0], /1.2000×/);
  assert.equal(action.requests.length, 0);
});

test("missing Cloud history metadata cannot discard a valid measured verdict", async (t) => {
  const action = await publication(t, true, false, 0.8);
  const path = join(action.inputs.reports, "bencher.json");
  const report = JSON.parse(await readFile(path, "utf8"));
  delete report.start_time;
  await writeFile(path, JSON.stringify(report));
  await publishReport(action);
  assert.match(action.summaries[0], /0.8000×/);
  assert.match(action.summaries[0], /!\[PR benchmark history\]/);
  const check = action.requests.find(
    (request) => request.method === "PATCH" && request.path.includes("/check-runs/"),
  )!;
  assert.equal(check.body.conclusion, "success");
});

test("a changed inventory without shared history retains both current figures", async (t) => {
  const action = await publication(t, true, false, 0.8);
  const path = join(action.inputs.reports, "bencher.json");
  const previous = JSON.parse(await readFile(path, "utf8"));
  previous.uuid = "earlier";
  previous.start_time = "2026-11-07T01:00:00Z";
  previous.results = [
    [
      {
        benchmark: { name: "rust/u32/random/65536/s1/api", uuid: "other-case" },
        measures: [
          {
            measure: { name: "paired-time-ratio", uuid: "ratio" },
            metric: { uuid: "older-value", value: 1.2 },
          },
        ],
      },
    ],
  ];
  await publishReport({
    ...action,
    fetchHistory: async () =>
      new Response(JSON.stringify([previous]), { headers: { "content-type": "application/json" } }),
  });
  const images = action.requests
    .filter((request) => request.path.endsWith("/git/trees"))
    .flatMap((request) => request.body.tree as { content: string }[]);
  assert.equal(images.length, 2);
  assert.match(action.summaries[0], /!\[PR benchmark history\]/);
});
