// SPDX-License-Identifier: BSD-2-Clause

import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { BOT_EMAIL, verifyCommit } from "./verify_dependabot.ts";

/** Check every non-merge commit, applying authenticated exceptions per commit only. */
export async function checkDco(
  base: string,
  head: string,
  verify: (sha: string) => Promise<boolean> = verifyCommit,
) {
  const git = (...args: string[]) =>
    execFileSync("git", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  const missing: { sha: string; email: string }[] = [];
  for (const sha of git("rev-list", "--no-merges", `${base}..${head}`)
    .split("\n")
    .filter(Boolean)) {
    const email = git("show", "-s", "--format=%ae", sha);
    const signoffs = git("show", "-s", "--format=%(trailers:key=Signed-off-by,valueonly)", sha);
    if (signoffs.toLowerCase().includes(`<${email.toLowerCase()}>`)) continue;
    if (email === BOT_EMAIL && (await verify(sha))) continue;
    missing.push({ sha, email });
  }
  return missing;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [base, head] = process.argv.slice(2);
  if (!base || !head) throw new Error("Usage: check-dco.ts BASE HEAD");
  const missing = await checkDco(base, head);
  console.log(JSON.stringify({ missing }));
  if (missing.length) process.exitCode = 1;
}
