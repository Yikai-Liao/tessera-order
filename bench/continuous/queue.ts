// SPDX-License-Identifier: BSD-2-Clause

import { setTimeout } from "node:timers/promises";
import { VARIANTS } from "../catalog.ts";
import { type ActionArguments, resolveSource } from "./github.ts";
import { type BencherReport, summarize } from "./report.ts";

const BUILD = "bench-build.yml";
const TRACK = "bench-track.yml";

/** Native build records are durable tickets; tracker run names identify attempts. */
export interface QueueBuild {
  id: number;
  head_sha: string;
  event: string;
  head_branch: string | null;
  created_at: string;
  status: string | null;
  conclusion: string | null;
}

interface Tracker {
  id: number;
  event: string;
  display_title: string;
  status: string | null;
}

type Source = Record<string, string>;

/** Resolve through the existing trusted workflow/PR identity boundary. */
async function sourceFor(args: ActionArguments, id: number) {
  const { data: run } = await args.github.rest.actions.getWorkflowRun({
    ...args.context.repo,
    run_id: id,
  });
  if (run.status !== "completed" || run.conclusion !== "success") return {};
  const source: Source = {};
  await resolveSource({
    ...args,
    context: { ...args.context, payload: { workflow_run: run } },
    core: {
      ...args.core,
      setOutput: (name, value) => {
        source[name] = value;
      },
    },
  });
  return source;
}

/** A measured alert is a usable sample; a green workflow alone is not proof. */
export async function baselineReady(args: ActionArguments, branch: string, hash: string) {
  const { github, context } = args;
  const checks = await github.paginate(github.rest.checks.listForRef, {
    ...context.repo,
    ref: hash,
    filter: "all",
    per_page: 100,
  });
  for (const variant of VARIANTS) {
    let found = false;
    for (const check of checks) {
      if (
        check.name !== `Benchmarks (rust, ${variant.name})` ||
        check.app?.slug !== "github-actions" ||
        check.status !== "completed" ||
        !/^[1-9]\d*$/.test(check.external_id ?? "")
      )
        continue;
      const { data: tracker } = await github.rest.actions.getWorkflowRun({
        ...context.repo,
        run_id: Number(check.external_id),
      });
      if (tracker.path !== ".github/workflows/bench-track.yml") continue;
      // Read the existing report link, not a second measurement-receipt format.
      const url = check.output.summary?.match(
        /\[JSON\]\((https:\/\/api\.bencher\.dev\/v0\/projects\/[^/()]+\/reports\/[a-f0-9-]+)\)/,
      )?.[1];
      if (!url) continue;
      const response = await fetch(url, { signal: AbortSignal.timeout(10_000) });
      if (!response.ok) throw new Error(`Baseline report returned HTTP ${response.status}`);
      const report = (await response.json()) as BencherReport;
      if (
        report.branch.name === branch &&
        report.branch.head.version?.hash === hash &&
        report.testbed.name === variant.testbed &&
        summarize(report).rows.length
      ) {
        found = true;
        break;
      }
    }
    if (!found) return false;
  }
  return true;
}

/** Select P0 dependencies, then ready PRs, with one main turn after three PRs. */
export function chooseBuild(
  main: QueueBuild | undefined,
  pulls: { build: QueueBuild; ready: boolean; needsMain: boolean }[],
  prStreak: number,
) {
  if (main?.conclusion === "success" && (pulls.some((pull) => pull.needsMain) || prStreak >= 3))
    return main;
  return (
    pulls.find((pull) => pull.ready)?.build ?? (main?.conclusion === "success" ? main : undefined)
  );
}

