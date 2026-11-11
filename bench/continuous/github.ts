// SPDX-License-Identifier: BSD-2-Clause

import type { context, getOctokit } from "@actions/github";
import { BENCHMARK_CHECK, MARKER, marker } from "./marker.ts";

export type GitHub = ReturnType<typeof getOctokit>;
export type ActionContext = Pick<typeof context, "repo" | "serverUrl" | "runId" | "payload">;
type Repository = ActionContext["repo"];

export interface ActionsCore {
  notice(message: string): void;
  summary: { addRaw(text: string): { write(): Promise<unknown> } };
}

/** Start the single-backend check on the measured PR head. */
export async function startReport({
  github,
  context,
  hash,
}: {
  github: GitHub;
  context: ActionContext;
  hash: string;
}) {
  const { data } = await github.rest.checks.create({
    ...context.repo,
    name: BENCHMARK_CHECK,
    head_sha: hash,
    external_id: String(context.runId),
    status: "in_progress",
    details_url: workflowUrl(context),
  });
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
