// SPDX-License-Identifier: BSD-2-Clause

import { readFile } from "node:fs/promises";

export const BOT_EMAIL = "49699333+dependabot[bot]@users.noreply.github.com";

/** Treat malformed API and event values as absent, never as authenticated identity. */
function object(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function bot(value: unknown) {
  const account = object(value);
  return account.id === 49699333 && account.login === "dependabot[bot]" && account.type === "Bot";
}

/** Require an authenticated bot update of this exact same-repository PR head. */
export function isDependabotUpdate(value: unknown, sha: string, repository: string) {
  const event = object(value);
  const pull = object(event.pull_request);
  const source = object(event.repository);
  const head = object(pull.head);
  const base = object(pull.base);
  return (
    ["opened", "synchronize"].includes(String(event.action)) &&
    (event.action !== "synchronize" || event.after === sha) &&
    bot(event.sender) &&
    bot(pull.user) &&
    source.full_name === repository &&
    typeof source.id === "number" &&
    Number.isSafeInteger(source.id) &&
    source.id > 0 &&
    object(head.repo).id === source.id &&
    object(base.repo).id === source.id &&
    head.sha === sha
  );
}

/** Require GitHub attribution and a valid signature on the exact commit. */
export function isVerifiedDependabot(value: unknown, sha: string) {
  const commit = object(value);
  const metadata = object(commit.commit);
  const committer = object(commit.committer);
  const verification = object(metadata.verification);
  return (
    commit.sha === sha &&
    bot(commit.author) &&
    committer.id === 19864447 &&
    committer.login === "web-flow" &&
    object(metadata.author).email === BOT_EMAIL &&
    object(metadata.committer).email === "noreply@github.com" &&
    verification.verified === true &&
    verification.reason === "valid"
  );
}

/** Verify the update event before querying GitHub; any error denies the DCO exception. */
export async function verifyCommit(
  sha: string,
  env: NodeJS.ProcessEnv = process.env,
  fetcher: typeof fetch = fetch,
) {
  const repository = env.GITHUB_REPOSITORY ?? "";
  if (
    !/^[0-9a-f]{40}$/.test(sha) ||
    !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository) ||
    !env.GH_TOKEN ||
    env.GITHUB_EVENT_NAME !== "pull_request" ||
    env.GITHUB_ACTOR_ID !== "49699333"
  )
    return false;
  try {
    const event: unknown = JSON.parse(await readFile(env.GITHUB_EVENT_PATH ?? "", "utf8"));
    if (!isDependabotUpdate(event, sha, repository)) return false;
    const response = await fetcher(`https://api.github.com/repos/${repository}/commits/${sha}`, {
      headers: {
        Authorization: `Bearer ${env.GH_TOKEN}`,
        Accept: "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
      },
      signal: AbortSignal.timeout(15_000),
    });
    return response.ok && isVerifiedDependabot(await response.json(), sha);
  } catch {
    // An unavailable API grants no exception. Do not expose requests or credentials.
    return false;
  }
}
