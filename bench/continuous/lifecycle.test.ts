// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { VARIANTS } from "../catalog.ts";
import { mockGitHub, type Request } from "./fixtures/github.ts";
import { hash } from "./fixtures/report.ts";
import { activateReport, finishPendingReports, publishProgress, queueReport } from "./lifecycle.ts";
import { marker } from "./marker.ts";

const repo = { owner: "owner", repo: "tessera" };
const runUrl = "https://github.com/owner/tessera/actions/runs/123";

function fixture(respond: (request: Request) => unknown) {
  const outputs: Record<string, string> = {};
  return {
    ...mockGitHub(respond),
    outputs,
    context: {
      repo,
      serverUrl: "https://github.com",
      runId: 123,
      payload: {
        workflow_run: {
          workflow_id: 1,
          event: "push",
          head_sha: hash,
          head_branch: "main",
          head_repository: { full_name: "owner/tessera" },
        },
      },
    },
    core: {
      notice: () => {},
      setOutput: (name: string, value: string) => {
        outputs[name] = value;
      },
    },
  };
}

function queued(id = 1) {
  return {
    id,
    head_sha: hash,
    external_id: "123",
    status: "queued",
    name: "Benchmarks (rust, Default)",
  };
}

for (const variant of VARIANTS) {
  test(`queues ${variant.id} on the exact source before measurement`, async () => {
    const f = fixture(() => ({ id: 42 }));
    assert.equal(await queueReport({ ...f, hash, variant }), "42");
    assert.deepEqual(f.requests[0].body, {
      name: `Benchmarks (rust, ${variant.name})`,
      head_sha: hash,
      external_id: "123",
      status: "queued",
      details_url: runUrl,
      output: {
        title: "Queued for measurement",
        summary: `### Benchmarks · rust · queued\n\n**${hash.slice(0, 7)}**: Waiting for the shared benchmark worker. No performance result is available yet.\n\n[Tracking workflow](${runUrl})`,
      },
    });
  });
}

test("activates the queued check without creating a second check", async () => {
  const f = fixture((r) => (r.method === "GET" ? queued() : {}));
  await activateReport({ ...f, hash, variant: VARIANTS[0], checkId: "1" });
  assert.deepEqual(
    f.requests.map((r) => r.method),
    ["GET", "PATCH"],
  );
  assert.equal(f.requests[1].body.status, "in_progress");
  assert.equal(f.requests[1].path, "/repos/owner/tessera/check-runs/1");
});

for (const [field, value] of [
  ["head_sha", "b".repeat(40)],
  ["external_id", "124"],
  ["name", "Benchmarks (rust, Portable SIMD)"],
])
  test(`does not activate a check with mismatched ${field}`, async () => {
    const f = fixture(() => ({ ...queued(), [field]: value }));
    await assert.rejects(activateReport({ ...f, hash, variant: VARIANTS[0], checkId: "1" }));
    assert.ok(f.requests.every((r) => r.method === "GET"));
  });

test("failed-job reruns create a new check without reopening the completed verdict", async () => {
  const f = fixture((r) =>
    r.method === "GET"
      ? { ...queued(), status: "completed", conclusion: "failure" }
      : r.method === "POST"
        ? { id: 99 }
        : {},
  );
  assert.equal(await activateReport({ ...f, hash, variant: VARIANTS[0], checkId: "1" }), "99");
  assert.deepEqual(
    f.requests.map((r) => r.method),
    ["GET", "POST", "PATCH"],
  );
  assert.equal(f.requests[2].path, "/repos/owner/tessera/check-runs/99");
});

test("cleanup closes orphaned checks without replacing a completed newer report", async () => {
  const f = fixture((r) =>
    r.method === "GET"
      ? {
          total_count: 3,
          check_runs: [
            queued(1),
            { ...queued(2), status: "completed" },
            { ...queued(3), status: "completed", name: "Benchmarks (rust, Portable SIMD)" },
          ],
        }
      : {},
  );
  await finishPendingReports({
    ...f,
    hash,
    number: "7",
    variants: VARIANTS,
    conclusion: "failure",
    reason: "orphaned check",
  });
  assert.deepEqual(
    f.requests.map((r) => r.method),
    ["GET", "PATCH"],
  );
  assert.equal(f.requests[1].path, "/repos/owner/tessera/check-runs/1");
});

test("rejects an invalid check ID before contacting GitHub", async () => {
  const f = fixture(() => {
    throw new Error("Unexpected request");
  });
  for (const checkId of ["", "NaN", "-1", "1.5", "9007199254740992"])
    await assert.rejects(activateReport({ ...f, hash, variant: VARIANTS[0], checkId }));
  assert.equal(f.requests.length, 0);
});

