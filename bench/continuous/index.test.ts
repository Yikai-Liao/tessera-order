// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type TestContext, test } from "node:test";
import { VARIANTS, type Variant } from "../catalog.ts";
import { mockGitHub } from "./fixtures/github.ts";
import { baseHash, fixture, hash } from "./fixtures/report.ts";
import { publishReport } from "./index.ts";

function publication(t: TestContext, outcome = "success") {
  const dir = mkdtempSync(join(tmpdir(), "tessera-report-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const reportPath = join(dir, "report.json");
  writeFileSync(reportPath, JSON.stringify(fixture()));
  const outputs: Record<string, string> = {};
  const summaries: string[] = [];
  const mock = mockGitHub((request) => {
    if (request.path.includes("/git/")) {
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
    setOutput: (name: string, value: string) => {
      outputs[name] = value;
    },
    summary: {
      addRaw(text: string) {
        summaries.push(text);
        return this;
      },
      async write() {},
    },
  };
  return {
    ...mock,
    outputs,
    summaries,
    core,
    fetchHistory: (async () => new Response("[]")) as typeof fetch,
    context: {
      repo: { owner: "o", repo: "r" },
      payload: {},
      serverUrl: "https://github.com",
      runId: 1,
    },
    inputs: {
      language: "rust",
      hash,
      number: "7",
      baseRef: "main",
      startPoint: "main",
      baseHash,
      variants: [{ variant: VARIANTS[0] as Variant, outcome, reportPath, checkId: "42" }],
    },
  };
}

// The SVG files written to the image branch, by path.
function publishedImages(action: ReturnType<typeof publication>) {
  const files = action.requests
    .filter((request) => request.path.endsWith("/git/trees"))
    .flatMap((request) => request.body.tree as { path: string; content: string }[]);
  return new Map(files.map((file) => [file.path, file.content]));
}

// A report whose cases were all measured against nothing: Bencher records
// no start point when it cannot resolve one.
function withoutBaseline(input: ReturnType<typeof fixture>) {
  for (const result of input.results[0]) {
    for (const measure of result.measures) {
      measure.boundary = null;
      measure.threshold = undefined;
    }
  }
  input.alerts = [];
  return input;
}

test("build verdicts stay independent while one comment embeds shared images", async (t) => {
  const action = publication(t);
  // Synthetic second configuration keeps publication isolation covered.
  const alternate: Variant = {
    id: "alternate",
    name: "Alternate",
    testbed: "alternate-testbed",
    features: [],
  };
  const simd = fixture();
  simd.testbed.name = alternate.testbed;
  simd.alerts = simd.alerts.filter((alert) => alert.limit === "lower");
  const reportPath = join(action.inputs.variants[0].reportPath, "..", "simd.json");
  writeFileSync(reportPath, JSON.stringify(simd));
  action.inputs.variants.push({
    variant: alternate,
    outcome: "success",
    reportPath,
    checkId: "43",
  });
  await publishReport(action);
  const checks = action.requests.filter((request) => request.path.includes("/check-runs/"));
  assert.deepEqual(
    checks.map((request) => request.body.conclusion),
    ["failure", "success"],
  );
  const comments = action.requests.filter(
    (request) => request.method === "POST" && request.path.endsWith("/issues/7/comments"),
  );
  assert.equal(comments.length, 1);
  assert.deepEqual(
    [...publishedImages(action).keys()],
    ["pr-7/rust/comparison.svg", "pr-7/rust/trend.svg"],
  );
  for (const file of ["comparison", "trend"]) {
    const image = `https://raw.githubusercontent.com/o/r/image-commit/pr-7/rust/${file}.svg`;
    for (const output of [
      ...checks.map((check) => (check.body.output as { summary: string }).summary),
      String(comments[0].body.body),
      ...action.summaries,
    ])
      assert.ok(output.includes(image));
  }
});

for (const failure of [
  "valid report",
  "missing baseline",
  "wrong baseline",
  "missing commit",
  "wrong commit",
  "wrong build",
  "empty report",
  "invalid JSON",
] as const) {
  test(`PR reports fail closed: ${failure}`, async (t) => {
    const action = publication(t);
    const input = fixture();
    input.alerts = [];
    switch (failure) {
      case "missing baseline":
        input.branch.head.start_point = null;
        break;
      case "wrong baseline":
        input.branch.head.start_point!.version!.hash = "c".repeat(40);
        break;
      case "missing commit":
        input.branch.head.version = null;
        break;
      case "wrong commit":
        input.branch.head.version!.hash = "c".repeat(40);
        break;
      case "wrong build":
        input.testbed.name = "unrelated-testbed";
        break;
      case "empty report":
        input.results = [];
        break;
    }
    writeFileSync(
      action.inputs.variants[0].reportPath,
      failure === "invalid JSON" ? "{" : JSON.stringify(input),
    );
    await publishReport(action);
    assert.equal(
      action.requests.find((request) => request.path.endsWith("/check-runs/42"))!.body.conclusion,
      failure === "valid report" ? "success" : "failure",
    );
  });
}

test("a PR needs comparable cases; main may establish its first baseline", async (t) => {
  for (const number of ["7", undefined]) {
    const action = publication(t);
    writeFileSync(action.inputs.variants[0].reportPath, JSON.stringify(withoutBaseline(fixture())));
    await publishReport({ ...action, inputs: { ...action.inputs, number } });
    assert.equal(
      action.requests.find((request) => request.path.endsWith("/check-runs/42"))!.body.conclusion,
      number ? "failure" : "success",
    );
  }
});

for (const outcome of ["failure", "cancelled"]) {
  test(`unmeasured builds complete their check: ${outcome}`, async (t) => {
    const action = publication(t, outcome);
    await publishReport(action);
    assert.equal(publishedImages(action).size, 0);
    assert.equal(action.requests[0].body.conclusion, outcome);
  });
}
