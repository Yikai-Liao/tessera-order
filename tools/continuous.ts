// SPDX-License-Identifier: BSD-2-Clause

import { resolve } from "node:path";
import type { Command } from "commander";
import { parseCriterionId, revisionSampling } from "../bench/catalog.ts";
import { CONTINUOUS_SHARDS } from "../bench/continuous/paired.ts";
import { type SubmissionOptions, submitContinuous } from "../bench/continuous/runner.ts";
import { prepareGo } from "../bench/local/go.ts";
import { readPrepared } from "../bench/local/model.ts";
import { compareLocal, comparePrepared } from "../bench/local/runner.ts";
import { loadPlan } from "../bench/plan.ts";
import { parseLogicalCpu, parseShard } from "./options.ts";
import { repositoryRoot, resultDirectory, runTask } from "./tasks.ts";

export function goShardCount(ids: string[]) {
  const nominalMs = ids.reduce((total, id) => {
    const { warm_ms, window_ms } = revisionSampling(parseCriterionId(id).len);
    return total + 2 * (warm_ms + window_ms);
  }, 0);
  return Math.min(4, Math.max(1, Math.ceil(nominalMs / 120_000)), Math.max(1, ids.length));
}

/** Register the build, measurement and JSON-only publication stages. */
export function continuousCommands(program: Command) {
  const continuous = program
    .command("continuous")
    .description("Prepare, measure and submit continuous revision comparisons");
  continuous
    .command("matrix")
    .description("Print the hosted measurement matrix without building or timing")
    .option("--go-prepared <directory>", "size the Go matrix from its frozen inventory")
    .action(async (options: { goPrepared?: string }) => {
      const count = options.goPrepared
        ? goShardCount((await readPrepared(options.goPrepared)).cases)
        : CONTINUOUS_SHARDS;
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
    .option("--out <directory>", "new prepared directory (default: bench/results/<timestamp>)")
    .option("--language <language>", "rust or go", "rust")
    .action(
      async (
        baseline: string | undefined,
        options: { plan: string; out?: string; language: string },
      ) => {
        const plan = await loadPlan(options.plan);
        const root = await repositoryRoot();
        const out = resultDirectory(root, options.out);
        if (options.language !== "go" && options.language !== "rust")
          throw new Error("Unsupported benchmark language");
        const revision = baseline ?? "HEAD";
        const prepare =
          options.language === "go"
            ? () => prepareGo(root, revision, out, { ...plan, implementations: ["go-tessera"] })
            : () => compareLocal({ root, baseline: revision, plan, out, prepareOnly: true });
        await runTask(out, Boolean(program.opts().verbose), async () => await prepare());
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
    .option("--language <language>", "rust or go", "rust")
    .action(async (options: SubmissionOptions) => {
      const out = resolve(options.out);
      await runTask(out, Boolean(program.opts().verbose), () =>
        submitContinuous({ ...options, out, artifacts: resolve(options.artifacts) }),
      );
    });
}