test("cleanup recovers unknown check IDs and preserves completed or foreign checks", async () => {
  const checks = [
    queued(1),
    { ...queued(2), name: "Benchmarks (rust, Portable SIMD)", status: "in_progress" },
    { ...queued(3), status: "completed", conclusion: "failure" },
    { ...queued(4), external_id: "124" },
    { ...queued(5), head_sha: "b".repeat(40) },
    { ...queued(6), name: "CI" },
  ];
  const f = fixture((r) => {
    if (r.path.endsWith("/check-runs") && r.method === "GET")
      return { total_count: checks.length, check_runs: checks };
    if (r.path.endsWith("/actions/workflows/1"))
      return { path: ".github/workflows/bench-build.yml" };
    if (r.method === "PATCH") return {};
    throw new Error(`Unexpected request: ${r.path}`);
  });
  await finishPendingReports({
    ...f,
    hash,
    variants: VARIANTS,
    conclusion: "cancelled",
    reason: "tracking was cancelled",
  });
  assert.equal(f.requests[0].query.filter, "all");
  const updates = f.requests.filter((r) => r.method === "PATCH");
  assert.deepEqual(
    updates.map((r) => r.path),
    ["/repos/owner/tessera/check-runs/1", "/repos/owner/tessera/check-runs/2"],
  );
  assert.ok(
    updates.every((r) => r.body.status === "completed" && r.body.conclusion === "cancelled"),
  );
});

test("completed results need no cleanup, even when the tracking job failed later", async () => {
  const f = fixture(() => ({ total_count: 1, check_runs: [{ ...queued(), status: "completed" }] }));
  await finishPendingReports({
    ...f,
    hash,
    variants: VARIANTS,
    conclusion: "failure",
    reason: "tracking ended",
  });
  assert.equal(f.requests.length, 1);
});

for (const state of ["queued", "running"] as const) {
  test(`publishes ${state} feedback for the current head and minimizes stale results`, async () => {
    const f = fixture((r) => {
      if (r.path.endsWith("/pulls/7")) return { state: "open", head: { sha: hash } };
      if (r.path.endsWith("/issues/7/comments") && r.method === "GET")
        return [
          {
            id: 9,
            node_id: "old",
            user: { login: "github-actions[bot]" },
            body: `${marker("rust", "b".repeat(40))}\n2 faster`,
          },
        ];
      return {};
    });
    await publishProgress({ ...f, hash, number: "7", state });
    const comment = f.requests.find(
      (r) => r.method === "POST" && r.path.endsWith("/issues/7/comments"),
    );
    assert.ok(comment);
    assert.match(
      String(comment.body.body),
      new RegExp(`^<!-- tessera-bench-report language=rust sha=${hash} -->`),
    );
    assert.match(String(comment.body.body), new RegExp(`Benchmarks · rust · ${state}`));
    assert.doesNotMatch(String(comment.body.body), /2 faster|clean|passed/);
    assert.ok(
      f.requests.some(
        (r) => r.path === "/graphql" && String(r.body.query).includes("minimizeComment"),
      ),
    );
  });
}

test("cleanup recovers a lost source handoff and publishes interruption for its current PR", async () => {
  const f = fixture((r) => {
    if (r.path.endsWith("/check-runs") && r.method === "GET")
      return { total_count: 1, check_runs: [queued()] };
    if (r.path.endsWith("/actions/workflows/1"))
      return { path: ".github/workflows/bench-build.yml" };
    if (r.path.endsWith(`/commits/${hash}/pulls`))
      return [
        {
          number: 7,
          state: "open",
          head: { sha: hash, ref: "feature", repo: { full_name: "owner/tessera" } },
          base: { sha: "b".repeat(40), ref: "main", repo: { full_name: "owner/tessera" } },
        },
      ];
    if (r.path.endsWith("/pulls/7")) return { state: "open", head: { sha: hash } };
    if (r.path.endsWith("/issues/7/comments") && r.method === "GET") return [];
    if (r.method !== "GET") return {};
    throw new Error(`Unexpected request: ${r.path}`);
  });
  f.context.payload.workflow_run.event = "pull_request";
  await finishPendingReports({
    ...f,
    hash,
    variants: VARIANTS,
    conclusion: "failure",
    reason: "setup failed",
  });
  assert.ok(f.requests.some((r) => r.method === "PATCH" && r.body.conclusion === "failure"));
  const comment = f.requests.find(
    (r) => r.method === "POST" && r.path.endsWith("/issues/7/comments"),
  );
  assert.ok(comment);
  assert.match(String(comment.body.body), /interrupted[\s\S]*setup failed/);
  assert.doesNotMatch(String(comment.body.body), /was not measured|clean|passed/);
});

test("cleanup attempts both configurations even if one update fails", async () => {
  const f = fixture((r) => {
    if (r.method === "GET")
      return {
        total_count: 2,
        check_runs: [queued(), { ...queued(2), name: "Benchmarks (rust, Portable SIMD)" }],
      };
    if (r.path.endsWith("/check-runs/1")) throw new Error("API unavailable");
    return {};
  });
  await assert.rejects(
    finishPendingReports({
      ...f,
      hash,
      variants: VARIANTS,
      conclusion: "failure",
      reason: "tracking ended",
    }),
    AggregateError,
  );
  assert.equal(f.requests.filter((r) => r.method === "PATCH").length, 2);
});

test("obsolete queued feedback never updates the current-head comment", async () => {
  const f = fixture(() => ({ state: "open", head: { sha: "b".repeat(40) } }));
  await publishProgress({ ...f, hash, number: "7", state: "queued" });
  assert.ok(f.requests.every((r) => r.method === "GET"));
});
