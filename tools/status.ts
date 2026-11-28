// SPDX-License-Identifier: BSD-2-Clause

import { parseCriterionId } from "../bench/catalog.ts";
import { benchmarkCheck, marker } from "../bench/continuous/marker.ts";
import { indexSchema, parseRuns } from "../bench/tracking/result.ts";
import { command } from "./process.ts";

interface Check {
  id: number;
  name: string;
  status: string;
  conclusion: string | null;
  external_id?: string;
  details_url: string;
  output: { title: string | null; summary: string | null };
}

interface Comment {
  body: string;
  html_url: string;
  user: { login: string } | null;
}

interface WorkflowRun {
  id: number;
  workflow_id: number;
  name: string;
  path?: string;
  event: string;
  head_sha: string;
  head_branch: string | null;
  run_attempt: number;
  status: string;
  conclusion: string | null;
  html_url: string;
}

interface Step {
  name: string;
  status: string;
  conclusion: string | null;
}

interface Job extends Step {
  html_url: string;
  steps: Step[];
}

export interface RepositoryStatus {
  repository: string;
  target: { ref: string; head: string; merge: string | null };
  workflows: {
    id: number;
    name: string;
    state: string;
    attempt: number;
    jobs: { name: string; state: string; steps: { name: string; state: string }[] }[];
  }[];
  benchmark: { state: string; title: string | null };
  report: string | null;
  goBenchmark?: { state: string; title: string | null; report: string | null };
  goTracking?: {
    state: string;
    runs: {
      hash: string;
      execution: string;
      operations: string[];
      workflow: string;
      values?: Record<string, [number, number, number]>;
    }[];
  };
  details?: {
    url: string;
    checks: Check[];
    workflows: { id: number; head: string; event: string; url: string; jobs: Job[] }[];
    comment: { url: string; body: string } | null;
  };
}

