// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { marker } from "../bench/continuous/marker.ts";

const cli = fileURLToPath(new URL("./index.ts", import.meta.url));

/** Exercise the real command and gh boundary with paginated API fixtures. */
async function fixture(t: { after: (cleanup: () => Promise<void>) => void }, responses: object) {
  const root = await mkdtemp(join(tmpdir(), "tessera-status-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const bin = join(root, "bin");
  await mkdir(bin);
  await writeFile(join(root, "responses.json"), JSON.stringify(responses));
  await writeFile(
    join(bin, "gh"),
    `#!/usr/bin/env node
// SPDX-License-Identifier: BSD-2-Clause
const fs = require("node:fs");
const path = require("node:path");
const root = process.env.STATUS_FIXTURE;
const args = process.argv.slice(2);
if (args[0] === "repo") {
  console.log("owner/project");
} else {
  const route = args.at(-1).replace("repos/owner/project/", "");
  fs.appendFileSync(path.join(root, "requests"), route + "\\n");
  const responses = JSON.parse(fs.readFileSync(path.join(root, "responses.json"), "utf8"));
  if (!(route in responses)) throw new Error("Unexpected API route: " + route);
  if (args.includes("--paginate") && !args.includes("--slurp")) throw new Error("Pages not slurped");
  console.log(JSON.stringify(responses[route]));
}
`,
    { mode: 0o755 },
  );
  return {
    run: (...args: string[]) =>
      execFileSync(process.execPath, [cli, ...args], {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
        env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, STATUS_FIXTURE: root },
      }),
    requests: async () => (await readFile(join(root, "requests"), "utf8")).split("\n"),
  };
}

function run(id: number, overrides: object = {}) {
  return {
    id,
    workflow_id: 1,
    name: "CI",
    event: "push",
    head_sha: "current",
    head_branch: "main",
    run_attempt: 1,
    status: "completed",
    conclusion: "success",
    html_url: `https://github.com/owner/project/actions/runs/${id}`,
    ...overrides,
  };
}

function check(id: number, overrides: object = {}) {
  return {
    id,
    name: "Benchmarks (rust, Default)",
    status: "completed",
    conclusion: "success",
    details_url: "https://github.com/owner/project/actions/runs/70",
    output: { title: "no alerts", summary: "Compared cases" },
    ...overrides,
  };
}

test("status pins remote refs and reads all feedback pages", async (t) => {
  const f = await fixture(t, {
    "commits/main": { sha: "current", html_url: "commit-url" },
    "commits/current/check-runs?per_page=100": [
      { check_runs: [check(1)] },
      {
        check_runs: [
          check(2, {
            conclusion: "failure",
            output: {
              title: "Measurement failed",
              summary:
                "The job did not measure. See [workflow run](https://github.com/owner/project/actions/runs/70).",
            },
          }),
        ],
      },
    ],
    "actions/runs?head_sha=current&per_page=100": [
      {
        workflow_runs: [
          run(10),
          run(90, { head_sha: "old" }),
          run(92, { event: "workflow_run", workflow_id: 2 }),
        ],
      },
      { workflow_runs: [run(11, { status: "in_progress", conclusion: null, run_attempt: 2 })] },
    ],
    "actions/runs/70": run(70, {
      workflow_id: 2,
      name: "Track benchmarks",
      event: "workflow_run",
      head_sha: "trusted",
      conclusion: "failure",
    }),
    "actions/runs/11/jobs?per_page=100": [
      {
        jobs: [
          {
            name: "miri",
            status: "in_progress",
            conclusion: null,
            html_url: "job-url",
            steps: [{ name: "Install tools", status: "in_progress", conclusion: null }],
          },
        ],
      },
    ],
    "actions/runs/70/jobs?per_page=100": [
      {
        jobs: [
          {
            name: "rust",
            status: "completed",
            conclusion: "failure",
            html_url: "failed-job",
            steps: [{ name: "Install Bencher", status: "completed", conclusion: "failure" }],
          },
        ],
      },
    ],
  });
  const status = JSON.parse(f.run("status"));
  assert.equal(status.target.head, "current");
  assert.deepEqual(
    status.workflows.map((w: { id: number }) => w.id),
    [11, 70],
  );
  assert.equal(status.benchmarks[0].state, "failure");
  assert.equal(status.benchmarks[0].title, "Measurement failed");
  assert.equal(status.benchmarks[1].state, "missing");
  assert.equal(status.report, null);
  assert.equal(status.details, undefined);
  assert.ok(JSON.stringify(status).length < 2000);
  const text = f.run("status", "main", "--text");
  assert.match(text, /Install tools: in_progress/);
  assert.match(text, /Install Bencher: failure/);
  assert.match(text, /Measurement failed/);
  assert.match(text, /portable-simd: missing/);
  assert.ok(!(await f.requests()).some((path) => path.includes("/92")));
});

test("PR status joins merge workflows without accepting stale comments", async (t) => {
  const comment = (sha: string, login = "github-actions[bot]") => ({
    body: marker("rust", sha),
    html_url: sha,
    user: { login },
  });
  const f = await fixture(t, {
    "pulls/49": {
      number: 49,
      state: "open",
      head: { sha: "current" },
      merge_commit_sha: "merge",
      html_url: "pr-url",
    },
    "commits/current/check-runs?per_page=100": [
      {
        check_runs: [
          check(1, {
            conclusion: "failure",
            external_id: "70",
            details_url: "https://github.com/owner/project/runs/1",
            output: { title: "2 slower", summary: "Alert, not a measurement failure" },
          }),
        ],
      },
    ],
    "actions/runs?head_sha=current&per_page=100": [{ workflow_runs: [] }],
    "actions/runs?head_sha=merge&per_page=100": [
      {
        workflow_runs: [
          run(15, { head_sha: "merge", event: "pull_request", head_branch: "feature" }),
        ],
      },
    ],
    "actions/runs/70": run(70, {
      workflow_id: 2,
      name: "Track benchmarks",
      event: "workflow_run",
      head_sha: "trusted",
    }),
    "issues/49/comments?per_page=100": [
      [comment("old"), comment("current", "untrusted")],
      [comment("current")],
    ],
  });
  const status = JSON.parse(f.run("status", "--pr", "49", "--details"));
  assert.equal(status.target.head, "current");
  assert.equal(status.target.merge, "merge");
  assert.equal(status.details.workflows[0].head, "merge");
  assert.equal(status.workflows[1].state, "success");
  assert.equal(status.benchmarks[0].state, "failure");
  assert.equal(status.benchmarks[0].title, "2 slower");
  assert.equal(status.report, "current");
  assert.equal(status.details.comment.body, marker("rust", "current"));
});

test("missing workflows never become a clean verdict", async (t) => {
  const f = await fixture(t, {
    "commits/feature%2Ftopic": { sha: "new", html_url: "new-url" },
    "commits/new/check-runs?per_page=100": [{ check_runs: [] }],
    "actions/runs?head_sha=new&per_page=100": [{ workflow_runs: [] }],
  });
  const status = JSON.parse(f.run("status", "feature/topic"));
  assert.deepEqual(status.workflows, []);
  assert.ok(status.benchmarks.every((c: { state: string }) => c.state === "missing"));
  assert.match(f.run("status", "feature/topic", "--text"), /No workflow runs attached/);
});

test("merged PR status does not substitute main for the PR head", async (t) => {
  const f = await fixture(t, {
    "pulls/49": {
      number: 49,
      state: "closed",
      head: { sha: "current" },
      merge_commit_sha: "squash",
      html_url: "pr-url",
    },
    "commits/current/check-runs?per_page=100": [{ check_runs: [] }],
    "actions/runs?head_sha=current&per_page=100": [{ workflow_runs: [] }],
    "issues/49/comments?per_page=100": [[]],
  });
  assert.equal(JSON.parse(f.run("status", "--pr", "49", "--details")).target.merge, null);
  assert.ok(!(await f.requests()).some((path) => path.includes("squash")));
});

test("API failures fail the command instead of returning empty success", async (t) => {
  const f = await fixture(t, {});
  assert.throws(() => f.run("status", "missing"));
});
