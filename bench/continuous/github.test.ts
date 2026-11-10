// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { test } from "node:test";
import { mockGitHub } from "./fixtures/github.ts";
import { baseHash, hash } from "./fixtures/report.ts";
import { resolveSource } from "./github.ts";

const fullName = "owner/tessera";
function sourceFixture(event = "pull_request") {
  const outputs: Record<string, string> = {};
  const pr = {
    number: 7,
    state: "open",
    head: { sha: hash, ref: "main", repo: { full_name: "fork/tessera" } },
    base: { sha: baseHash, ref: "parent-feature", repo: { full_name: fullName } },
  };
  const response = { missingBase: false, workflow: ".github/workflows/bench-build.yml" };
  const mock = mockGitHub((request) => {
    if (request.path.endsWith("/actions/workflows/123")) return { path: response.workflow };
    if (request.path.endsWith("/pulls/7")) return pr;
    if (request.path.includes("/compare/")) {
      if (response.missingBase) throw new Error("Recorded base unavailable");
      return { merge_base_commit: { sha: baseHash } };
    }
    if (request.path.endsWith(`/commits/${hash}`)) return { parents: [{ sha: baseHash }] };
    throw new Error(`Unexpected request: ${request.path}`);
  });
  return {
    ...mock,
    pr,
    response,
    outputs,
    context: {
      repo: { owner: "owner", repo: "tessera" },
      serverUrl: "https://github.com",
      runId: 1,
      payload: {
        workflow_run: {
          event,
          workflow_id: 123,
          head_sha: hash,
          display_title: `paired-v1 pr=7 base=${baseHash} head=${hash}`,
          head_branch: "main",
          head_repository: { full_name: fullName },
          pull_requests: [],
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

test("fork named main resolves its captured PR even without workflow_run pull_requests", async () => {
  const action = sourceFixture();
  await resolveSource(action);
  assert.deepEqual(action.outputs, {
    branch: "pr-7",
    hash,
    base: "recorded target",
    base_hash: baseHash,
    number: "7",
    eligible: "true",
  });
});

for (const scenario of ["parent rewritten", "PR retargeted", "native rerun"] as const) {
  test(`${scenario} retains the captured target, not a fresh live merge base`, async () => {
    const action = sourceFixture();
    action.pr.base.sha = "c".repeat(40);
    if (scenario === "PR retargeted") action.pr.base.ref = "other-target";
    await resolveSource(action);
    const comparison = action.requests.find((request) => request.path.includes("/compare/"))!;
    assert.ok(comparison.path.includes(`${baseHash}...${hash}`));
    assert.equal(action.outputs.base_hash, baseHash);
    assert.equal(action.requests.filter((request) => request.path.endsWith("/pulls")).length, 0);
  });
}

for (const title of [
  "",
  "old workflow",
  `paired-v1 pr=0 base=${baseHash} head=${hash}`,
  `paired-v1 pr=7 base=${baseHash} head=${"c".repeat(40)}`,
]) {
  test(`rejects missing or mismatched captured event identity: ${title.slice(0, 30)}`, async () => {
    const action = sourceFixture();
    action.context.payload.workflow_run.display_title = title;
    await assert.rejects(resolveSource(action), /triggering PR identity/);
  });
}

test("an unavailable recorded base is not replaced with the live target", async () => {
  const action = sourceFixture();
  action.response.missingBase = true;
  await assert.rejects(resolveSource(action), /Recorded base unavailable/);
});

for (const event of ["push", "workflow_dispatch"]) {
  test(`${event} on main compares the measured commit with its first parent`, async () => {
    const action = sourceFixture(event);
    await resolveSource(action);
    assert.deepEqual(action.outputs, {
      branch: "main",
      hash,
      base_hash: baseHash,
      eligible: "true",
    });
  });
  for (const field of ["head_branch", "head_repository"]) {
    test(`${event} rejects foreign or non-main source: ${field}`, async () => {
      const action = sourceFixture(event);
      if (field === "head_branch") action.context.payload.workflow_run.head_branch = "feature";
      else action.context.payload.workflow_run.head_repository.full_name = "fork/tessera";
      await assert.rejects(resolveSource(action), /Only main/);
    });
  }
}

for (const scenario of ["closed", "superseded", "foreign"] as const) {
  test(`skips ${scenario} PR`, async () => {
    const action = sourceFixture();
    if (scenario === "closed") action.pr.state = "closed";
    if (scenario === "superseded") action.pr.head.sha = "c".repeat(40);
    if (scenario === "foreign") action.pr.base.repo.full_name = "elsewhere/tessera";
    await resolveSource(action);
    assert.deepEqual(action.outputs, {});
  });
}

test("rejects another workflow with the same display name and unsupported events", async () => {
  const action = sourceFixture();
  action.response.workflow = ".github/workflows/other.yml";
  await assert.rejects(resolveSource(action), /Unexpected source workflow/);
  await assert.rejects(resolveSource(sourceFixture("release")), /Unexpected source event/);
});
