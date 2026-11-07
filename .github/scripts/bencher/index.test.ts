// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test, type TestContext } from "node:test";
import { publishReport } from "./index.ts";
import { mockGitHub } from "./fixtures/github.ts";
import { fixture, hash } from "./fixtures/report.ts";

function publication(t: TestContext, outcome = "success") {
  const dir = mkdtempSync(join(tmpdir(), "tessera-report-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const reportPath = join(dir, "report.json");
  const chartPath = join(dir, "chart.svg");
  writeFileSync(reportPath, JSON.stringify(fixture()));
  const outputs: Record<string, string> = {};
  const summaries: string[] = [];
  const mock = mockGitHub(request => {
    if (request.path.endsWith("/pulls/7")) return { state: "open", head: { sha: hash } };
    if (request.method === "GET" && request.path.endsWith("/issues/7/comments")) return [];
    return {};
  });
  const core = {
    notice: () => {},
    setOutput: (name: string, value: string) => { outputs[name] = value; },
    summary: { addRaw(text: string) { summaries.push(text); return this; }, async write() {} },
  };
  return {
    ...mock, outputs, summaries, core,
    context: { repo: { owner: "o", repo: "r" }, payload: {}, serverUrl: "https://github.com", runId: 1 },
    inputs: { language: "rust", hash, outcome, reportPath, chartPath, checkId: "42", number: "7" },
  };
}

test("publishes the chart, summary, failing regression check and PR comment", async t => {
  const action = publication(t);
  await publishReport(action);
  assert.deepEqual(action.outputs, { chart: "true" });
  assert.equal(readFileSync(action.inputs.chartPath, "utf8"),
    readFileSync(new URL("./fixtures/chart.svg", import.meta.url), "utf8"));
  assert.match(action.summaries[0], /2 slower, 1 faster, 1 new/);
  assert.equal(action.requests.find(request => request.path.endsWith("/check-runs/42"))?.body.conclusion, "failure");
  assert.ok(action.requests.some(request => request.method === "POST" && request.path.endsWith("/issues/7/comments")));
});

test("passes a check when all active alerts are improvements", async t => {
  const action = publication(t);
  const input = fixture();
  input.alerts = input.alerts.filter(alert => alert.limit === "lower");
  writeFileSync(action.inputs.reportPath, JSON.stringify(input));
  await publishReport(action);
  assert.equal(action.requests[0].body.conclusion, "success");
});

for (const outcome of ["failure", "cancelled"]) {
  test(`completes the check when measurement is ${outcome}`, async t => {
    const action = publication(t, outcome);
    await publishReport(action);
    assert.deepEqual(action.outputs, {});
    assert.equal(action.requests[0].body.conclusion, outcome);
    assert.match(action.summaries[0], /measurement failed/);
    if (outcome === "cancelled") assert.equal(action.requests.length, 1);
  });
}

for (const [name, contents] of [
  ["unreadable", null],
  ["invalid JSON", "{"],
  ["empty", JSON.stringify({ ...fixture(), results: [] })],
]) {
  test(`fails the check for an ${name} report`, async t => {
    const action = publication(t);
    if (contents === null) rmSync(action.inputs.reportPath);
    else writeFileSync(action.inputs.reportPath, contents);
    await publishReport(action);
    assert.deepEqual(action.outputs, {});
    assert.equal(action.requests[0].body.conclusion, "failure");
    assert.match(action.summaries[0], /measurement failed/);
  });
}
