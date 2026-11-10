// SPDX-License-Identifier: BSD-2-Clause

import { resolve } from "node:path";
import type { Command } from "commander";
import {
  buildContinuous,
  type MeasurementOptions,
  measureContinuous,
  runContinuous,
} from "../bench/continuous/runner.ts";
import { readMainHistory } from "../bench/continuous/volatility.ts";
import { probeCounters } from "../bench/local/counters.ts";
import { loadPlan } from "../bench/plan.ts";
import { repositoryRoot, resultDirectory, runTask } from "./tasks.ts";

/** Register the continuous protocol's local and explicit remote operations. */
export function continuousCommands(program: Command) {
  const continuous = program
    .command("continuous")
    .description("Build, reproduce or explicitly submit the continuous benchmark protocol");

  continuous
    .command("probe-counters")
    .description("Probe Linux PMU events on the current host; never changes permissions")
    .requiredOption("--out <directory>", "new probe directory")
    .action(async (options: { out: string }) => {
      const out = resolve(options.out);
      await runTask(out, Boolean(program.opts().verbose), () => probeCounters(out));
    });

  continuous
    .command("history")
    .description("Read adjacent main changes in both existing Testbeds without changing thresholds")
    .requiredOption("--project <slug>", "public Bencher project")
    .requiredOption("--out <directory>", "new retained history directory")
    .action(async (options: { project: string; out: string }) => {
      const out = resolve(options.out);
      await runTask(out, Boolean(program.opts().verbose), () =>
        readMainHistory(options.project, out),
      );
    });

  continuous
    .command("build")
    .description("Build both static Linux x86-64 artifacts exactly as CI does")
    .requiredOption("--out <directory>", "new artifact directory")
    .action(async (options: { out: string }) => {
      const root = await repositoryRoot();
      const out = resolve(options.out);
      await runTask(out, Boolean(program.opts().verbose), () => buildContinuous(root, out));
    });

  continuous
    .command("run <executable>")
    .description("Run the cloud's two-CPU scheduler locally (Linux; no credentials)")
    .option("--out <directory>", "new output directory (default: bench/results/<timestamp>)")
    .option("--plan <name-or-file>", "local override of the executable's compiled continuous plan")
    .action(async (executable: string, options: { out?: string; plan?: string }) => {
      const root = await repositoryRoot();
      const out = resultDirectory(root, options.out);
      const plan = options.plan ? await loadPlan(options.plan) : undefined;
      await runTask(out, Boolean(program.opts().verbose), () =>
        runContinuous(root, executable, out, plan),
      );
    });

  continuous
    .command("submit")
    .description("Submit the SIMD build to Bencher; requires its CLI and BENCHER_API_KEY")
    .requiredOption("--image <digest>", "immutable image reference containing the SIMD build")
    .requiredOption("--hash <sha>", "measured source commit")
    .requiredOption("--project <id>", "Bencher project")
    .requiredOption("--out <directory>", "new report directory")
    .option("--pr <number>", "record under pr-N rather than main")
    .option("--start-point <branch>", "Bencher baseline branch, including a parent pr-N")
    .option("--base-hash <sha>", "exact baseline commit; requires --start-point")
    .action(async (options: MeasurementOptions) => {
      const out = resolve(options.out);
      const outcomes = await runTask(out, Boolean(program.opts().verbose), () =>
        measureContinuous({ ...options, out }),
      );
      if (Object.values(outcomes).includes("failure")) process.exitCode = 1;
    });
}
