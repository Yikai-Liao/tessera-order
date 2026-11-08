// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { mockGitHub } from "./fixtures/github.ts";
import { hash } from "./fixtures/report.ts";
import { marker, publishComment, resolveSource, startReport } from "./github.ts";
import { VARIANTS } from "./variants.ts";

const repo = { owner: "owner", repo: "tessera" };
const fullName = "owner/tessera";
const pull = {
  number: 7,
  state: "open",
  head: { sha: hash, ref: "main", repo: { full_name: "fork/tessera" } },
  base: { sha: "b".repeat(40), ref: "main", repo: { full_name: fullName } },
};

function sourceFixture(
  event = "pull_request",
  pulls = [structuredClone(pull)],
  workflowPath = ".github/workflows/bench-build.yml",
  // Pull requests whose head is the base branch of a stacked PR.
  parents?: unknown[],
) {
  const outputs: Record<string, string> = {};
  const mock = mockGitHub((request) => {
    if (request.path.endsWith("/actions/workflows/123")) return { path: workflowPath };
    if (request.path.endsWith(`/commits/${hash}/pulls`)) return pulls;
    if (request.path.endsWith("/pulls") && parents) return parents;
    throw new Error(`Unexpected request: ${request.path}`);
  });
  return {
    ...mock,
    outputs,
    context: {
      repo,
      serverUrl: "https://github.com",
      runId: 1,
      payload: {
        workflow_run: {
          event,
          workflow_id: 123,
          head_sha: hash,
          head_branch: "main",
          head_repository: { full_name: fullName },
          pull_requests: [],
        },
      },
    },
    core: {
      setOutput: (key: string, value: string) => {
        outputs[key] = value;
      },
      notice: () => {},
    },
  };
}

test("fork named main uses PR identity, never the baseline", async () => {
  const fixture = sourceFixture();
  await resolveSource(fixture);
  assert.deepEqual(fixture.outputs, {
    branch: "pr-7",
    hash,
    base: "main",
    start_point: "main",
    base_hash: "b".repeat(40),
    number: "7",
    eligible: "true",
  });
});

const stacked = {
  ...structuredClone(pull),
  base: { sha: "b".repeat(40), ref: "perf/reverse-fast-path", repo: { full_name: fullName } },
};

function parent(number: number, state = "open", repo = fullName) {
  return {
    number,
    state,
    head: { sha: "c".repeat(40), ref: "perf/reverse-fast-path", repo: { full_name: repo } },
    base: { sha: "d".repeat(40), ref: "main", repo: { full_name: fullName } },
  };
}

test("a stacked PR compares with its parent's samples at the base commit", async () => {
  const fixture = sourceFixture("pull_request", [stacked], undefined, [parent(41)]);
  await resolveSource(fixture);
  assert.deepEqual(fixture.outputs, {
    branch: "pr-7",
    hash,
    base: "perf/reverse-fast-path",
    start_point: "pr-41",
    base_hash: "b".repeat(40),
    number: "7",
    eligible: "true",
  });
});

test("a merged parent keeps the branch name and the Bencher branch", async () => {
  const fixture = sourceFixture("pull_request", [stacked], undefined, [parent(41, "closed")]);
  await resolveSource(fixture);
  assert.equal(fixture.outputs.start_point, "pr-41");
  assert.equal(fixture.outputs.base, "perf/reverse-fast-path");
});

test("a stacked PR without a parent names why it has no baseline", async () => {
  const fixture = sourceFixture("pull_request", [stacked], undefined, []);
  await resolveSource(fixture);
  assert.equal(fixture.outputs.start_point, undefined);
  assert.equal(fixture.outputs.base_hash, undefined);
  assert.equal(
    fixture.outputs.baseline_reason,
    "the base branch perf/reverse-fast-path has no pull request",
  );
});

for (const [name, parents] of [
  ["several pull requests", [parent(41), parent(42)]],
  ["a pull request from a fork", [parent(41, "open", "fork/tessera")]],
] as const) {
  test(`a stacked base branch with ${name} has no baseline`, async () => {
    const fixture = sourceFixture("pull_request", [stacked], undefined, [...parents]);
    await resolveSource(fixture);
    assert.equal(fixture.outputs.start_point, undefined);
    assert.ok(fixture.outputs.baseline_reason);
  });
}

for (const event of ["push", "workflow_dispatch"]) {
  test(`${event} on main updates the baseline`, async () => {
    const fixture = sourceFixture(event);
    await resolveSource(fixture);
    assert.deepEqual(fixture.outputs, { branch: "main", hash, eligible: "true" });
  });

  for (const field of ["head_branch", "head_repository"]) {
    test(`${event} rejects non-main or foreign source: ${field}`, async () => {
      const fixture = sourceFixture(event);
      if (field === "head_branch") fixture.context.payload.workflow_run.head_branch = "feature";
      else fixture.context.payload.workflow_run.head_repository = { full_name: "fork/tessera" };
      await assert.rejects(resolveSource(fixture), /Only main/);
    });
  }
}

test("rejects a different workflow with the same display name", async () => {
  await assert.rejects(
    resolveSource(sourceFixture("pull_request", [pull], ".github/workflows/other.yml")),
    /Unexpected source workflow/,
  );
});

test("rejects unexpected event types", async () => {
  await assert.rejects(resolveSource(sourceFixture("release")), /Unexpected source event/);
});

