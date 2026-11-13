// SPDX-License-Identifier: BSD-2-Clause

import type { Command } from "commander";
import { codeChange, codeTable } from "../bench/code-change.ts";
import { repositoryRoot } from "./tasks.ts";

/** Inspect core engine production lines without building or measuring code. */
export function codeCommands(program: Command) {
  program
    .command("code [baseline]")
    .description("Compare core engine production lines with a commit (default: HEAD)")
    .option("--current <commit>", "compare a committed revision instead of the working tree")
    .option("--details", "include every source file's line counts")
    .option("--text", "print the same compact table used by PR reports")
    .action(
      async (
        baseline: string | undefined,
        options: { current?: string; details?: boolean; text?: boolean },
      ) => {
        const change = await codeChange(
          baseline ?? "HEAD",
          options.current,
          await repositoryRoot(),
        );
        if (options.text) {
          console.log(
            codeTable(change, {
              baseline: baseline ?? "HEAD",
              current: options.current ?? "Working tree",
              details: options.details,
            }),
          );
        } else {
          const { files, ...summary } = change;
          console.log(
            JSON.stringify({
              ...summary,
              net: change.current - change.baseline,
              ...(options.details ? { files } : {}),
            }),
          );
        }
      },
    );
}
