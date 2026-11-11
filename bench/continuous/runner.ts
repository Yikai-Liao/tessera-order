// SPDX-License-Identifier: BSD-2-Clause

import { mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { CommandCancelledError, command, diagnostic } from "../../tools/process.ts";
import { VARIANTS } from "../catalog.ts";
import { type PairedSource, pairedBmf, readPaired, validateCloud } from "./paired.ts";

export interface SubmissionOptions extends PairedSource {
  artifacts: string;
  prepared: string;
  project: string;
  out: string;
  pr?: string;
}

/** Upload validated JSON only; no executable, image, historical baseline or remote job.
 *
 * Each configuration is attempted independently. Cloud history stores the ratio
 * to this run's exact baseline, not a comparison with a previous run's ratio.
 * The PR's reports share one branch history, so merging main does not start a new one.
 * New Testbeds and a dedicated Measure isolate the old absolute-time histories.
 */
export async function submitContinuous(options: SubmissionOptions) {
  if (!/^[a-f0-9]{40}$/.test(options.hash) || !/^[a-f0-9]{40}$/.test(options.baseHash))
    throw new Error("Expected exact source and baseline SHAs");
  if (options.pr && !/^[1-9]\d*$/.test(options.pr))
    throw new Error("PR number must be a positive integer");
  if (!process.env.BENCHER_API_KEY) throw new Error("BENCHER_API_KEY is required for upload");
  await mkdir(options.out, { recursive: false });
  const outcomes: Record<string, "success" | "failure"> = {};
  for (const variant of VARIANTS) {
    try {
      const runs = await readPaired(options.artifacts, variant, options, options.prepared);
      const file = resolve(options.out, `bmf-${variant.id}.json`);
      await writeFile(file, `${JSON.stringify(pairedBmf(runs))}\n`);
      const report = await command(
        "bencher",
        [
          "run",
          "--project",
          options.project,
          "--branch",
          options.pr ? `pr-${options.pr}` : "main",
          "--hash",
          options.hash,
          "--testbed",
          variant.relativeTestbed,
          "--adapter",
          "json",
          "--file",
          file,
          "--format",
          "json",
          "--quiet",
        ],
        { capture: true },
      );
      await writeFile(join(options.out, `bencher-${variant.id}.json`), `${report}\n`);
      validateCloud(JSON.parse(report), runs, variant, options.pr);
      outcomes[variant.id] = "success";
    } catch (error) {
      if (error instanceof CommandCancelledError) throw error;
      diagnostic(`${variant.name}: ${error instanceof Error ? error.message : String(error)}`);
      outcomes[variant.id] = "failure";
    }
    await writeFile(join(options.out, "outcomes.json"), `${JSON.stringify(outcomes)}\n`);
  }
  return outcomes;
}
