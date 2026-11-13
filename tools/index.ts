// SPDX-License-Identifier: BSD-2-Clause

import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { Command, InvalidArgumentError } from "commander";
import { codeCommands } from "./code.ts";
import { continuousCommands } from "./continuous.ts";
import { experimentCommands } from "./experiments.ts";
import { repositoryCommands } from "./repository.ts";
import { formatStatus, readStatus } from "./status.ts";
import { repositoryRoot } from "./tasks.ts";

/** Construct the task-oriented command surface without executing an operation. */
export function createProgram() {
  const program = new Command()
    .name("bun run tools")
    .description("Experiments, continuous benchmark reproduction and CI feedback")
    .option("--verbose", "stream subprocess logs instead of retaining them in a file")
    .exitOverride();

  experimentCommands(program);
  continuousCommands(program);
  repositoryCommands(program);
  codeCommands(program);
  program
    .command("status [ref]")
    .description("Read compact CI and benchmark status for a remote ref (default: main) or PR")
    .option("--pr <number>", "read the PR head and its test merge workflows")
    .option("--details", "include full check summaries, report body and workflow links")
    .option("--text", "print a human-readable overview instead of compact JSON")
    .action(
      async (
        ref: string | undefined,
        options: { pr?: string; details?: boolean; text?: boolean },
      ) => {
        if (ref && options.pr) throw new InvalidArgumentError("choose a ref or --pr, not both");
        const status = await readStatus(
          options.pr ? { pr: options.pr } : { ref: ref ?? "main" },
          await repositoryRoot(),
          Boolean(options.details),
        );
        console.log(options.text ? formatStatus(status) : JSON.stringify(status));
      },
    );
  return program;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  createProgram()
    .parseAsync()
    .catch((error: unknown) => {
      if (
        error instanceof Error &&
        "code" in error &&
        ["commander.helpDisplayed", "commander.help"].includes(String(error.code))
      )
        return;
      if (
        !(error instanceof Error && "code" in error && String(error.code).startsWith("commander."))
      )
        console.error(
          JSON.stringify({ error: error instanceof Error ? error.message : String(error) }),
        );
      process.exitCode = 1;
    });
}
