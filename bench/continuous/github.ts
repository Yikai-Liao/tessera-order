// SPDX-License-Identifier: BSD-2-Clause

import type { context, getOctokit } from "@actions/github";
import type { Variant } from "./variants.ts";

export type GitHub = ReturnType<typeof getOctokit>;
export type ActionContext = Pick<typeof context, "repo" | "serverUrl" | "runId" | "payload">;
type Repository = ActionContext["repo"];

export interface ActionsCore {
  notice(message: string): void;
  setOutput(name: string, value: string): void;
  summary: { addRaw(text: string): { write(): Promise<unknown> } };
}

interface SourceRun {
  workflow_id: number;
  event: string;
  head_sha: string;
  head_branch: string;
  head_repository: { full_name: string };
}

interface ActionArguments {
  github: GitHub;
  context: ActionContext;
  core: Pick<ActionsCore, "notice" | "setOutput">;
}

// The branch every push to the default branch records samples under. The
// pipeline assumes it throughout: only `main` updates the baseline, and the
// tracker's own workflow runs default-branch code.
const DEFAULT_BRANCH = "main";

// Either the Bencher branch a PR must compare against, or why none exists.
type Baseline = { startPoint: string; hash: string } | { reason: string };

interface PullRequest {
  number: number;
  state: string;
  head: { sha: string; ref: string; repo: { full_name: string } };
  base: { sha: string; ref: string; repo: { full_name: string } };
}

// A stacked PR's base branch is the head of its parent PR. Bencher stores a
// PR's samples under `pr-<number>`, never under the branch name, so a child
// must name the parent's number or Bencher silently records no comparison.
async function baselineFor(
  { github, context, core }: ActionArguments,
  pr: PullRequest,
): Promise<Baseline> {
  const repo = context.repo;
  if (pr.base.ref === DEFAULT_BRANCH) return { startPoint: DEFAULT_BRANCH, hash: pr.base.sha };
  const { data: candidates } = await github.rest.pulls.list({
    ...repo,
    state: "all",
    head: `${repo.owner}:${pr.base.ref}`,
    per_page: 100,
  });
  const fullName = `${repo.owner}/${repo.repo}`;
  const parents = candidates.filter(
    (candidate) => candidate.head.repo.full_name === fullName && candidate.head.ref === pr.base.ref,
  );
  const open = parents.filter((candidate) => candidate.state === "open");
  // Several PRs from one branch are ambiguous; one open PR wins over a closed
  // one because it is the stack a reviewer sees.
  const parent =
    open.length === 1 ? open[0] : open.length === 0 && parents.length === 1 ? parents[0] : null;
  if (!parent) {
    const reason = parents.length
      ? `several pull requests use ${pr.base.ref} as their head`
      : `the base branch ${pr.base.ref} has no pull request`;
    core.notice(`No baseline for the benchmark: ${reason}.`);
    return { reason };
  }
  return { startPoint: `pr-${parent.number}`, hash: pr.base.sha };
}

export async function resolveSource({ github, context, core }: ActionArguments) {
  const run = context.payload.workflow_run as SourceRun;
  const repo = context.repo;
  const fullName = `${repo.owner}/${repo.repo}`;
  const { data: workflow } = await github.rest.actions.getWorkflow({
    ...repo,
    workflow_id: run.workflow_id,
  });
  if (workflow.path !== ".github/workflows/bench-build.yml") {
    throw new Error("Unexpected source workflow");
  }

  if (run.event === "push" || run.event === "workflow_dispatch") {
    if (run.head_branch !== "main" || run.head_repository.full_name !== fullName) {
      throw new Error("Only main can update the baseline");
    }
    core.setOutput("branch", "main");
    core.setOutput("hash", run.head_sha);
    core.setOutput("eligible", "true");
    return;
  }
  if (run.event !== "pull_request") {
    throw new Error("Unexpected source event");
  }

  // Fork workflow_run events can omit pull_requests; resolve the current head
  // and target repository through GitHub's commit association instead.
  const pulls = await github.paginate(github.rest.repos.listPullRequestsAssociatedWithCommit, {
    ...repo,
    commit_sha: run.head_sha,
    per_page: 100,
  });
  const matching = pulls.filter(
    (pr) =>
      pr.state === "open" && pr.head.sha === run.head_sha && pr.base.repo.full_name === fullName,
  );
  if (matching.length === 0) {
    core.notice("PR closed or superseded; no benchmark will be submitted.");
    return;
  }
  if (matching.length !== 1) {
    throw new Error("Ambiguous PR for benchmark commit");
  }
  const pr = matching[0];
  core.setOutput("branch", `pr-${pr.number}`);
  core.setOutput("hash", pr.head.sha);
  // The GitHub base ref is what the report shows; the start point is the
  // Bencher branch that holds the samples.
  core.setOutput("base", pr.base.ref);
  core.setOutput("number", String(pr.number));
  core.setOutput("eligible", "true");
  const baseline = await baselineFor({ github, context, core }, pr);
  if ("reason" in baseline) {
    core.setOutput("baseline_reason", baseline.reason);
  } else {
    core.setOutput("start_point", baseline.startPoint);
    core.setOutput("base_hash", baseline.hash);
  }
}

