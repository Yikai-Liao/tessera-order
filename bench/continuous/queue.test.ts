// SPDX-License-Identifier: BSD-2-Clause

import assert from "node:assert/strict";
import childProcess from "node:child_process";
import { type TestContext, test } from "node:test";
import { VARIANTS } from "../catalog.ts";
import { mockGitHub } from "./fixtures/github.ts";
import { fixture as reportFixture } from "./fixtures/report.ts";
import {
  baselineReady,
  chooseBuild,
  prepareScheduledSource,
  type QueueBuild,
  scheduleNext,
} from "./queue.ts";

const since = "2026-11-10T00:00:00Z";
const hash = (id: number) => id.toString(16).padStart(40, "0");
function build(id: number, event = "push", conclusion: string | null = "success"): QueueBuild {
  return {
    id,
    head_sha: hash(id),
    event,
    head_branch: event === "push" ? "main" : "feature",
    created_at: since,
    status: conclusion ? "completed" : "in_progress",
    conclusion,
  };
}

test("priority keeps exact dependencies ahead, ready PRs next, and main starvation bounded", () => {
  const main = build(1);
  const pr = { build: build(2, "pull_request"), ready: true, needsMain: false };
  assert.equal(chooseBuild(main, [pr], 0), pr.build);
  assert.equal(chooseBuild(main, [pr], 3), main);
  assert.equal(chooseBuild(main, [{ ...pr, ready: false, needsMain: true }], 0), main);
  assert.equal(chooseBuild(main, [{ ...pr, ready: false }], 0), main);
  assert.equal(chooseBuild(build(1, "push", null), [pr], 3), pr.build);
  assert.equal(chooseBuild(build(1, "push", "failure"), [], 0), undefined);
});

function tracker(id: number, build: number, overrides: object = {}) {
  return {
    id,
    event: "workflow_dispatch",
    status: "completed",
    run_attempt: 1,
    head_branch: "main",
    head_repository: { full_name: "owner/tessera" },
    display_title: `Track benchmarks · ${build}`,
    ...overrides,
  };
}

