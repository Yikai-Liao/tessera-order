// SPDX-License-Identifier: BSD-2-Clause

import type { Command } from "commander";
import { runTitleCheck } from "../.github/scripts/check_pr_title.ts";
import { runDcoCheck } from "../.github/scripts/check-dco.ts";
import { routeChanges } from "../.github/scripts/ci_changes.ts";
import { verifyRustRelease } from "../.github/scripts/release.ts";
import { fetchNative } from "../bench/rust/native/fetch.ts";
import { repositoryRoot } from "../bench/workspace.ts";

/** Register repository checks and native-source operations, not CI scheduling policy. */
export function repositoryCommands(program: Command) {
  const ci = program.command("ci").description("Reproduce change routing, PR title and DCO checks");
  ci.command("changes")
    .option("--base <commit>", "base commit; unavailable history checks every language")
    .option("--head <commit>", "head commit", "HEAD")
    .option("--out <file>", "append GitHub Actions key=value outputs")
    .action(async (options: { base?: string; head: string; out?: string }) => {
      console.log(JSON.stringify(await routeChanges(options.base, options.head, options.out)));
    });
  ci.command("title [title]")
    .description("Validate a squash title (default: PR_TITLE)")
    .action((title?: string) => {
      runTitleCheck(title ?? process.env.PR_TITLE ?? "");
    });
  ci.command("dco <base> <head>")
    .description("Check sign-offs; authenticated Dependabot exceptions fail closed")
    .action(async (base: string, head: string) => {
      await runDcoCheck(base, head);
    });

  program
    .command("native")
    .description("Manage pinned native benchmark sources")
    .command("fetch")
    .description("Fetch and verify archives, licenses and source trees")
    .action(async () => console.log(JSON.stringify(await fetchNative(await repositoryRoot()))));

  const release = program
    .command("release")
    .description("Validate release inputs without publishing");
  release
    .command("verify")
    .requiredOption("--tag <tag>", "release tag")
    .action(async (options: { tag: string }) =>
      console.log(JSON.stringify(await verifyRustRelease(options.tag, await repositoryRoot()))),
    );
}
