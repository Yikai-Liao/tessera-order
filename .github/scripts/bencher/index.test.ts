// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test, type TestContext } from "node:test";
import { publishReport } from "./index.ts";
import { mockGitHub } from "./fixtures/github.ts";
import { fixture, hash } from "./fixtures/report.ts";

function publication(t: TestContext, outcome = "success", imageFailure = false) {
  const dir = mkdtempSync(join(tmpdir(), "tessera-report-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const reportPath = join(dir, "report.json");
  writeFileSync(reportPath, JSON.stringify(fixture()));
  const outputs: Record<string, string> = {};
  const summaries: string[] = [];
  const mock = mockGitHub(request => {
    if (request.path.includes("/git/")) {
      if (imageFailure) throw new Error("Image storage unavailable");
      if (request.path.includes("/matching-refs/")) return [];
      if (request.path.endsWith("/trees")) return { sha: "image-tree" };
      if (request.path.endsWith("/commits")) return { sha: "image-commit" };
    }
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
    fetchHistory: async () => new Response("[]"),
    context: { repo: { owner: "o", repo: "r" }, payload: {}, serverUrl: "https://github.com", runId: 1 },
    inputs: { language: "rust", hash, outcome, reportPath, checkId: "42", number: "7" },
  };
}

// The SVG files written to the image branch, by path.
function publishedImages(action: ReturnType<typeof publication>) {
  const tree = action.requests.find(request => request.path.endsWith("/git/trees"))?.body.tree as
    { path: string; content: string }[] | undefined;
  return new Map((tree ?? []).map(file => [file.path, file.content]));
}

test("embeds both published charts in the summary, check and PR comment", async t => {
  const action = publication(t);
  await publishReport(action);
  const images = publishedImages(action);
  assert.deepEqual([...images.keys()], ["pr-7/rust/comparison.svg", "pr-7/rust/trend.svg"]);
  assert.equal(images.get("pr-7/rust/comparison.svg"),
    readFileSync(new URL("./fixtures/chart.svg", import.meta.url), "utf8"));
  assert.ok(images.get("pr-7/rust/trend.svg")!.startsWith("<svg"));
  assert.match(action.summaries[0], /2 slower, 1 faster, 1 new/);
  const raw = "https://raw.githubusercontent.com/o/r/image-commit/pr-7/rust";
  for (const [alt, file] of [["Changes against main", "comparison"], ["PR benchmark history", "trend"]]) {
    const embed = `![${alt}](${raw}/${file}.svg)`;
    assert.ok(action.summaries[0].includes(embed), embed);
    const comment = action.requests.find(request => request.method === "POST" && request.path.endsWith("/issues/7/comments"));
    assert.ok(String(comment?.body.body).includes(embed), embed);
  }
  assert.doesNotMatch(action.summaries[0], /#artifacts/);
  assert.equal(action.requests.find(request => request.path.endsWith("/check-runs/42"))?.body.conclusion, "failure");
  assert.ok(action.requests.some(request => request.method === "POST" && request.path.endsWith("/issues/7/comments")));
});

test("passes a check when all active alerts are improvements", async t => {
  const action = publication(t);
  const input = fixture();
  input.alerts = input.alerts.filter(alert => alert.limit === "lower");
  writeFileSync(action.inputs.reportPath, JSON.stringify(input));
  await publishReport(action);
  assert.equal(action.requests.find(request => request.path.endsWith("/check-runs/42"))?.body.conclusion, "success");
});

test("keeps the current verdict and comparison when history is unavailable", async t => {
  const action = publication(t);
  action.fetchHistory = async () => new Response("unavailable", { status: 503 });
  await publishReport(action);
  assert.deepEqual([...publishedImages(action).keys()], ["pr-7/rust/comparison.svg"]);
  assert.match(action.summaries[0], /Changes against main/);
  assert.match(action.summaries[0], /\[Trend unavailable\]/);
  assert.equal(action.requests.find(request => request.path.endsWith("/check-runs/42"))?.body.conclusion, "failure");
});

test("reports image publication failures without masking metrics", async t => {
  const action = publication(t, "success", true);
  await publishReport(action);
  assert.match(action.summaries[0], /\[Images unavailable\]/);
  assert.ok(!action.summaries[0].includes("!["));
  assert.equal(action.requests.find(request => request.path.endsWith("/check-runs/42"))?.body.conclusion, "failure");
});

for (const outcome of ["failure", "cancelled"]) {
  test(`completes the check when measurement is ${outcome}`, async t => {
    const action = publication(t, outcome);
    await publishReport(action);
    assert.equal(publishedImages(action).size, 0);
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
    assert.equal(publishedImages(action).size, 0);
    assert.equal(action.requests[0].body.conclusion, "failure");
    assert.match(action.summaries[0], /measurement failed/);
  });
}
