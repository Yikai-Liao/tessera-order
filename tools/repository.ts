// SPDX-License-Identifier: BSD-2-Clause

import { resolve } from "node:path";
import type { Command } from "commander";
import { titleErrors } from "../.github/scripts/check_pr_title.ts";
import { checkDco } from "../.github/scripts/check-dco.ts";
import { routeChanges } from "../.github/scripts/ci_changes.ts";
import { collectWheels, verifyRelease } from "../.github/scripts/release.ts";
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
      const errors = titleErrors(title ?? process.env.PR_TITLE ?? "");
      console.log(JSON.stringify({ errors }));
      if (errors.length) process.exitCode = 1;
    });
  ci.command("dco <base> <head>")
    .description("Check sign-offs; authenticated Dependabot exceptions fail closed")
    .action(async (base: string, head: string) => {
      const missing = await checkDco(base, head);
      console.log(JSON.stringify({ missing }));
      if (missing.length) process.exitCode = 1;
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
    .command("verify <language>")
    .requiredOption("--tag <tag>", "release tag")
    .action(async (language: string, options: { tag: string }) =>
      console.log(
        JSON.stringify(await verifyRelease(language, options.tag, await repositoryRoot())),
      ),
    );
  release
    .command("collect-wheels <source>")
    .requiredOption("--out <directory>", "new wheel directory")
    .action(async (source: string, options: { out: string }) =>
      console.log(JSON.stringify(await collectWheels(resolve(source), resolve(options.out)))),
    );
}
