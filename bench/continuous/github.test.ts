// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { mockGitHub } from "./fixtures/github.ts";
import { hash } from "./fixtures/report.ts";
import { resolveSource } from "./github.ts";

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
