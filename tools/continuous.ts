// SPDX-License-Identifier: BSD-2-Clause

import { resolve } from "node:path";
import type { Command } from "commander";
import { VARIANTS } from "../bench/catalog.ts";
import { CONTINUOUS_SHARDS } from "../bench/continuous/paired.ts";
import { type SubmissionOptions, submitContinuous } from "../bench/continuous/runner.ts";
import { readMainHistory } from "../bench/continuous/volatility.ts";
import { probeCounters } from "../bench/local/counters.ts";
import { runTask } from "./tasks.ts";

/** Register read-only diagnostics and explicit JSON-only Cloud publication. */
export function continuousCommands(program: Command) {
  const continuous = program
    .command("continuous")
    .description("Read history or upload paired results");
  continuous
    .command("matrix")
    .description("Print the hosted measurement matrix without building or timing")
    .action(() =>
      console.log(
        JSON.stringify({
          include: VARIANTS.flatMap((variant) =>
            Array.from({ length: CONTINUOUS_SHARDS }, (_, index) => ({
              configuration: variant.id,
              shard: `${index + 1}/${CONTINUOUS_SHARDS}`,
              artifact: `paired-${variant.id}-${index + 1}`,
            })),
          ),
        }),
      ),
    );
  continuous
    .command("probe-counters")
    .description("Probe Linux PMU events; never changes permissions")
    .requiredOption("--out <directory>", "new probe directory")
    .action(async (options: { out: string }) => {
      const out = resolve(options.out);
      await runTask(out, Boolean(program.opts().verbose), () => probeCounters(out));
    });
  continuous
    .command("history")
    .description("Read legacy absolute-time main history without changing thresholds")
    .requiredOption("--project <slug>", "public Bencher project")
    .requiredOption("--out <directory>", "new retained history directory")
    .action(async (options: { project: string; out: string }) => {
      const out = resolve(options.out);
      await runTask(out, Boolean(program.opts().verbose), () =>
        readMainHistory(options.project, out),
      );
    });
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
      const outcomes = await runTask(out, Boolean(program.opts().verbose), () =>
        submitContinuous({ ...options, out, artifacts: resolve(options.artifacts) }),
      );
      if (Object.values(outcomes).includes("failure")) process.exitCode = 1;
    });
}
