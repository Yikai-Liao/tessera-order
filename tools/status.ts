// SPDX-License-Identifier: BSD-2-Clause

import { VARIANTS } from "../bench/catalog.ts";
import { BENCHMARK_CHECK, marker } from "../bench/continuous/marker.ts";
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
  benchmarks: { configuration: string; state: string; title: string | null }[];
  report: string | null;
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
  const read = async (path: string, paginate = false) =>
    JSON.parse(
      await command(
        "gh",
        ["api", ...(paginate ? ["--paginate", "--slurp"] : []), `repos/${repo}/${path}`],
        { cwd: root, capture: true },
      ),
    );
  return {
    repo,
    get: <T>(path: string): Promise<T> => read(path),
    pages: <T>(path: string): Promise<T[]> => read(path, true),
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
    const commit = await api.get<{ sha: string; html_url: string }>(
      `commits/${encodeURIComponent(target.ref)}`,
    );
    resolved = { ref: target.ref, head: commit.sha, merge: null };
    url = commit.html_url;
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
  const checks = VARIANTS.map(() => candidates.find((check) => check.name === BENCHMARK_CHECK));
  const attached = checks.filter((check) => check !== undefined);
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
  const current = commentPages
    .flat()
    .filter(
      (comment) =>
        comment.user?.login === "github-actions[bot]" &&
        comment.body.includes(marker("rust", resolved.head)),
    )
    .at(-1);
  const comment = current ? { url: current.html_url, body: current.body } : null;
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
    benchmarks: VARIANTS.map((variant, index) => ({
      configuration: variant.id,
      state: checks[index] ? (checks[index].conclusion ?? checks[index].status) : "missing",
      title: checks[index]?.output.title ?? null,
    })),
    report: comment?.url ?? null,
    ...(details
      ? {
          details: {
            url,
            checks: attached.map(
              ({ id, name, status, conclusion, external_id, details_url, output }) => ({
                id,
                name,
                status,
                conclusion,
                external_id,
                details_url,
                output,
              }),
            ),
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
  for (const check of status.benchmarks)
    lines.push(
      `Benchmark ${check.configuration}: ${check.state}${check.title ? ` · ${check.title}` : ""}`,
    );
  return lines.join("\n");
}