/** Reconstruct pending work from Actions; never create a second writable queue. */
async function nextBuild(
  args: ActionArguments,
  since: string,
  currentRun?: number,
  requested?: number,
) {
  if (
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(since) ||
    !Number.isFinite(Date.parse(since))
  ) {
    throw new Error("Set BENCHMARK_QUEUE_SINCE to an ISO activation time");
  }
  const { github, context, core } = args;
  const repo = context.repo;
  const trackers = (await github.paginate(github.rest.actions.listWorkflowRuns, {
    ...repo,
    workflow_id: TRACK,
    per_page: 100,
  })) as Tracker[];
  if (!currentRun && trackers.some((run) => run.status !== "completed")) {
    core.notice("A tracker is active or queued; leave selection to its completion.");
    return;
  }
  const builds = (
    await github.paginate(github.rest.actions.listWorkflowRuns, {
      ...repo,
      workflow_id: BUILD,
      created: `>=${since}`,
      per_page: 100,
    })
  ).filter((run) => Date.parse(run.created_at) >= Date.parse(since));
  const attempts = new Map<number, Tracker>();
  for (const run of trackers
    .filter((run) => run.id !== currentRun && run.event === "workflow_dispatch")
    .sort((a, b) => a.id - b.id)) {
    const id = run.display_title.match(/^Track benchmarks · ([1-9]\d*)$/)?.[1];
    if (id) attempts.set(Number(id), run);
  }
  const ready = new Map<string, boolean>();
  const measured = async (branch: string, hash: string) => {
    const key = `${branch}/${hash}`;
    if (!ready.has(key)) ready.set(key, await baselineReady(args, branch, hash));
    return ready.get(key)!;
  };

  // Only pending tickets need ancestry comparisons, not the whole retained history.
  const main = builds.filter(
    (run) =>
      (run.event === "push" || run.event === "workflow_dispatch") && run.head_branch === "main",
  );
  let pendingMain: QueueBuild | undefined;
  for (const build of main.filter((run) => !attempts.has(run.id)).sort((a, b) => a.id - b.id)) {
    if (!pendingMain || build.head_sha === pendingMain.head_sha) {
      pendingMain ??= build;
      continue;
    }
    const { data } = await github.rest.repos.compareCommits({
      ...repo,
      base: pendingMain.head_sha,
      head: build.head_sha,
    });
    if (data.status === "diverged") throw new Error("Main history diverged; reconcile queue");
    if (data.status === "behind") pendingMain = build;
  }
  const lastMain = main
    .filter((run) => attempts.has(run.id))
    .sort((a, b) => attempts.get(b.id)!.id - attempts.get(a.id)!.id)[0];
  if (lastMain && !(await measured("main", lastMain.head_sha))) {
    core.notice(
      `Main build ${lastMain.id} has no complete sample; rerun its tracker before advancing main.`,
    );
    pendingMain = undefined;
  }
  if (lastMain && pendingMain && lastMain.head_sha !== pendingMain.head_sha) {
    const { data } = await github.rest.repos.compareCommits({
      ...repo,
      base: lastMain.head_sha,
      head: pendingMain.head_sha,
    });
    if (data.status !== "ahead") throw new Error("Refusing to append an older main sample");
  }
  if (pendingMain && pendingMain.conclusion !== "success")
    core.notice(
      `Main build ${pendingMain.id} is unfinished or failed; later main builds remain queued.`,
    );
  const pulls = [];
  for (const build of builds
    .filter((run) => run.event === "pull_request")
    .sort((a, b) => a.id - b.id)) {
    if (attempts.has(build.id) || build.conclusion !== "success") continue;
    const source = await sourceFor(args, build.id);
    if (!source.eligible) continue;
    // Only the latest successful build of the same head needs a fresh attempt.
    if (
      builds.some(
        (other) =>
          other.event === "pull_request" &&
          other.head_sha === build.head_sha &&
          other.id > build.id &&
          other.conclusion === "success",
      )
    )
      continue;
    const comparable =
      source.start_point && source.base_hash
        ? await measured(source.start_point, source.base_hash)
        : false;
    pulls.push({
      build,
      ready: Boolean(comparable),
      needsMain:
        !comparable &&
        source.start_point === "main" &&
        main.some((run) => run.head_sha === source.base_hash),
    });
    if (!comparable)
      core.notice(
        `Build ${build.id} waits for exact baseline ${source.start_point ?? "unresolved"}/${source.base_hash ?? "unknown"}.`,
      );
  }
  const recent = [...attempts.keys()].sort((a, b) => attempts.get(b)!.id - attempts.get(a)!.id);
  let streak = 0;
  for (const id of recent) {
    if (builds.find((build) => build.id === id)?.event !== "pull_request") break;
    streak++;
    if (streak === 3) break;
  }
  // Priority may change after dispatch. Keep an already selected, still-safe
  // ticket rather than consuming it without measuring when a newer PR arrives.
  const selected = requested
    ? pendingMain?.id === requested && pendingMain.conclusion === "success"
      ? pendingMain
      : pulls.find((pull) => pull.build.id === requested && pull.ready)?.build
    : chooseBuild(pendingMain, pulls, streak);
  core.notice(selected ? `Next measurement: build ${selected.id}.` : "No runnable measurement.");
  return selected;
}

/** Dispatch one ticket. Completion, new builds and a periodic sweep wake selection. */
export async function scheduleNext(args: ActionArguments, since: string) {
  const build = await nextBuild(args, since);
  if (!build) return;
  await args.github.rest.actions.createWorkflowDispatch({
    ...args.context.repo,
    workflow_id: TRACK,
    ref: "main",
    inputs: { build_id: String(build.id) },
  });
  // Do not release the selector lock before the native dispatch record is visible.
  for (let attempt = 0; attempt < 10; attempt++) {
    const { data } = await args.github.rest.actions.listWorkflowRuns({
      ...args.context.repo,
      workflow_id: TRACK,
      event: "workflow_dispatch",
      per_page: 100,
    });
    if (data.workflow_runs.some((run) => run.display_title === `Track benchmarks · ${build.id}`))
      return;
    await setTimeout(1000);
  }
  throw new Error("Dispatch acknowledgement not visible; inspect Actions before retrying");
}

/** Revalidate selection and baseline before any image submission or main write. */
export async function prepareScheduledSource(args: ActionArguments, since: string, id: string) {
  if (!/^[1-9]\d*$/.test(id)) throw new Error("Invalid benchmark build ID");
  const selected = await nextBuild(args, since, args.context.runId, Number(id));
  if (selected?.id !== Number(id)) {
    args.core.notice(
      "Selection changed or ticket was already attempted; no measurement submitted.",
    );
    return;
  }
  const source = await sourceFor(args, Number(id));
  if (!source.eligible) return;
  if (
    source.number &&
    (!source.start_point ||
      !source.base_hash ||
      !(await baselineReady(args, source.start_point, source.base_hash)))
  ) {
    throw new Error("Exact PR baseline is unavailable; refusing Bencher fallback");
  }
  const { data: build } = await args.github.rest.actions.getWorkflowRun({
    ...args.context.repo,
    run_id: Number(id),
  });
  args.core.setOutput("build_id", id);
  args.core.setOutput("build_attempt", String(build.run_attempt));
  for (const [name, value] of Object.entries(source)) args.core.setOutput(name, value);
}
