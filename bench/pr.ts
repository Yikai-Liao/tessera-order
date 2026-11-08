// SPDX-License-Identifier: BSD-2-Clause

import { marker } from "./continuous/github.ts";
import { command } from "./process.ts";

interface Check {
  name: string;
  status: string;
  conclusion: string | null;
  details_url: string;
  output: { title: string | null; summary: string | null };
}

interface Comment {
  body: string;
  html_url: string;
  user: { login: string } | null;
}

export interface PullFeedback {
  number: number;
  head: string;
  url: string;
  checks: Check[];
  comment: { url: string; body: string } | null;
}

/** Select benchmark feedback attached to the current PR head. */
export function selectFeedback(
  pull: { number: number; head: { sha: string }; html_url: string },
  checks: Check[],
  comments: Comment[],
): PullFeedback {
  const current = comments
    .filter(
      (comment) =>
        comment.user?.login === "github-actions[bot]" &&
        comment.body.includes(marker("rust", pull.head.sha)),
    )
    .at(-1);
  return {
    number: pull.number,
    head: pull.head.sha,
    url: pull.html_url,
    checks: checks.filter((check) => check.name.startsWith("Benchmarks (")),
    comment: current ? { url: current.html_url, body: current.body } : null,
  };
}

/** Read checks and the current-head report through an authenticated GitHub CLI. */
export async function pullFeedback(number: string, root: string): Promise<PullFeedback> {
  if (!/^[1-9]\d*$/.test(number)) throw new Error("PR number must be a positive integer");
  const repo = await command(
    "gh",
    ["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
    { cwd: root, capture: true },
  );
  const api = async (path: string) =>
    JSON.parse(await command("gh", ["api", path], { cwd: root, capture: true }));
  const pull = await api(`repos/${repo}/pulls/${number}`);
  const checks = await api(`repos/${repo}/commits/${pull.head.sha}/check-runs?per_page=100`);
  const pages = JSON.parse(
    await command(
      "gh",
      ["api", "--paginate", "--slurp", `repos/${repo}/issues/${number}/comments?per_page=100`],
      { cwd: root, capture: true },
    ),
  );
  return selectFeedback(pull, checks.check_runs, pages.flat());
}