for (const [name, pr] of [
  ["closed", { ...pull, state: "closed" }],
  ["superseded", { ...pull, head: { ...pull.head, sha: "c".repeat(40) } }],
  ["foreign", { ...pull, base: { ...pull.base, repo: { full_name: "foreign/repo" } } }],
] as const) {
  test(`skips ${name} PR`, async () => {
    const fixture = sourceFixture("pull_request", [pr]);
    await resolveSource(fixture);
    assert.deepEqual(fixture.outputs, {});
  });
}

test("skips a commit with no PR", async () => {
  const fixture = sourceFixture("pull_request", []);
  await resolveSource(fixture);
  assert.deepEqual(fixture.outputs, {});
});

test("rejects ambiguous PR association", async () => {
  const fixture = sourceFixture("pull_request", [pull, { ...pull, number: 8 }]);
  await assert.rejects(resolveSource(fixture), /Ambiguous PR/);
});

interface Comment {
  id: number;
  node_id: string;
  user: { login: string };
  body: string;
  isMinimized?: boolean;
}

function commentFixture(comments: Comment[], headSha = hash, state = "open") {
  const mock = mockGitHub((request) => {
    if (request.method === "GET" && request.path.endsWith("/pulls/7")) {
      return { state, head: { sha: headSha } };
    }
    if (request.method === "GET" && request.path.endsWith("/issues/7/comments")) return comments;
    if (request.path === "/graphql") {
      return { data: { node: { isMinimized: comments.some((comment) => comment.isMinimized) } } };
    }
    return {};
  });
  return {
    ...mock,
    repo,
    number: 7,
    language: "rust",
    hash,
    body: "new",
    core: { notice: () => {} },
  };
}

const bot = { login: "github-actions[bot]" };

test("posts a new report and minimizes earlier ones in the same language", async () => {
  const fixture = commentFixture([
    { id: 1, node_id: "n1", user: bot, body: `${marker("rust", "c".repeat(40))}\nold` },
    { id: 2, node_id: "n2", user: bot, body: "<h2><a>Bencher Report</a></h2>" },
    { id: 3, node_id: "n3", user: { login: "someone" }, body: marker("rust", "d".repeat(40)) },
    { id: 4, node_id: "n4", user: bot, body: `${marker("cpp", "c".repeat(40))}\nother language` },
  ]);
  await publishComment(fixture);
  const writes = fixture.requests.filter((request) => request.method !== "GET");
  assert.deepEqual(writes[0].body, { body: `${marker("rust", hash)}\nnew` });
  assert.equal(writes[0].path, "/repos/owner/tessera/issues/7/comments");
  assert.deepEqual(
    writes.slice(1).map((request) => request.body.variables),
    [{ id: "n1" }, { id: "n2" }],
  );
  assert.ok(
    writes.slice(1).every((request) => String(request.body.query).includes("minimizeComment(")),
  );
});

test("updates and expands the report when returning to an earlier measured head", async () => {
  const fixture = commentFixture([
    { id: 5, node_id: "n5", user: bot, body: `${marker("rust", hash)}\nold`, isMinimized: true },
    { id: 6, node_id: "n6", user: bot, body: `${marker("rust", "c".repeat(40))}\nnewer head` },
  ]);
  await publishComment(fixture);
  const writes = fixture.requests.filter((request) => request.method !== "GET");
  assert.equal(writes[0].method, "PATCH");
  assert.equal(writes[0].path, "/repos/owner/tessera/issues/comments/5");
  assert.deepEqual(writes[0].body, { body: `${marker("rust", hash)}\nnew` });
  assert.match(String(writes[1].body.query), /isMinimized/);
  assert.deepEqual(writes[1].body.variables, { id: "n5" });
  assert.match(String(writes[2].body.query), /unminimizeComment/);
  assert.deepEqual(writes[2].body.variables, { id: "n5" });
  assert.match(String(writes[3].body.query), /minimizeComment/);
  assert.deepEqual(writes[3].body.variables, { id: "n6" });
});

test("rerunning a visible report updates it without changing its visibility", async () => {
  const fixture = commentFixture([
    { id: 5, node_id: "n5", user: bot, body: `${marker("rust", hash)}\nold` },
  ]);
  await publishComment(fixture);
  assert.equal(fixture.requests.filter((request) => request.method === "PATCH").length, 1);
  assert.ok(fixture.requests.every((request) => !String(request.body.query).includes("mutation(")));
});

for (const [name, headSha, state] of [
  ["superseded", "e".repeat(40), "open"],
  ["closed", hash, "closed"],
]) {
  test(`does not comment for a ${name} PR`, async () => {
    const fixture = commentFixture([], headSha, state);
    await publishComment(fixture);
    assert.ok(fixture.requests.every((request) => request.method === "GET"));
  });
}

for (const variant of VARIANTS) {
  test(`starts the ${variant.id} check on the measured head`, async () => {
    const mock = mockGitHub(() => ({ id: 42 }));
    const outputs: Record<string, string> = {};
    await startReport({
      github: mock.github,
      hash,
      variant,
      context: { repo, payload: {}, serverUrl: "https://github.com", runId: 123 },
      core: {
        notice: () => {},
        setOutput: (name, value) => {
          outputs[name] = value;
        },
      },
    });
    assert.deepEqual(outputs, { [variant.id]: "42" });
    assert.deepEqual(mock.requests[0].body, {
      name: `Benchmarks (rust, ${variant.name})`,
      head_sha: hash,
      status: "in_progress",
      details_url: "https://github.com/owner/tessera/actions/runs/123",
    });
  });
}