export async function startReport({
  github,
  context,
  core,
  hash,
  variant,
}: ActionArguments & { hash: string; variant: Variant }) {
  const { data } = await github.rest.checks.create({
    ...context.repo,
    name: `Benchmarks (rust, ${variant.name})`,
    head_sha: hash,
    status: "in_progress",
    details_url: workflowUrl(context),
  });
  core.setOutput(variant.id, String(data.id));
}

export function workflowUrl(context: ActionContext) {
  const { owner, repo } = context.repo;
  return `${context.serverUrl}/${owner}/${repo}/actions/runs/${context.runId}`;
}

const MARKER = "tessera-bench-report";

export function marker(language: string, hash: string) {
  return `<!-- ${MARKER} language=${language} sha=${hash} -->`;
}

interface Comment {
  id: number;
  node_id: string;
  user: { login: string } | null;
  body?: string;
}

function ownComment(comment: Comment, language: string) {
  return (
    comment.user?.login === "github-actions[bot]" &&
    (comment.body?.includes(`<!-- ${MARKER} language=${language} `) ||
      comment.body?.includes("Bencher Report</a></h2>"))
  );
}

interface CommentOptions {
  github: GitHub;
  repo: Repository;
  number: number;
  language: string;
  hash: string;
  body: string;
  core: Pick<ActionsCore, "notice">;
}

export async function publishComment({
  github,
  repo,
  number,
  language,
  hash,
  body,
  core,
}: CommentOptions) {
  const { data: pr } = await github.rest.pulls.get({ ...repo, pull_number: number });
  if (pr.state !== "open" || pr.head.sha !== hash) {
    core.notice(
      `PR #${number} moved past ${hash.slice(0, 7)}; the report stays in the job summary.`,
    );
    return;
  }
  const comments = (
    await github.paginate(github.rest.issues.listComments, {
      ...repo,
      issue_number: number,
      per_page: 100,
    })
  ).filter((c) => ownComment(c, language));
  const tag = marker(language, hash);
  const full = `${tag}\n${body}`;
  const same = comments.find((c) => c.body?.startsWith(tag));
  if (same) {
    await github.rest.issues.updateComment({ ...repo, comment_id: same.id, body: full });
    const { node } = await github.graphql<{ node: { isMinimized: boolean } }>(
      "query($id: ID!) { node(id: $id) { ... on IssueComment { isMinimized } } }",
      { id: same.node_id },
    );
    if (node.isMinimized) {
      await github.graphql(
        "mutation($id: ID!) { unminimizeComment(input: {subjectId: $id}) { clientMutationId } }",
        { id: same.node_id },
      );
    }
  } else {
    await github.rest.issues.createComment({ ...repo, issue_number: number, body: full });
  }
  for (const c of comments) {
    if (c === same) continue;
    await github.graphql(
      "mutation($id: ID!) { minimizeComment(input: {subjectId: $id, classifier: OUTDATED}) { clientMutationId } }",
      { id: c.node_id },
    );
  }
}
