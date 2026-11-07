// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test, type TestContext } from "node:test";
import { publishReport } from "./index.ts";
import { mockGitHub } from "./fixtures/github.ts";
import { fixture, hash } from "./fixtures/report.ts";
import { chartSvg } from "./chart.ts";
import { summarize } from "./report.ts";
import { VARIANTS, type Variant } from "./variants.ts";

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
    fetchHistory: (async () => new Response("[]")) as typeof fetch,
    context: { repo: { owner: "o", repo: "r" }, payload: {}, serverUrl: "https://github.com", runId: 1 },
    inputs: { language: "rust", hash, number: "7",
      variants: [{ variant: VARIANTS[0] as Variant, outcome, reportPath, checkId: "42" }],
    },
  };
}

// The SVG files written to the image branch, by path.
function publishedImages(action: ReturnType<typeof publication>) {
  const files = action.requests.filter(request => request.path.endsWith("/git/trees"))
    .flatMap(request => request.body.tree as { path: string; content: string }[]);
  return new Map(files.map(file => [file.path, file.content]));
}

test("embeds both published charts in the summary, check and PR comment", async t => {
  const action = publication(t);
  await publishReport(action);
  const images = publishedImages(action);
  assert.deepEqual([...images.keys()], ["pr-7/rust/comparison.svg", "pr-7/rust/trend.svg"]);
  assert.equal(images.get("pr-7/rust/comparison.svg"), chartSvg([{ name: "Default", summary: summarize(fixture()) }]));
  assert.match(images.get("pr-7/rust/trend.svg")!, /^<svg[^>]*>\n<title id="t">PR benchmark history: Default /);
  assert.match(action.summaries[0], /^### Benchmarks · rust · 2 slower, 1 faster$/m);
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
  writeFileSync(action.inputs.variants[0].reportPath, JSON.stringify(input));
  await publishReport(action);
  assert.equal(action.requests.find(request => request.path.endsWith("/check-runs/42"))?.body.conclusion, "success");
});

test("keeps the current verdict and comparison when history is unavailable", async t => {
  const action = publication(t);
  action.fetchHistory = async () => new Response("unavailable", { status: 503 });
  await publishReport(action);
  assert.deepEqual([...publishedImages(action).keys()], ["pr-7/rust/comparison.svg"]);
  assert.match(action.summaries[0], /!\[Changes against main\]/);
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
    assert.match(action.summaries[0], /\*\*Default\*\* was not measured/);
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
    if (contents === null) rmSync(action.inputs.variants[0].reportPath);
    else writeFileSync(action.inputs.variants[0].reportPath, contents);
    await publishReport(action);
    assert.equal(publishedImages(action).size, 0);
    assert.equal(action.requests[0].body.conclusion, "failure");
    assert.match(action.summaries[0], /\*\*Default\*\* was not measured/);
  });
}

function addSimd(action: ReturnType<typeof publication>) {
  const report = fixture();
  report.uuid = "r-simd";
  report.testbed = { ...report.testbed, uuid: "t-simd", name: VARIANTS[1].testbed };
  report.alerts = report.alerts.filter(alert => alert.limit === "lower");
  // Its baseline differs from Default, so a mixed comparison is observable.
  report.results[0][0].measures[0].boundary!.baseline = 1000;
  const reportPath = join(action.inputs.variants[0].reportPath, "..", "simd.json");
  writeFileSync(reportPath, JSON.stringify(report));
  action.inputs.variants.push({ variant: VARIANTS[1], outcome: "success", reportPath, checkId: "43" });
}

