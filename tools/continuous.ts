// SPDX-License-Identifier: BSD-2-Clause

import { resolve } from "node:path";
import type { Command } from "commander";
import { continuousShards } from "../bench/continuous/paired.ts";
import { type SubmissionOptions, submitContinuous } from "../bench/continuous/runner.ts";
import { selectContinuous } from "../bench/continuous/selection.ts";
import { readPrepared } from "../bench/local/model.ts";
import { compareLocal, comparePrepared } from "../bench/local/runner.ts";
import { loadPlan } from "../bench/plan.ts";
import { parseLogicalCpu, parseShard } from "./options.ts";
import { repositoryRoot, resultDirectory, runTask } from "./tasks.ts";

/** Register the build, measurement and JSON-only publication stages. */
export function continuousCommands(program: Command) {
  const continuous = program
    .command("continuous")
    .description("Prepare, measure and submit continuous revision comparisons");
  continuous
    .command("matrix <directory>")
    .description("Print the hosted measurement matrix for a frozen preparation")
    .action(async (directory: string) => {
      const count = continuousShards((await readPrepared(resolve(directory))).cases);
      console.log(
        JSON.stringify({
          include: Array.from({ length: count }, (_, index) => ({
            shard: `${index + 1}/${count}`,
            artifact: `paired-${index + 1}`,
          })),
        }),
      );
    });
  continuous
    .command("prepare [revision]")
    .description("Freeze a revision pair without timing (baseline: HEAD; plan: continuous)")
    .option("--plan <name-or-file>", "JSON/TOML selection or local/continuous preset", "continuous")
    .option("--groups <names>", "comma-separated operation kinds, sort, parallel, or all", "all")
    .option("--out <directory>", "new prepared directory (default: bench/results/<timestamp>)")
    .action(
      async (
        baseline: string | undefined,
        options: { plan: string; groups: string; out?: string },
      ) => {
        const plan = selectContinuous(await loadPlan(options.plan), options.groups);
        const root = await repositoryRoot();
        const out = resultDirectory(root, options.out);
        await runTask(out, Boolean(program.opts().verbose), () =>
          compareLocal({ root, baseline: baseline ?? "HEAD", plan, out, prepareOnly: true }),
        );
      },
    );
  continuous
    .command("measure <directory>")
    .description("Measure and report a frozen revision pair without compiling")
    .option("--out <directory>", "new result directory (default: bench/results/<timestamp>)")
    .option("--cpu <list>", "pin measurement to a CPU list (Linux)", parseLogicalCpu)
    .option("--shard <index/count>", "measure one complete-case partition", parseShard)
    .action(
      async (
        directory: string,
        options: {
          out?: string;
          cpu?: string;
          shard?: { index: number; count: number };
        },
      ) => {
        const root = await repositoryRoot();
        const out = resultDirectory(root, options.out);
        await runTask(out, Boolean(program.opts().verbose), () =>
          comparePrepared({
            root,
            prepared: resolve(directory),
            out,
            cpu: options.cpu,
            shard: options.shard,
          }),
        );
      },
    );
  continuous
    .command("submit")
    .description("Upload complete paired shards; never executes measured code")
    .requiredOption("--prepared <directory>", "this run’s prepared revision pair")
    .requiredOption("--artifacts <directory>", "downloaded paired-* artifact directories")
    .requiredOption("--hash <sha>", "exact measured source commit")
    .requiredOption("--base-hash <sha>", "exact same-run baseline commit")
    .requiredOption("--project <id>", "Bencher project")
    .requiredOption("--out <directory>", "new report directory")
    .option("--pr <number>", "record under pr-N rather than main")
    .action(async (options: SubmissionOptions) => {
      const out = resolve(options.out);
      await runTask(out, Boolean(program.opts().verbose), () =>
        submitContinuous({ ...options, out, artifacts: resolve(options.artifacts) }),
      );
    });
}