function fixture(
  t: TestContext,
  options: {
    builds?: QueueBuild[];
    attempts?: number[];
    trackers?: ReturnType<typeof tracker>[];
    samples?: { owner: number; started: string; configuration?: string }[];
    attemptStarted?: string;
    history?: string[];
    busy?: boolean;
    baseline?: number;
    missing?: boolean;
    wrongReport?: boolean;
    closed?: boolean;
  } = {},
) {
  const builds = options.builds ?? [build(1), build(2, "pull_request")];
  t.mock.method(childProcess, "execFileSync", () =>
    (
      options.history ?? [
        ...new Set(builds.toSorted((a, b) => b.id - a.id).map((run) => run.head_sha)),
      ]
    ).join("\n"),
  );
  const outputs: Record<string, string> = {};
  const notices: string[] = [];
  let dispatched = "";
  const trackers =
    options.trackers ?? (options.attempts ?? []).map((id, index) => tracker(100 + index, id));
  const mock = mockGitHub((request) => {
    const path = request.path;
    if (request.method === "POST" && path.endsWith("/dispatches")) {
      dispatched = (request.body.inputs as { build_id: string }).build_id;
      return {};
    }
    if (path.endsWith("/workflows/bench-track.yml/runs"))
      return {
        total_count: trackers.length + 2,
        workflow_runs: [
          ...trackers,
          ...(options.busy ? [{ id: 999, status: "in_progress" }] : []),
          // Legacy skipped callbacks must not consume scheduled tickets.
          {
            id: 50,
            event: "workflow_run",
            status: "completed",
            display_title: "Track benchmarks · 1",
          },
          ...(dispatched
            ? [{ id: 1000, display_title: `Track benchmarks · ${dispatched}`, status: "queued" }]
            : []),
        ],
      };
    if (path.endsWith("/workflows/bench-build.yml/runs"))
      return { total_count: builds.length, workflow_runs: builds };
    if (/\/attempts\/\d+$/.test(path)) return { run_started_at: options.attemptStarted ?? since };
    const run = path.match(/\/actions\/runs\/(\d+)$/)?.[1];
    if (run) {
      const source = builds.find((build) => build.id === Number(run));
      return source
        ? {
            ...source,
            workflow_id: 123,
            run_attempt: 1,
            head_repository: { full_name: "owner/tessera" },
          }
        : { path: ".github/workflows/bench-track.yml" };
    }
    if (path.endsWith("/workflows/123")) return { path: ".github/workflows/bench-build.yml" };
    if (path.includes("/compare/")) {
      const [base, head] = path.split("/compare/")[1].split("...");
      return {
        status: base < head ? "ahead" : "behind",
        merge_base_commit: { sha: hash(options.baseline ?? 0) },
      };
    }
    if (path.endsWith("/pulls")) {
      const commit = path.match(/\/commits\/([a-f0-9]+)\//)?.[1];
      return [
        {
          number: 7,
          state: options.closed ? "closed" : "open",
          head: { sha: commit, ref: "feature", repo: { full_name: "fork/tessera" } },
          base: { sha: hash(9), ref: "main", repo: { full_name: "owner/tessera" } },
        },
      ];
    }
    if (path.endsWith("/check-runs")) {
      const commit = path.match(/\/commits\/([a-f0-9]+)\//)?.[1];
      return {
        total_count: options.missing ? 0 : VARIANTS.length,
        check_runs: options.missing
          ? []
          : VARIANTS.flatMap((variant) =>
              (options.samples ?? [{ owner: 200, started: "2026-11-10T00:01:00Z" }])
                .filter((sample) => !sample.configuration || sample.configuration === variant.id)
                .map((sample) => ({
                  name: `Benchmarks (rust, ${variant.name})`,
                  app: { slug: "github-actions" },
                  status: "completed",
                  conclusion: "failure",
                  external_id: String(sample.owner),
                  started_at: sample.started,
                  output: {
                    summary: `[JSON](https://api.bencher.dev/v0/projects/project/reports/${commit}-${variant.id === "default" ? "a" : "b"})`,
                  },
                })),
            ),
      };
    }
    throw new Error(`Unexpected request: ${request.method} ${path}`);
  });
  t.mock.method(globalThis, "fetch", async (url: string) => {
    const match = url.match(/reports\/([a-f0-9]{40})-([ab])$/)!;
    const report = reportFixture();
    report.branch.name = "main";
    report.branch.head.version = { hash: options.wrongReport ? hash(999) : match[1] };
    report.testbed.name = VARIANTS[match[2] === "a" ? 0 : 1].testbed;
    return new Response(JSON.stringify(report), { status: 200 });
  });
  return {
    ...mock,
    outputs,
    notices,
    dispatched: () => dispatched,
    context: {
      repo: { owner: "owner", repo: "tessera" },
      runId: 300,
      payload: {},
      serverUrl: "https://github.com",
    },
    core: {
      setOutput: (name: string, value: string) => {
        outputs[name] = value;
      },
      notice: (message: string) => {
        notices.push(message);
      },
    },
  };
}

test("native dispatch chooses a ready PR ahead of unrelated main without writing queue state", async (t) => {
  const f = fixture(t);
  await scheduleNext(f, since);
  assert.equal(f.dispatched(), "2");
  assert.equal(f.requests.filter((r) => r.method !== "GET").length, 1);
});

test("a missing PR baseline promotes the oldest main ancestor, not the needed descendant", async (t) => {
  const f = fixture(t, {
    builds: [build(3), build(1), build(4, "pull_request")],
    baseline: 3,
    missing: true,
  });
  await scheduleNext(f, since);
  assert.equal(f.dispatched(), "1");
});

test("failed and unfinished main builds block descendants without blocking ready PRs", async (t) => {
  for (const conclusion of [null, "failure"]) {
    const f = fixture(t, {
      builds: [build(1, "push", conclusion), build(3), build(4, "pull_request")],
    });
    await scheduleNext(f, since);
    assert.equal(f.dispatched(), "4");
  }
});

test("a failed main measurement remains a barrier rather than silently skipping history", async (t) => {
  const f = fixture(t, { builds: [build(1), build(3)], attempts: [1], missing: true });
  await scheduleNext(f, since);
  assert.equal(f.dispatched(), "");
});

test("three native PR attempts give the oldest pending main a turn", async (t) => {
  const f = fixture(t, {
    builds: [build(1), ...[2, 3, 4, 5].map((id) => build(id, "pull_request"))],
    attempts: [2, 3, 4],
  });
  await scheduleNext(f, since);
  assert.equal(f.dispatched(), "1");
});

test("active trackers exclude another dispatch, including legacy runs", async (t) => {
  const f = fixture(t, { busy: true });
  await scheduleNext(f, since);
  assert.equal(f.dispatched(), "");
});

test("closed PRs and already attempted tickets are not submitted again", async (t) => {
  const f = fixture(t, { builds: [build(2, "pull_request")], closed: true });
  await scheduleNext(f, since);
  assert.equal(f.dispatched(), "");
  const attempted = fixture(t, { builds: [build(2, "pull_request")], attempts: [2] });
  await scheduleNext(attempted, since);
  assert.equal(attempted.dispatched(), "");
});

test("measured alerts are baselines, but missing or wrong exact-head evidence is not", async (t) => {
  assert.equal(await baselineReady(fixture(t), "main", hash(0)), true);
  assert.equal(await baselineReady(fixture(t, { missing: true }), "main", hash(0)), false);
  assert.equal(await baselineReady(fixture(t, { wrongReport: true }), "main", hash(0)), false);
});

test("main completion requires both configurations from its owner and current attempt", async (t) => {
  const builds = [build(1), { ...build(2), head_sha: hash(1) }, build(3)];
  for (const samples of [
    [{ owner: 100, started: "2026-11-10T00:06:00Z" }],
    [{ owner: 101, started: "2026-11-10T00:01:00Z" }],
    [{ owner: 101, started: "2026-11-10T00:05:00Z" }],
    [
      { owner: 101, started: "2026-11-10T00:06:00Z", configuration: "default" },
      { owner: 100, started: "2026-11-10T00:06:00Z", configuration: "portable-simd" },
    ],
  ]) {
    const f = fixture(t, {
      builds,
      attempts: [1, 2],
      samples,
      attemptStarted: "2026-11-10T00:05:00Z",
    });
    await scheduleNext(f, since);
    assert.equal(f.dispatched(), "");
  }
  const f = fixture(t, {
    builds,
    attempts: [1, 2],
    samples: [{ owner: 101, started: "2026-11-10T00:06:00Z" }],
    attemptStarted: "2026-11-10T00:05:00Z",
  });
  await scheduleNext(f, since);
  assert.equal(f.dispatched(), "3");
});

test("duplicate dispatches preserve one main owner and its native recovery", async (t) => {
  for (const attempt of [1, 2]) {
    const f = fixture(t, {
      builds: [build(1)],
      trackers: [
        tracker(298, 1, { head_repository: { full_name: "fork/tessera" } }),
        tracker(299, 1, { head_branch: "feature" }),
        tracker(300, 1, { run_attempt: attempt }),
        tracker(301, 1, { status: "queued" }),
      ],
    });
    await prepareScheduledSource(f, since, "1");
    assert.equal(f.outputs.eligible, "true");
    const duplicate = { ...f, context: { ...f.context, runId: 301 } };
    delete f.outputs.eligible;
    await prepareScheduledSource(duplicate, since, "1");
    assert.equal(f.outputs.eligible, undefined);
  }
});

test("worker keeps a safe main ticket despite changed priority or queued dispatches", async (t) => {
  const f = fixture(t, {
    busy: true,
    trackers: [tracker(300, 1), tracker(301, 1, { status: "queued" })],
  });
  await prepareScheduledSource(f, since, "1");
  assert.equal(f.outputs.eligible, "true");
  assert.equal(f.outputs.build_id, "1");
  assert.equal(f.outputs.hash, hash(1));
});

test("native recovery keeps old code but uses the freshly fetched main graph", async (t) => {
  const f = fixture(t, {
    builds: [build(1), build(2)],
    trackers: [tracker(300, 1, { run_attempt: 2 })],
  });
  t.mock.method(childProcess, "execFileSync", (_command: string, args: string[]) =>
    args?.at(-1) === "HEAD" ? hash(1) : `${hash(2)}\n${hash(1)}`,
  );
  await prepareScheduledSource(f, since, "1");
  assert.equal(f.outputs.eligible, "true");
  assert.equal(f.outputs.build_id, "1");
});

test("main history, not build IDs or dispatch order, owns the prefix", async (t) => {
  const f = fixture(t, { builds: [build(1), build(3)], history: [hash(1), hash(3)] });
  await scheduleNext(f, since);
  assert.equal(f.dispatched(), "3");
  const missing = fixture(t, { builds: [build(1)], history: [hash(9)] });
  await assert.rejects(scheduleNext(missing, since), /trusted history/);
  const blocked = fixture(t, {
    builds: [build(1), build(2), build(3), build(4)],
    trackers: [tracker(100, 3), tracker(101, 2), tracker(300, 1)],
    samples: [{ owner: 300, started: "2026-11-10T00:01:00Z" }],
  });
  await scheduleNext(blocked, since);
  assert.equal(blocked.dispatched(), "");
  await prepareScheduledSource(
    { ...blocked, context: { ...blocked.context, runId: 101 } },
    since,
    "2",
  );
  assert.equal(blocked.outputs.eligible, "true");
});

test("worker refuses out-of-order main, duplicate attempts and invalid dispatch identities", async (t) => {
  const f = fixture(t, { builds: [build(1), build(3)], trackers: [tracker(300, 3)] });
  await prepareScheduledSource(f, since, "3");
  assert.equal(f.outputs.eligible, undefined);
  await scheduleNext(f, since);
  assert.equal(f.dispatched(), "1");
  const recovered = fixture(t, {
    builds: [build(1), build(3)],
    trackers: [tracker(100, 1), tracker(300, 3, { run_attempt: 2 })],
    samples: [{ owner: 100, started: "2026-11-10T00:01:00Z" }],
  });
  await prepareScheduledSource(recovered, since, "3");
  assert.equal(recovered.outputs.eligible, "true");
  const backward = fixture(t, {
    builds: [build(1), build(3)],
    trackers: [tracker(300, 3)],
    samples: [{ owner: 300, started: "2026-11-10T00:01:00Z" }],
  });
  await assert.rejects(scheduleNext(backward, since), /older main/);
  const duplicate = fixture(t, { builds: [build(1)], attempts: [1] });
  await prepareScheduledSource(duplicate, since, "1");
  assert.equal(duplicate.outputs.eligible, undefined);
  await assert.rejects(prepareScheduledSource(f, since, "bad"), /Invalid/);
  await assert.rejects(scheduleNext(f, ""), /activation/);
});
