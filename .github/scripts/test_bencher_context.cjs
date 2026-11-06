// SPDX-License-Identifier: BSD-2-Clause

const assert = require("node:assert/strict");
const test = require("node:test");
const resolve = require("./bencher_context.cjs");

const repo = { owner: "owner", repo: "tessera" };
const fullName = "owner/tessera";
const hash = "a".repeat(40);
const pull = {
  number: 7, state: "open",
  head: { sha: hash, ref: "main", repo: { full_name: "fork/tessera" } },
  base: { sha: "b".repeat(40), ref: "main", repo: { full_name: fullName } },
};

function fixture(event = "pull_request", pulls = [pull]) {
  const outputs = {};
  return {
    outputs,
    context: {
      repo,
      payload: { workflow_run: {
        event, workflow_id: 123, head_sha: hash, head_branch: "main",
        head_repository: { full_name: fullName },
        // Fork workflow_run events can omit the PR association.
        pull_requests: [],
      } },
    },
    github: {
      rest: {
        actions: { getWorkflow: async () => ({
          data: { path: ".github/workflows/bench-build.yml" },
        }) },
        repos: { listPullRequestsAssociatedWithCommit: Symbol("listPulls") },
      },
      paginate: async () => pulls,
    },
    core: {
      setOutput: (key, value) => { outputs[key] = value; },
      notice: () => {},
    },
  };
}

test("fork named main uses PR identity, never the baseline", async () => {
  const f = fixture();
  await resolve(f);
  assert.deepEqual(f.outputs, {
    branch: "pr-7", hash, base: "main", base_hash: "b".repeat(40),
    number: "7", eligible: "true",
  });
});

for (const event of ["push", "workflow_dispatch"]) {
  test(`${event} on main updates the baseline`, async () => {
    const f = fixture(event);
    await resolve(f);
    assert.deepEqual(f.outputs, { branch: "main", hash, eligible: "true" });
  });

  for (const field of ["head_branch", "head_repository"]) {
    test(`${event} rejects non-main or foreign source: ${field}`, async () => {
      const f = fixture(event);
      f.context.payload.workflow_run[field] = field === "head_branch"
        ? "feature" : { full_name: "fork/tessera" };
      await assert.rejects(resolve(f), /Only main/);
    });
  }
}

test("rejects a different workflow with the same display name", async () => {
  const f = fixture();
  f.github.rest.actions.getWorkflow = async () => ({
    data: { path: ".github/workflows/other.yml" },
  });
  await assert.rejects(resolve(f), /Unexpected source workflow/);
});

test("rejects unexpected event types", async () => {
  await assert.rejects(resolve(fixture("release")), /Unexpected source event/);
});

for (const [name, pr] of [
  ["closed", { ...pull, state: "closed" }],
  ["superseded", { ...pull, head: { ...pull.head, sha: "c".repeat(40) } }],
  ["foreign", { ...pull, base: { ...pull.base, repo: { full_name: "foreign/repo" } } }],
]) {
  test(`skips ${name} PR`, async () => {
    const f = fixture("pull_request", [pr]);
    await resolve(f);
    assert.deepEqual(f.outputs, {});
  });
}

test("skips a commit with no PR", async () => {
  const f = fixture("pull_request", []);
  await resolve(f);
  assert.deepEqual(f.outputs, {});
});

test("rejects ambiguous PR association", async () => {
  const f = fixture("pull_request", [pull, { ...pull, number: 8 }]);
  await assert.rejects(resolve(f), /Ambiguous PR/);
});