test("publishes one check per build and one comment with builds as columns", async t => {
  const action = publication(t);
  addSimd(action);
  const histories: string[] = [];
  action.fetchHistory = async (...args: Parameters<typeof fetch>) => {
    histories.push(String(args[0]));
    return new Response("[]");
  };
  await publishReport(action);
  const checks = action.requests.filter(request => request.path.includes("/check-runs/"));
  assert.deepEqual(checks.map(request => request.body.conclusion), ["failure", "success"]);
  const summary = (i: number) => String((checks[i].body.output as { summary: string }).summary);
  assert.match(summary(0), /^### Benchmarks · rust · Default · 2 slower/);
  assert.match(summary(0), /\*\*Overall run time −5\.1%\*\*/);
  assert.match(summary(1), /^### Benchmarks · rust · Portable SIMD · 1 faster/);
  assert.match(summary(1), /−26.4%/);
  // One pair of images holds both builds as columns.
  assert.deepEqual([...publishedImages(action).keys()], ["pr-7/rust/comparison.svg", "pr-7/rust/trend.svg"]);
  assert.match(publishedImages(action).get("pr-7/rust/comparison.svg")!, />Portable SIMD</);
  assert.deepEqual(histories.map(url => new URL(url).searchParams.get("testbed")), ["t-1", "t-simd"]);
  const comments = action.requests.filter(request => request.method === "POST" && request.path.endsWith("/issues/7/comments"));
  assert.equal(comments.length, 1);
  const body = String(comments[0].body.body);
  // Distinct cases: the sorted99 speedup in both builds counts once.
  assert.match(body, /^### Benchmarks · rust · 2 slower, 1 faster$/m);
  assert.equal(body.match(/^#{3,4} /gm)?.length, 1);
  assert.match(body, /^\|  \| Default \| Portable SIMD \|$/m);
  assert.match(body, /^\| \*\*All cases\*\* \| −5\.1% · \*\*2 slower\*\* · 1 faster \| −12\.9% · 1 faster \|$/m);
  // A single mode needs no per-mode rows.
  assert.doesNotMatch(body, /^\| stable \|/m);
  assert.match(body, /> \*\*2 cases slower\*\*: 2 in Default only\./);
  assert.match(body, /> - \{u32,u32\} · random · 48: Default \*\*\+10\.2%\*\*, Portable SIMD \+10\.2%/);
  assert.match(body, /^\*\*1 case faster\*\*: 1 in both\. Best: u64 · 99% sorted · 1,000,000, Default \*\*−29\.1%\*\*\.$/m);
  assert.equal(body.match(/\[!CAUTION\]/g)?.length, 1);
  assert.doesNotMatch(body, /\[!TIP\]|Nice work|Overall run time/);
  assert.match(body, /<details><summary>3 changed cases<\/summary>/);
  assert.match(body, /^\| Input \| Type \| N \| Mode \| Default \| Portable SIMD \|$/m);
  assert.match(body, /\| u64 \| 49 \| stable \| \*\*\+3\.8%\*\* <sub>736 ns<\/sub> \| −26\.4% <sub>736 ns<\/sub> \|/);
  assert.match(body, /1 new case without a baseline in each build/);
  assert.match(body, /\*\*Portable SIMD\*\* · `intel-v1-portable-simd`/);
  // Every table is folded below the charts.
  assert.doesNotMatch(body.replace(/<details>[\s\S]*?<\/details>/g, ""), /\| Input \|/);
  assert.ok(body.indexOf("](https://raw.githubusercontent.com/o/r/image-commit/pr-7/rust/trend.svg)") < body.indexOf("<details>"));
});

test("groups the overview by API mode when the suite has several", async t => {
  const action = publication(t);
  const input = fixture();
  const first = input.results[0][0];
  input.results[0].push({ ...structuredClone(first),
    benchmark: { uuid: "b-unstable", name: "rust/u64/random/49/s1/unstable" },
    measures: [{ ...structuredClone(first.measures[0]), metric: { uuid: "m-unstable", value: 709 } }] });
  writeFileSync(action.inputs.variants[0].reportPath, JSON.stringify(input));
  await publishReport(action);
  assert.match(action.summaries[0], /^\| stable \| −5\.1% · \*\*2 slower\*\* · 1 faster \|$/m);
  assert.match(action.summaries[0], /^\| unstable \| \+0\.0% \|$/m);
});

test("publishes both builds on main without loading PR history or commenting", async t => {
  const action = publication(t);
  addSimd(action);
  action.fetchHistory = async () => { throw new Error("Main must not load PR history"); };
  await publishReport({ ...action, inputs: { ...action.inputs, number: undefined } });
  assert.deepEqual([...publishedImages(action).keys()], ["main/rust/comparison.svg"]);
  assert.equal(action.requests.filter(request => request.path.includes("/check-runs/")).length, 2);
  assert.ok(action.requests.every(request => !request.path.includes("/issues/") && !request.path.includes("/pulls/")));
  assert.match(action.summaries[0], /^\|  \| Default \| Portable SIMD \|$/m);
  assert.doesNotMatch(action.summaries[0], /Trend unavailable/);
});

for (const failed of [0, 1]) {
  test(`preserves the other build when build ${failed} fails`, async t => {
    const action = publication(t);
    addSimd(action);
    action.inputs.variants[failed].outcome = "failure";
    await publishReport(action);
    const checks = action.requests.filter(request => request.path.includes("/check-runs/"));
    assert.equal(checks[failed].body.conclusion, "failure");
    assert.match(JSON.stringify(checks[failed].body.output), /measurement failed/);
    assert.doesNotMatch(JSON.stringify(checks[1 - failed].body.output), /measurement failed/);
    const name = VARIANTS[failed].name;
    // The charts draw the measured build only.
    assert.equal(publishedImages(action).size, 2);
    assert.doesNotMatch(publishedImages(action).get("pr-7/rust/comparison.svg")!, new RegExp(`>${name}<`));
    assert.match(action.summaries[0], new RegExp(`, ${name} failed$`, "m"));
    assert.match(action.summaries[0], new RegExp(`\\*\\*${name}\\*\\* was not measured: the Bencher job failed\\.`));
    // "only" never stands for "the other build failed".
    assert.doesNotMatch(action.summaries[0], / only\./);
    assert.doesNotMatch(action.summaries[0], new RegExp(`${name} —`));
  });
}

for (const mismatch of ["testbed", "commit"]) {
  test(`rejects a report with the wrong ${mismatch}`, async t => {
    const action = publication(t);
    const input = fixture();
    if (mismatch === "testbed") input.testbed.name = VARIANTS[1].testbed;
    else input.branch.head.version!.hash = "c".repeat(40);
    writeFileSync(action.inputs.variants[0].reportPath, JSON.stringify(input));
    await publishReport(action);
    assert.match(action.summaries[0], /does not match the expected configuration and commit/);
    assert.equal(action.requests[0].body.conclusion, "failure");
    assert.equal(publishedImages(action).size, 0);
  });
}