/** Scope authenticated and paginated reads to the checkout's remote repository. */
async function repositoryGitHub(root: string) {
  const repo = await command(
    "gh",
    ["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
    { cwd: root, capture: true },
  );
  const read = (path: string, args: string[] = [], capture: true | "raw" = true) =>
    command("gh", ["api", ...args, `repos/${repo}/${path}`], { cwd: root, capture });
  return {
    repo,
    get: async <T>(path: string): Promise<T> => JSON.parse(await read(path)),
    getRaw: (path: string) => read(path, ["-H", "Accept: application/vnd.github.raw"], "raw"),
    pages: async <T>(path: string): Promise<T[]> =>
      JSON.parse(await read(path, ["--paginate", "--slurp"])),
  };
}

/** Keep the latest run of each workflow/event/branch, including reruns. */
function latestRuns(runs: WorkflowRun[]) {
  const latest = new Map<string, WorkflowRun>();
  for (const run of runs.toSorted((a, b) => b.id - a.id || b.run_attempt - a.run_attempt)) {
    const key = `${run.workflow_id}:${run.event}:${run.head_branch}`;
    if (!latest.has(key)) latest.set(key, run);
  }
  return [...latest.values()].sort((a, b) => a.name.localeCompare(b.name));
}

function hasUnfinishedOrFailedResult(item: Step) {
  return item.conclusion
    ? !["success", "skipped", "neutral"].includes(item.conclusion)
    : item.status !== "completed";
}

/**
 * Read one remote commit's CI and benchmark feedback, with compact output by default.
 *
 * PR workflows may use the test merge commit. Benchmark checks attach to the
 * measured head; all current jobs belong to the ordinary CI run.
 * Missing feedback is not a pass; API failures reject the entire read.
 */
export async function readStatus(
  target: { ref: string } | { pr: string },
  root: string,
  details = false,
): Promise<RepositoryStatus> {
  if ("pr" in target && !/^[1-9]\d*$/.test(target.pr))
    throw new Error("PR number must be a positive integer");
  const api = await repositoryGitHub(root);
  let pull:
    | {
        number: number;
        state: string;
        head: { sha: string };
        merge_commit_sha: string | null;
        html_url: string;
      }
    | undefined;
  let resolved: RepositoryStatus["target"];
  let url: string;
  let headCommitDate = "";
  if ("pr" in target) {
    pull = await api.get(`pulls/${target.pr}`);
    if (!pull) throw new Error("PR response is missing");
    resolved = {
      ref: `PR #${pull.number}`,
      head: pull.head.sha,
      merge: pull.state === "open" ? pull.merge_commit_sha : null,
    };
    url = pull.html_url;
  } else {
    const commit = await api.get<{
      sha: string;
      html_url: string;
      commit: { committer: { date: string } };
    }>(`commits/${encodeURIComponent(target.ref)}`);
    resolved = { ref: target.ref, head: commit.sha, merge: null };
    url = commit.html_url;
    headCommitDate = commit.commit.committer.date;
  }
  const heads = [...new Set([resolved.head, resolved.merge].filter((sha) => sha !== null))];
  const [checkPages, runPages, commentPages] = await Promise.all([
    api.pages<{ check_runs: Check[] }>(`commits/${resolved.head}/check-runs?per_page=100`),
    Promise.all(
      heads.map((sha) =>
        api.pages<{ workflow_runs: WorkflowRun[] }>(`actions/runs?head_sha=${sha}&per_page=100`),
      ),
    ),
    pull ? api.pages<Comment[]>(`issues/${pull.number}/comments?per_page=100`) : [],
  ]);
  const candidates = checkPages.flatMap((page) => page.check_runs).sort((a, b) => b.id - a.id);
  const currentRuns = runPages
    .flat()
    .flatMap((page) => page.workflow_runs)
    .filter((run) => heads.includes(run.head_sha) && run.event !== "workflow_run");
  const runs = latestRuns(currentRuns);
  const workflows = await Promise.all(
    runs.map(async (run) => {
      const pages =
        run.conclusion === "success" || run.conclusion === "skipped"
          ? []
          : await api.pages<{ jobs: Job[] }>(`actions/runs/${run.id}/jobs?per_page=100`);
      return { run, jobs: pages.flatMap((page) => page.jobs).filter(hasUnfinishedOrFailedResult) };
    }),
  );
  const comments = commentPages.flat();
  const feedback = (language: "rust" | "go") => ({
    check: candidates.find((candidate) => candidate.name === benchmarkCheck(language)),
    comment: comments.findLast(
      (entry) =>
        entry.user?.login === "github-actions[bot]" &&
        entry.body.includes(marker(language, resolved.head)),
    ),
  });
  const { check, comment: current } = feedback("rust");
  const { check: goCheck, comment: goComment } = feedback("go");
  const comment = current ? { url: current.html_url, body: current.body } : null;
  const tracking =
    !pull && resolved.ref === "main"
      ? await readGoTracking(api, resolved.head, headCommitDate, details)
      : undefined;
  return {
    repository: api.repo,
    target: resolved,
    workflows: workflows.map(({ run, jobs }) => ({
      id: run.id,
      name: run.name,
      state: run.conclusion ?? run.status,
      attempt: run.run_attempt,
      jobs: jobs.map((job) => ({
        name: job.name,
        state: job.conclusion ?? job.status,
        steps: (job.steps ?? [])
          .filter(
            (step) =>
              step.status === "in_progress" ||
              (step.status === "completed" && hasUnfinishedOrFailedResult(step)),
          )
          .map((step) => ({
            name: step.name,
            state: step.conclusion ?? step.status,
          })),
      })),
    })),
    benchmark: {
      state: check
        ? (check.conclusion ?? check.status)
        : resolved.ref === "main"
          ? "not_applicable"
          : "missing",
      title: check?.output.title ?? null,
    },
    report: comment?.url ?? null,
    ...(pull
      ? {
          goBenchmark: {
            state: goCheck?.conclusion ?? goCheck?.status ?? "missing",
            title: goCheck?.output.title ?? null,
            report: goComment?.html_url ?? goCheck?.details_url ?? null,
          },
        }
      : {}),
    ...(tracking ? { goTracking: tracking } : {}),
    ...(details
      ? {
          details: {
            url,
            checks: [check, goCheck]
              .filter((entry): entry is Check => entry !== undefined)
              .map(({ id, name, status, conclusion, external_id, details_url, output }) => ({
                id,
                name,
                status,
                conclusion,
                external_id,
                details_url,
                output,
              })),
            workflows: workflows.map(({ run, jobs }) => ({
              id: run.id,
              head: run.head_sha,
              event: run.event,
              url: run.html_url,
              jobs: jobs.map(({ name, status, conclusion, html_url, steps }) => ({
                name,
                status,
                conclusion,
                html_url,
                steps: (steps ?? []).map(({ name, status, conclusion }) => ({
                  name,
                  status,
                  conclusion,
                })),
              })),
            })),
            comment,
          },
        }
      : {}),
  };
}

async function readGoTracking(
  api: Awaited<ReturnType<typeof repositoryGitHub>>,
  head: string,
  headCommitDate: string,
  details: boolean,
) {
  const path = "go-tessera";
  const decode = ({ content }: { content: string }) =>
    Buffer.from(content, "base64").toString("utf8");
  const refs = await api.get<{ ref: string }[]>("git/matching-refs/heads/benchmark-data");
  const ref = "benchmark-data";
  if (!refs.some((entry) => entry.ref === `refs/heads/${ref}`))
    return { state: "unmeasured", runs: [] };
  const files = await api.get<{ name: string }[]>(`contents?ref=${ref}`);
  if (!files.some((file) => file.name === path)) return { state: "unmeasured", runs: [] };
  const index = indexSchema.parse(
    JSON.parse(decode(await api.get(`contents/${path}/index.json?ref=benchmark-data`))),
  );
  const headRuns = new Map<string, ReturnType<typeof parseRuns>[number]>();
  const latest = new Map(headRuns);
  const executions = ["serial", "t4"];
  const firstMonth = new Date(headCommitDate).toISOString().slice(0, 7);
  for (const month of index.months.toReversed()) {
    const canContainHead = month >= firstMonth;
    if (!canContainHead && executions.every((execution) => latest.has(execution))) break;
    let monthRuns: ReturnType<typeof parseRuns>;
    try {
      monthRuns = parseRuns(await api.getRaw(`contents/${path}/${month}.jsonl?ref=${ref}`));
    } catch (error) {
      if (canContainHead) throw error;
      break;
    }
    for (const run of monthRuns.toReversed()) {
      if (!latest.has(run.execution)) latest.set(run.execution, run);
      if (canContainHead && run.hash === head && !headRuns.has(run.execution))
        headRuns.set(run.execution, run);
    }
    if (executions.every((execution) => headRuns.has(execution))) break;
  }
  const runs = [...latest].map(([execution, prior]) => {
    const { hash, values, workflow } = headRuns.get(execution) ?? prior;
    return {
      hash,
      execution,
      workflow,
      ...(details ? { values } : {}),
      operations: [
        ...new Set(Object.keys(values).map((id) => parseCriterionId(id).operation.kind)),
      ],
    };
  });
  return {
    state: executions.every((execution) => headRuns.has(execution))
      ? "measured"
      : headRuns.size
        ? "partial"
        : "unmeasured",
    runs,
  };
}

/** Print a small optional overview without expanding report bodies. */
export function formatStatus(status: RepositoryStatus) {
  const lines = [`${status.target.ref} · ${status.target.head}`];
  if (!status.workflows.length) lines.push("No workflow runs attached to this target yet.");
  for (const run of status.workflows) {
    lines.push(`${run.name}: ${run.state} (run=${run.id}, attempt=${run.attempt})`);
    for (const job of run.jobs) {
      lines.push(`  ${job.name}: ${job.state}`);
      for (const step of job.steps) lines.push(`    ${step.name}: ${step.state}`);
    }
  }
  const { state, title } = status.benchmark;
  lines.push(`Benchmark: ${state}${title ? ` · ${title}` : ""}`);
  if (status.goBenchmark)
    lines.push(
      `Benchmarks (go): ${status.goBenchmark.state}${status.goBenchmark.title ? ` · ${status.goBenchmark.title}` : ""}`,
    );
  if (status.goTracking) {
    lines.push(`Go tracking: ${status.goTracking.state}`);
    for (const run of status.goTracking.runs)
      lines.push(
        `  ${run.execution} · ${run.hash} · ${run.operations.join(", ")} · ${run.workflow}`,
      );
  }
  return lines.join("\n");
}
