// SPDX-License-Identifier: BSD-2-Clause

import type { context, getOctokit } from "@actions/github";
import type { Variant } from "../catalog.ts";
import { MARKER, marker } from "./marker.ts";

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
  display_title: string;
  event: string;
  head_sha: string;
  head_branch: string;
  head_repository: { full_name: string };
}

export interface ActionArguments {
  github: GitHub;
  context: ActionContext;
  core: Pick<ActionsCore, "notice" | "setOutput">;
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
    const { data: commit } = await github.rest.repos.getCommit({ ...repo, ref: run.head_sha });
    const baseline = commit.parents[0]?.sha;
    if (!baseline) throw new Error("Main has no parent baseline");
    core.setOutput("base_hash", baseline);
    core.setOutput("branch", "main");
    core.setOutput("hash", run.head_sha);
    core.setOutput("eligible", "true");
    return;
  }
  if (run.event !== "pull_request") {
    throw new Error("Unexpected source event");
  }

  // Native run metadata snapshots the event; live PR state only determines
  // eligibility. This also works when fork events omit pull_requests entirely.
  const captured = /^paired-v1 pr=([1-9]\d*) base=([a-f0-9]{40}) head=([a-f0-9]{40})$/.exec(
    run.display_title,
  );
  if (!captured || captured[3] !== run.head_sha)
    throw new Error("Missing or mismatched triggering PR identity");
  const [, number, target] = captured;
  const { data: pr } = await github.rest.pulls.get({ ...repo, pull_number: Number(number) });
  if (pr.state !== "open" || pr.head.sha !== run.head_sha || pr.base.repo.full_name !== fullName) {
    core.notice("PR closed or superseded; no benchmark will be submitted.");
    return;
  }
  core.setOutput("branch", `pr-${pr.number}`);
  core.setOutput("hash", pr.head.sha);
  // The live ref may have been retargeted; display only the recorded baseline.
  core.setOutput("base", "recorded target");
  core.setOutput("number", String(pr.number));
  core.setOutput("eligible", "true");
  const { data: comparison } = await github.rest.repos.compareCommits({
    ...repo,
    base: target,
    head: pr.head.sha,
  });
  core.setOutput("base_hash", comparison.merge_base_commit.sha);
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
    external_id: String(context.runId),
    status: "in_progress",
    details_url: workflowUrl(context),
  });
  core.setOutput(variant.id, String(data.id));
  return data.id;
}

export function workflowUrl(context: ActionContext) {
  const { owner, repo } = context.repo;
  return `${context.serverUrl}/${owner}/${repo}/actions/runs/${context.runId}`;
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
