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
    `#!/usr/bin/env bun
// SPDX-License-Identifier: BSD-2-Clause
const fs = require("node:fs");
const path = require("node:path");
const root = process.env.STATUS_FIXTURE;
const args = process.argv.slice(2);
if (args[0] === "repo") {
  console.log("owner/project");
} else {
  const route = args.at(-1).replace("repos/owner/project/", "");
  const raw = args.includes("Accept: application/vnd.github.raw");
  fs.appendFileSync(path.join(root, "requests"), (raw ? "raw " : "json ") + route + "\\n");
  const responses = JSON.parse(fs.readFileSync(path.join(root, "responses.json"), "utf8"));
  if (!(route in responses)) throw new Error("Unexpected API route: " + route);
  if (args.includes("--paginate") && !args.includes("--slurp")) throw new Error("Pages not slurped");
  if (raw) process.stdout.write(String(responses[route]));
  else console.log(JSON.stringify(responses[route]));
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
    name: "Benchmarks (rust)",
    status: "completed",
    conclusion: "success",
    details_url: "https://github.com/owner/project/actions/runs/70",
    output: { title: "no alerts", summary: "Compared cases" },
    ...overrides,
  };
}

function commit(sha: string, html_url = "commit-url") {
  return { sha, html_url, commit: { committer: { date: "2026-11-15T00:00:00Z" } } };
}

test("status pins remote refs and reads all feedback pages", async (t) => {
  const f = await fixture(t, {
    "git/matching-refs/heads/benchmark-data": [],
    "commits/main": commit("current"),
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
          run(93, {
            event: "workflow_dispatch",
            workflow_id: 2,
            path: ".github/workflows/bench-diagnostics.yml@refs/heads/main",
          }),
        ],
      },
      { workflow_runs: [run(11, { status: "in_progress", conclusion: null, run_attempt: 2 })] },
    ],
    "actions/runs/93/jobs?per_page=100": [{ jobs: [] }],
    "actions/runs/11/jobs?per_page=100": [{ jobs: [] }],
    "actions/runs/70/jobs?per_page=100": [{ jobs: [] }],
    "actions/runs/70": run(70, {
      workflow_id: 2,
      name: "Legacy tracker",
      event: "workflow_run",
      head_sha: "trusted",
      conclusion: "failure",
    }),
  });
  const status = JSON.parse(f.run("status"));
  assert.equal(status.target.head, "current");
  assert.deepEqual(
    status.workflows.map((w: { id: number }) => w.id),
    [93, 11],
  );
  assert.equal(status.benchmark.state, "failure");
  assert.equal(status.benchmark.title, "Measurement failed");
  assert.equal(status.report, null);
  assert.equal(status.details, undefined);
  assert.ok(JSON.stringify(status).length < 2000);
  assert.ok(!(await f.requests()).some((path) => /\/(70|92)(\/|$)/.test(path)));
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
      name: "Legacy tracker",
      event: "workflow_dispatch",
      path: ".github/workflows/bench-track.yml",
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
  assert.equal(status.workflows.length, 1);
  assert.equal(status.workflows[0].state, "success");
  assert.equal(status.benchmark.state, "failure");
  assert.equal(status.benchmark.title, "2 slower");
  assert.equal(status.report, "current");
  assert.equal(status.details.comment.body, marker("rust", "current"));
});

test("missing workflows never become a clean verdict", async (t) => {
  const f = await fixture(t, {
    "commits/feature%2Ftopic": commit("new", "new-url"),
    "commits/new/check-runs?per_page=100": [{ check_runs: [] }],
    "actions/runs?head_sha=new&per_page=100": [{ workflow_runs: [] }],
  });
  const status = JSON.parse(f.run("status", "feature/topic"));
  assert.deepEqual(status.workflows, []);
  assert.equal(status.benchmark.state, "missing");
  assert.match(f.run("status", "feature/topic", "--text"), /No workflow runs attached/);
});

test("main has no automatic benchmark, while an unmeasured PR is still missing", async (t) => {
  const responses = {
    "git/matching-refs/heads/benchmark-data": [],
    "commits/main": commit("current"),
    "pulls/49": {
      number: 49,
      state: "open",
      head: { sha: "current" },
      merge_commit_sha: null,
      html_url: "pr-url",
    },
    "commits/current/check-runs?per_page=100": [{ check_runs: [] }],
    "actions/runs?head_sha=current&per_page=100": [{ workflow_runs: [] }],
    "issues/49/comments?per_page=100": [[]],
  };
  const f = await fixture(t, responses);
  assert.equal(JSON.parse(f.run("status", "main")).benchmark.state, "not_applicable");
  assert.equal(JSON.parse(f.run("status", "--pr", "49")).benchmark.state, "missing");
});

test("API failures fail the command instead of returning empty success", async (t) => {
  const f = await fixture(t, {});
  assert.throws(() => f.run("status", "missing"));
});

test("main scans Go journals from the head month onward", async (t) => {
  const head = "a".repeat(40);
  const stale = "b".repeat(40);
  const encoded = (data: string) => ({ content: Buffer.from(data).toString("base64") });
  const tracked = (hash: string, execution: string, time = "2026-11-20T00:00:00Z") => ({
    hash,
    execution,
    time,
    commit_time: hash === head ? commit(head).commit.committer.date : "2026-10-01T00:00:00Z",
    id: "00000000-0000-4000-8000-000000000001",
    spec: "00000000-0000-4000-8000-000000000002",
    image: `registry.bencher.dev/p/i@sha256:${"0".repeat(64)}`,
    workflow: "https://github.com/owner/project/actions/runs/77",
    go: { GoVersion: "go1.27.4", Settings: [], Deps: [] },
    values: { [`go/argsort/u64/random/32/s1/${execution}`]: [100, 90, 110] },
  });
  const journal = (month: string) => `contents/go-tessera/${month}.jsonl?ref=benchmark-data`;
  const prior = [tracked(stale, "serial"), tracked(stale, "t4")];
  const currentSerial = tracked(head, "serial");
  const currentT4 = tracked(head, "t4");
  const late = tracked(head, "t4", "2026-12-03T00:00:00Z");
  for (const scenario of [
    {
      name: "missing",
      state: "unmeasured",
      november: prior,
      december: [],
      serial: stale,
      t4: stale,
    },
    {
      name: "partial",
      state: "partial",
      november: [...prior, { ...currentSerial, padding: "x".repeat(1_100_000) }],
      december: [],
      serial: head,
      t4: stale,
    },
    {
      name: "complete",
      state: "measured",
      november: [...prior, currentSerial, currentT4],
      december: [tracked(stale, "serial", "2026-12-01T00:00:00Z")],
      serial: head,
      t4: head,
    },
    {
      name: "delayed into a later month",
      state: "measured",
      november: [...prior, currentSerial],
      december: [late],
      serial: head,
      t4: head,
    },
  ]) {
    await t.test(scenario.name, async (t) => {
      const months = ["2026-10", "2026-11"];
      if (scenario.december.length) months.push("2026-12");
      const f = await fixture(t, {
        "commits/main": commit(head),
        [`commits/${head}/check-runs?per_page=100`]: [{ check_runs: [] }],
        [`actions/runs?head_sha=${head}&per_page=100`]: [{ workflow_runs: [] }],
        "git/matching-refs/heads/benchmark-data": [{ ref: "refs/heads/benchmark-data" }],
        "contents?ref=benchmark-data": [{ name: "go-tessera" }],
        "contents/go-tessera/index.json?ref=benchmark-data": encoded(
          JSON.stringify({
            months,
            cases: [],
            facets: { type: [], input: [], length: [], group: [] },
          }),
        ),
        [journal("2026-11")]:
          `${scenario.november.map((entry) => JSON.stringify(entry)).join("\n")}\n`,
        [journal("2026-12")]:
          `${scenario.december.map((entry) => JSON.stringify(entry)).join("\n")}\n`,
        // The indexed October journal has no API response.
      });
      const { goTracking } = JSON.parse(f.run("status", "main", "--details"));
      assert.equal(goTracking.state, scenario.state);
      for (const execution of ["serial", "t4"] as const) {
        const result = goTracking.runs.find(
          (run: { execution: string }) => run.execution === execution,
        );
        assert.equal(result.hash, scenario[execution]);
        assert.deepEqual(result.values, {
          [`go/argsort/u64/random/32/s1/${execution}`]: [100, 90, 110],
        });
      }
      assert.deepEqual(
        (await f.requests()).filter((request) => request.endsWith(".jsonl?ref=benchmark-data")),
        months
          .slice(1)
          .reverse()
          .map((month) => `raw ${journal(month)}`),
      );
    });
  }
});
